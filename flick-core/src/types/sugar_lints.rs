//! Warnings that point at the sugar saying the same thing more directly,
//! each with its rewrite: FLICK-T029 `do` over `Eff` (a `task`), T030
//! `x <- e; pure x` (`e`), T031 a whole infinite `Seq`, T032
//! `Seq.to_list` of a `for` (`[for …]`) and T033 `List.filter` piped into
//! `List.map` (`[for …]`), T034 a `match` that fails with the error of a
//! `Result` (`?`), T035 a chain of `flat_map` (`do`, in [`chains`]). They
//! run once the module is solved.

mod chains;

use super::Checker;
use super::ty::Type;
use super::typing::Ref;
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};

/// The `Seq` functions whose result never ends.
const INFINITE: [&str; 4] = ["from", "repeat", "iterate", "cycle"];

/// The `Seq` functions whose result ends when their sequence argument does.
const FOLLOWS: [&str; 8] = [
    "map",
    "filter",
    "filter_map",
    "flat_map",
    "scan",
    "enumerate",
    "drop",
    "drop_while",
];

/// Whether `module` holds `List` functions: the module of `List.map` or
/// one of its submodules.
pub(super) fn is_list_module(module: &str) -> bool {
    lang::module_of(Lang::ListMap).is_some_and(|list| module.starts_with(list))
}

impl Checker<'_> {
    pub(super) fn sugar_lints(&mut self, node: &Node) {
        self.shape_lints(node);
        match node.kind {
            K::DoExpr => {
                self.do_over_eff(node);
                self.returned_binding(node);
                self.bind_before(node);
            }
            K::AppExpr => {
                let children: Vec<&Node> = node.nodes().collect();
                if let [head, .., last] = children[..] {
                    self.forced(node, head, last);
                }
            }
            K::MatchExpr => self.rethrowing_match(node),
            K::BinExpr if node.has_token(K::PipeGt) => {
                let children: Vec<&Node> = node.nodes().collect();
                if let [lhs, rhs] = children[..] {
                    self.forced(node, rhs, lhs);
                    self.filter_then_map(node, lhs, rhs);
                }
            }
            _ => {}
        }
        let bare = matches!(node.kind, K::Block | K::LetDecl | K::ParenExpr);
        for child in node.nodes() {
            self.lint_child(child, bare);
        }
    }

    /// The lints of `child`, a chain of `flat_map` first.
    fn lint_child(&mut self, child: &Node, bare: bool) {
        match self.bind_chain(child, bare) {
            Some(parts) => parts.into_iter().for_each(|p| self.lint_child(p, true)),
            None => self.sugar_lints(child),
        }
    }

    /// T029: a `task` with direct calls instead of `do` over `Eff`.
    fn do_over_eff(&mut self, node: &Node) {
        let Some(ty) = self.sugar_type(node) else {
            return;
        };
        if self.eff_parts(&ty).is_none() {
            return;
        }
        let statements: Vec<&Node> = block_statements(node).collect();
        if statements
            .iter()
            .any(|s| matches!(s.kind, K::GuardStmt | K::BindGroup))
        {
            return;
        }
        let Some(keyword) = node.child_tokens().find(|t| t.kind == K::DoKw) else {
            return;
        };
        let warning = Diagnostic::new(
            codes::T029,
            Severity::Warning,
            "`do` over `Eff`: a `task` with direct calls builds the same computation",
        )
        .with_span(self.token_span(keyword));
        self.diagnostics.push(warning.with_fix(|| {
            let mut edits = vec![self.edit(keyword.start, keyword.end, "task")];
            let last = statements.len().saturating_sub(1);
            for (i, statement) in statements.iter().enumerate() {
                let start = statement.range().0;
                match statement.kind {
                    K::BindStmt => {
                        edits.push(self.edit(start, start, "let "));
                        if let Some(arrow) = statement.child_tokens().find(|t| t.kind == K::LArrow)
                        {
                            edits.push(self.edit(arrow.start, arrow.end, "= await"));
                        }
                    }
                    K::LetDecl | K::Signature => {}
                    _ if i == last => match self.pure_argument(statement) {
                        Some(value) => edits.push(self.edit(start, value.range().0, "")),
                        None => edits.push(self.edit(start, start, "await ")),
                    },
                    _ => {
                        let unit = self
                            .sugar_type(statement)
                            .and_then(|t| self.eff_parts(&t))
                            .is_some_and(|(_, _, a)| self.whnf(&a).is_lang(Lang::Unit));
                        let prefix = if unit { "await " } else { "ignore await " };
                        edits.push(self.edit(start, start, prefix));
                    }
                }
            }
            ("rewrite as a `task` with `await`", edits)
        }));
    }

    /// T030: `x <- e; pure x` at the end of a `do` block is `e`.
    fn returned_binding(&mut self, node: &Node) {
        let statements: Vec<&Node> = block_statements(node).collect();
        let [.., bind, last] = statements[..] else {
            return;
        };
        let mut parts = bind.nodes();
        let (Some(pattern), Some(value)) = (parts.next(), parts.next()) else {
            return;
        };
        let name = (bind.kind == K::BindStmt && pattern.kind == K::VarPat)
            .then(|| self.source_of(pattern).trim().to_owned());
        let returned = self
            .pure_argument(last)
            .map(|a| self.source_of(a).trim().to_owned());
        if name.is_none() || name != returned {
            return;
        }
        let (start, end) = if statements.len() == 2 {
            node.range()
        } else {
            (bind.range().0, last.range().1)
        };
        let warning = Diagnostic::new(
            codes::T030,
            Severity::Warning,
            "binding a value only to return it: the bound computation is the same",
        )
        .with_span(self.span(bind));
        self.diagnostics.push(warning.with_safe_fix(|| {
            (
                "keep the bound computation",
                vec![self.edit(start, end, self.source_of(value))],
            )
        }));
    }

    /// T031 and T032: `Seq.to_list` of what never ends, or of a `for`.
    fn forced(&mut self, node: &Node, head: &Node, arg: &Node) {
        if self.seq_function(head).as_deref() != Some("to_list") {
            return;
        }
        let arg = unparen(arg);
        if arg.kind == K::ForExpr {
            let (start, end) = node.range();
            let warning = Diagnostic::new(
                codes::T032,
                Severity::Warning,
                "`Seq.to_list` of a `for`: `[for …]` builds the list without a generator",
            )
            .with_span(self.span(node));
            self.diagnostics.push(warning.with_safe_fix(|| {
                let list = format!("[{}]", self.source_of(arg));
                (
                    "build it with `[for …]`",
                    vec![self.edit(start, end, &list)],
                )
            }));
        } else if self.infinite(arg) {
            let warning = Diagnostic::new(
                codes::T031,
                Severity::Warning,
                "this sequence never ends, so `Seq.to_list` never returns: take a part first with `Seq.take`",
            )
            .with_span(self.span(node));
            self.diagnostics.push(warning);
        }
    }

    /// Whether the sequence `node` builds surely never ends.
    fn infinite(&self, node: &Node) -> bool {
        let node = unparen(node);
        let children: Vec<&Node> = node.nodes().collect();
        match (node.kind, &children[..]) {
            (K::AppExpr, [head, .., last]) => match self.seq_function(head) {
                Some(f) if INFINITE.contains(&f.as_str()) => true,
                Some(f) if FOLLOWS.contains(&f.as_str()) => self.infinite(last),
                _ => false,
            },
            (K::BinExpr, [lhs, rhs]) if node.has_token(K::PipeGt) => {
                let head = match rhs.kind {
                    K::AppExpr => rhs.nodes().next(),
                    _ => Some(*rhs),
                };
                match head.and_then(|h| self.seq_function(h)) {
                    Some(f) if INFINITE.contains(&f.as_str()) => true,
                    Some(f) if FOLLOWS.contains(&f.as_str()) => self.infinite(lhs),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// T033: `xs |> List.filter (fun x -> p) |> List.map (fun x -> e)`.
    fn filter_then_map(&mut self, node: &Node, lhs: &Node, rhs: &Node) {
        let Some(map) = self.list_call(rhs, "map") else {
            return;
        };
        let lhs = unparen(lhs);
        let parts: Vec<&Node> = lhs.nodes().collect();
        let ([source, filter], true) = (&parts[..], lhs.has_token(K::PipeGt)) else {
            return;
        };
        let Some(keep) = self.list_call(filter, "filter") else {
            return;
        };
        let (Some((x, p)), Some((y, e))) = (self.lambda_parts(keep), self.lambda_parts(map)) else {
            return;
        };
        if self.source_of(x).trim() != self.source_of(y).trim() {
            return;
        }
        let (start, end) = node.range();
        let warning = Diagnostic::new(
            codes::T033,
            Severity::Warning,
            "`List.filter` then `List.map` walk the list twice: `[for …]` does both in one loop",
        )
        .with_span(self.span(node));
        self.diagnostics.push(warning.with_fix(|| {
            let source = self.source_of(source).trim();
            let (x, p, e) = (
                self.source_of(x).trim(),
                self.source_of(p).trim(),
                self.source_of(e).trim(),
            );
            let list = format!("[for {x} <- {source} if {p} yield {e}]");
            ("write it as `[for …]`", vec![self.edit(start, end, &list)])
        }));
    }

    /// T034: `match r with | Ok x -> x | Err e -> from_result (Err e)` is `r?`.
    fn rethrowing_match(&mut self, node: &Node) {
        let mut parts = node.nodes();
        let Some(scrutinee) = parts.next() else {
            return;
        };
        let arms: Vec<_> = parts
            .filter(|n| n.kind == K::MatchArm)
            .map(|arm| self.arm_parts(arm))
            .collect();
        let [Some(first), Some(second)] = &arms[..] else {
            return;
        };
        let ((("Ok", x, value), ("Err", e, failure)) | (("Err", e, failure), ("Ok", x, value))) =
            (first, second)
        else {
            return;
        };
        if self.source_of(value).trim() != self.source_of(x).trim() || !self.rethrows(failure, e) {
            return;
        }
        let (start, end) = node.range();
        let warning = Diagnostic::new(
            codes::T034,
            Severity::Warning,
            "this `match` fails with the error of the `Result`: `?` says the same",
        )
        .with_span(self.span(node));
        self.diagnostics.push(warning.with_safe_fix(|| {
            let source = self.source_of(scrutinee).trim();
            let operand = match unparen(scrutinee).kind {
                K::NameExpr | K::FieldExpr | K::QuestionExpr | K::ParenExpr => source.to_owned(),
                _ => format!("({source})"),
            };
            (
                "write it with `?`",
                vec![self.edit(start, end, &format!("{operand}?"))],
            )
        }));
    }

    /// The constructor of an arm `| C x -> body` without a guard, its one
    /// variable and its body.
    fn arm_parts<'n>(&self, arm: &'n Node) -> Option<(&str, &'n Node, &'n Node)> {
        let mut parts = arm.nodes();
        let (pattern, body) = (parts.next()?, parts.next()?);
        if pattern.kind != K::ConPat || body.kind != K::Block {
            return None;
        }
        let con = pattern.child_tokens().find(|t| t.kind == K::Con)?;
        let vars: Vec<&Node> = pattern.nodes().collect();
        let statements: Vec<&Node> = body.nodes().collect();
        let ([var], [value]) = (&vars[..], &statements[..]) else {
            return None;
        };
        (var.kind == K::VarPat).then(|| {
            let con = &self.text[con.start as usize..con.end as usize];
            (con, *var, unparen(value))
        })
    }

    /// Whether `failure` is `from_result (Err e)` of the Prelude.
    fn rethrows(&self, failure: &Node, e: &Node) -> bool {
        let parts: Vec<&Node> = failure.nodes().collect();
        let (K::AppExpr, [head, arg]) = (failure.kind, &parts[..]) else {
            return false;
        };
        matches!(self.typing.names.get(&head.range()),
            Some(Ref::Value { module, name }) if lang::is(Lang::FromResult, module, name))
            && words(self.source_of(unparen(arg))) == format!("Err {}", self.source_of(e).trim())
    }

    /// The function argument of `List.name f`, as written.
    pub(super) fn list_call<'n>(&self, node: &'n Node, name: &str) -> Option<&'n Node> {
        let parts: Vec<&Node> = node.nodes().collect();
        match (node.kind, &parts[..]) {
            (K::AppExpr, [head, f]) => {
                let found = match self.typing.names.get(&head.range()) {
                    Some(Ref::Value { module, name: n }) => is_list_module(module) && n == name,
                    _ => false,
                };
                found.then(|| unparen(f))
            }
            _ => None,
        }
    }

    /// `(x, body)` of `fun x -> body` with one variable and a one-line body.
    pub(super) fn lambda_parts<'n>(&self, f: &'n Node) -> Option<(&'n Node, &'n Node)> {
        if f.kind != K::FunExpr {
            return None;
        }
        let mut parts = f.nodes();
        let (param, body) = (parts.next()?, parts.next()?);
        (param.kind == K::VarPat && !self.source_of(body).trim().contains('\n'))
            .then_some((param, body))
    }

    /// The name of the `Seq` function `head` refers to.
    fn seq_function(&self, head: &Node) -> Option<String> {
        match self.typing.names.get(&unparen(head).range())? {
            Ref::Value { module, name } if lang::module_of(Lang::SeqMap) == Some(module) => {
                Some(name.clone())
            }
            _ => None,
        }
    }

    /// `v` of `pure v`, with the Prelude's `pure`.
    fn pure_argument<'n>(&self, node: &'n Node) -> Option<&'n Node> {
        let parts: Vec<&Node> = node.nodes().collect();
        let (K::AppExpr, [head, value]) = (node.kind, &parts[..]) else {
            return None;
        };
        match self.typing.names.get(&head.range())? {
            Ref::Method { module, name } if lang::is(Lang::Pure, module, name) => Some(value),
            _ => None,
        }
    }

    /// The solved type a `do` block or one of its statements recorded.
    pub(super) fn sugar_type(&self, node: &Node) -> Option<Type> {
        let ty = self
            .sugar
            .iter()
            .find(|(at, _)| *at == node.range())?
            .1
            .clone();
        Some(self.zonk(&ty))
    }

    pub(super) fn source_of(&self, node: &Node) -> &str {
        let (start, end) = node.range();
        &self.text[start as usize..end as usize]
    }

    pub(super) fn edit(&self, start: u32, end: u32, replacement: &str) -> Edit {
        Edit {
            span: Span::new(self.file, start, end),
            replacement: replacement.to_owned(),
        }
    }
}

fn block_statements(node: &Node) -> impl Iterator<Item = &Node> {
    node.nodes()
        .filter(|n| n.kind == K::Block)
        .flat_map(Node::nodes)
}

/// `text` with its whitespace runs as single spaces.
fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn unparen(node: &Node) -> &Node {
    match node.kind {
        K::ParenExpr => node.nodes().next().map_or(node, unparen),
        _ => node,
    }
}
