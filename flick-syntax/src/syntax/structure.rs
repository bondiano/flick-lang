//! Structural facts of source bodies, shared by function lints and plugins.
use flick_plugin::{Nesting, Span, Structure};

use super::cst::{Element, Node};
use super::kind::SyntaxKind as K;

impl super::Parse {
    /// A qualified file declaration with `= <members>` has the same static
    /// scope as a header followed by declarations. Keep token positions and
    /// leave the parser's lossless representation available to formatters.
    #[must_use]
    pub fn into_module(mut self) -> Self {
        let Some(first) = self.tree.nodes().next() else {
            return self;
        };
        if first.kind != K::InlineModuleDecl
            || !first.nodes().any(|n| {
                n.kind == K::ModuleParamGroup || (n.kind == K::ModulePath && n.has_token(K::Dot))
            })
        {
            return self;
        }
        let mut children = Vec::new();
        for element in self.tree.children {
            match element {
                Element::Node(mut node)
                    if node.kind == K::InlineModuleDecl
                        && children.iter().all(|e| !matches!(e, Element::Node(_))) =>
                {
                    let mut members = Vec::new();
                    node.children.retain(|child| {
                        if let Element::Node(body) = child
                            && body.kind == K::Members
                        {
                            members.clone_from(&body.children);
                            false
                        } else {
                            true
                        }
                    });
                    node.kind = K::ModuleHeader;
                    children.push(Element::Node(node));
                    children.extend(members);
                }
                other => children.push(other),
            }
        }
        self.tree.children = children;
        self
    }
}

pub fn measure(body: &Node, text: &str, limits: Thresholds) -> Structure {
    let (start, end) = body.range();
    let mut nesting = Vec::new();
    depths(body, 0, limits.nesting, &mut nesting);
    Structure {
        lines: u32::try_from(text[start as usize..end as usize].lines().count())
            .unwrap_or(u32::MAX),
        complexity: 1 + branches(body),
        nesting,
        max_lines: limits.function_lines,
        max_complexity: limits.complexity,
        max_nesting: limits.nesting,
    }
}

fn is_function(node: &Node) -> bool {
    node.kind == K::LetDecl && node.nodes().any(|n| n.kind == K::Params)
}

/// A `match` that is a table: every arm is one expression of constants and
/// names, with no guard, `if`, nested `match` or other branch in it. Such a
/// match maps a value to a name; its arms are data, not decisions.
fn is_lookup(matched: &Node) -> bool {
    matched
        .nodes()
        .filter(|n| n.kind == K::MatchArm)
        .all(|arm| {
            !arm.nodes().any(|n| n.kind == K::Guard) && arm.nodes().last().is_some_and(constant)
        })
}

/// Whether `node` is a constant expression: a literal, a name, or an
/// application, list, tuple or field of constants. Any construct that
/// branches makes it false.
fn constant(node: &Node) -> bool {
    match node.kind {
        K::Literal | K::NameExpr | K::Path | K::UnitExpr => true,
        K::Block => {
            let mut bodies = node.nodes();
            matches!((bodies.next(), bodies.next()), (Some(body), None) if constant(body))
        }
        K::ParenExpr | K::AppExpr | K::FieldExpr | K::ListExpr | K::TupleExpr => {
            node.nodes().all(constant)
        }
        _ => false,
    }
}

pub fn branches(node: &Node) -> u32 {
    let mut total = 0;
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        for child in node.nodes().filter(|c| !is_function(c)) {
            total += match child.kind {
                K::IfExpr | K::Guard | K::GuardStmt => 1,
                K::MatchExpr if is_lookup(child) => 0,
                K::MatchExpr => {
                    u32::try_from(child.nodes().filter(|a| a.kind == K::MatchArm).count())
                        .map_or(0, |n| n.saturating_sub(1))
                }
                K::BinExpr if child.has_token(K::AndAnd) || child.has_token(K::OrOr) => 1,
                _ => 0,
            };
            pending.push(child);
        }
    }
    total
}

fn depths(node: &Node, depth: u32, limit: u32, out: &mut Vec<Nesting>) {
    let mut pending = vec![(node, depth)];
    while let Some((node, depth)) = pending.pop() {
        if depth > limit {
            let (start, end) = node
                .child_tokens()
                .next()
                .map_or_else(|| node.range(), |t| (t.start, t.end));
            out.push(Nesting {
                span: Span { start, end },
                depth,
            });
            continue;
        }
        let children = node.nodes().filter(|c| !is_function(c)).collect::<Vec<_>>();
        for child in children.into_iter().rev() {
            let nests = matches!(
                child.kind,
                K::IfExpr
                    | K::MatchExpr
                    | K::FunExpr
                    | K::DoExpr
                    | K::TaskExpr
                    | K::LazyExpr
                    | K::ForExpr
                    | K::TryExpr
                    | K::ResultExpr
            );
            let else_if = node.kind == K::IfExpr && child.kind == K::IfExpr;
            pending.push((child, depth + u32::from(nests && !else_if)));
        }
    }
}

/// `[lint]` thresholds: a function past one gets a warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    pub function_lines: u32,
    pub nesting: u32,
    pub complexity: u32,
    pub params: u32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Thresholds {
            function_lines: 60,
            nesting: 4,
            complexity: 10,
            params: 5,
        }
    }
}
