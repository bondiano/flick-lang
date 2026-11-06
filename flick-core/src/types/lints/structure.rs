//! T036–T039: a function too long, nested too deep, with too many branches
//! or parameters, past the `[lint]` thresholds. U007: a function that reads
//! atoms to build a `Ui`, which a `component` does with its own re-render.

use super::super::Checker;
use super::super::let_parts;
use super::super::ty::Type;
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};

/// The expressions each of which nests what is inside it one level deeper.
const NESTS: [K; 7] = [
    K::IfExpr,
    K::MatchExpr,
    K::FunExpr,
    K::DoExpr,
    K::TaskExpr,
    K::ForExpr,
    K::TryExpr,
];

impl Checker<'_> {
    /// The lints of a function `decl`: a `let` with parameters or a
    /// `component`; other declarations have none.
    pub(super) fn structure(&mut self, decl: &Node) {
        let parts = let_parts(decl, self.text);
        let component = decl
            .nodes()
            .find(|n| n.kind == K::Block)
            .and_then(|b| b.nodes().find(|n| n.kind == K::ComponentExpr));
        let body = match component {
            Some(c) => c.nodes().find(|n| n.kind == K::Block),
            None if !parts.params.is_empty() => parts.body,
            None => None,
        };
        let (Some(body), Some(pattern)) = (body, parts.pattern) else {
            return;
        };
        let name = parts.name.unwrap_or_default();
        let at = self.span(pattern);
        let limits = self.options.thresholds;
        let warn =
            |code, message: String| Diagnostic::new(code, Severity::Warning, message).with_span(at);

        let lines = self.text[body.range().0 as usize..body.range().1 as usize]
            .lines()
            .count();
        if lines > limits.function_lines as usize {
            self.diagnostics.push(
                warn(
                    codes::T036,
                    format!(
                        "`{name}` is {lines} lines long, more than {}",
                        limits.function_lines
                    ),
                )
                .with_fix(|| ("split it into smaller functions", Vec::new())),
            );
        }
        let complexity = 1 + branches(body);
        if complexity > limits.complexity {
            self.diagnostics.push(
                warn(
                    codes::T038,
                    format!(
                        "`{name}` has a complexity of {complexity}, more than {}",
                        limits.complexity
                    ),
                )
                .with_fix(|| {
                    (
                        "move some of its branches into functions of their own",
                        Vec::new(),
                    )
                }),
            );
        }
        let params = parts.params.len();
        if params > limits.params as usize {
            self.diagnostics.push(
                warn(
                    codes::T039,
                    format!(
                        "`{name}` takes {params} parameters, more than {}",
                        limits.params
                    ),
                )
                .with_fix(|| ("take a record of them", Vec::new())),
            );
        }
        self.too_deep(body, 0, limits.nesting);
        if component.is_none() {
            self.reads_atoms(&name, at);
        }
    }

    /// T037 at each expression `depth` levels deep past `limit`, and not
    /// within one already reported.
    fn too_deep(&mut self, node: &Node, depth: u32, limit: u32) {
        for child in node.nodes() {
            if is_function(child) {
                continue;
            }
            // `else if` continues its `if`
            let else_if = node.kind == K::IfExpr && child.kind == K::IfExpr;
            let depth = depth + u32::from(NESTS.contains(&child.kind) && !else_if);
            if depth > limit {
                let at = child
                    .child_tokens()
                    .next()
                    .map_or_else(|| self.span(child), |t| self.token_span(t));
                self.diagnostics.push(
                    Diagnostic::new(
                        codes::T037,
                        Severity::Warning,
                        format!("nested {depth} levels deep, more than {limit}"),
                    )
                    .with_span(at)
                    .with_fix(|| ("move the inner part into a function of its own", Vec::new())),
                );
                continue;
            }
            self.too_deep(child, depth, limit);
        }
    }

    /// U007: a top-level function whose result is a `Ui` it builds from
    /// atoms it reads.
    fn reads_atoms(&mut self, name: &str, at: Span) {
        let Some(mut ty) = self.signed_type(name) else {
            return;
        };
        if let Type::Forall { body, .. } = ty {
            ty = *body;
        }
        let mut reads = false;
        while let Type::Fun {
            effects, result, ..
        } = ty
        {
            if let Some(effects) = effects
                && let Type::Row(row) = &effects.services
            {
                reads |= (row.entries.iter())
                    .any(|(l, _)| Some(l.as_str()) == lang::label(Lang::AtomRead));
            }
            ty = *result;
        }
        if let Type::Captured { ty: inner, .. } = ty {
            ty = *inner;
        }
        if reads && ty.is_lang(Lang::Ui) {
            self.diagnostics.push(
                Diagnostic::new(
                    codes::U007,
                    Severity::Warning,
                    format!("`{name}` reads atoms to build a `Ui`: whatever calls it renders again when they change"),
                )
                .with_span(at).with_fix(|| ("make it a `component`: it renders again on its own, not with its parent", Vec::new())),
            );
        }
    }
}

/// A local function, linted as a function of its own.
fn is_function(node: &Node) -> bool {
    node.kind == K::LetDecl
        && (node.nodes().any(|n| n.kind == K::Params) || node.has_token(K::ComponentKw))
}

/// The branches of `node` apart from local functions: each `if`, each arm
/// after the first, each guard, each `&&` and `||`.
fn branches(node: &Node) -> u32 {
    node.nodes()
        .filter(|c| !is_function(c))
        .map(|c| {
            let own = match c.kind {
                K::IfExpr | K::Guard | K::GuardStmt => 1,
                K::MatchExpr => u32::try_from(c.nodes().filter(|a| a.kind == K::MatchArm).count())
                    .map_or(0, |n| n.saturating_sub(1)),
                K::BinExpr if c.has_token(K::AndAnd) || c.has_token(K::OrOr) => 1,
                _ => 0,
            };
            own + branches(c)
        })
        .sum()
}
