//! FLICK-T035, a chain of `flat_map` that `do` says, and the fix of
//! FLICK-E001 in the pure rest of a `do` block: the call bound with `let`
//! before the block.

use std::fmt::Write;

use super::{Checker, Ref, block_statements, unparen, words};
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};

/// `var <- source`: one step of a chain.
struct Step<'n> {
    var: &'n Node,
    source: &'n Node,
}

impl Checker<'_> {
    /// T035: `flat_map (fun x -> flat_map (fun y -> e) (g x)) o`, the same
    /// with `Option.bind`, `Result.bind`, `|>` or `match … | None -> None`.
    /// `bare` when `node` may be replaced by a `do` block without
    /// parentheses. When it fires, the parts left to lint.
    pub(super) fn bind_chain<'n>(&mut self, node: &'n Node, bare: bool) -> Option<Vec<&'n Node>> {
        let mut steps = Vec::new();
        let mut body = node;
        while let Some((step, rest)) = self.bind_step(body) {
            steps.push(step);
            body = rest;
        }
        if steps.len() < 2
            || self.contains_mutable(body)
            || steps
                .iter()
                .skip(1)
                .any(|s| self.contains_mutable(s.source))
            || steps
                .iter()
                .any(|step| self.source_of(unparen(step.source)).trim().contains('\n'))
            || self.source_of(body).trim().contains('\n')
        {
            return None;
        }
        let (start, end) = node.range();
        let warning = Diagnostic::new(
            codes::T035,
            Severity::Warning,
            "a chain of `flat_map`: `do` binds each value in turn",
        )
        .with_span(self.span(node));
        self.diagnostics.push(warning.with_safe_fix(|| {
            let mut text = String::from("do { ");
            for step in &steps {
                let source = self.source_of(unparen(step.source)).trim();
                let var = self.source_of(step.var).trim();
                let _ = write!(text, "{var} <- {source}; ");
            }
            text += self.source_of(body).trim();
            text += " }";
            if !bare {
                text = format!("({text})");
            }
            ("write it as `do`", vec![self.edit(start, end, &text)])
        }));
        let mut rest: Vec<&Node> = steps.iter().map(|s| s.source).collect();
        rest.push(body);
        Some(rest)
    }

    /// The first step of `node` and the rest after it.
    fn bind_step<'n>(&self, node: &'n Node) -> Option<(Step<'n>, &'n Node)> {
        let node = single(node);
        let parts: Vec<&Node> = node.nodes().collect();
        let (f, source) = match (node.kind, &parts[..]) {
            (K::AppExpr, [head, f, source]) if self.binds(head) => (*f, *source),
            (K::BinExpr, [source, call]) if node.has_token(K::PipeGt) => {
                let call = unparen(call);
                let parts: Vec<&Node> = call.nodes().collect();
                match (call.kind, &parts[..]) {
                    (K::AppExpr, [head, f]) if self.binds(head) => (*f, *source),
                    _ => return None,
                }
            }
            (K::MatchExpr, [source, ..]) => return self.short_circuit(node, source),
            _ => return None,
        };
        let f = unparen(f);
        let parts: Vec<&Node> = f.nodes().collect();
        match (f.kind, &parts[..]) {
            (K::FunExpr, [param, body]) if param.kind == K::VarPat => {
                Some((Step { var: param, source }, single(body)))
            }
            _ => None,
        }
    }

    /// Whether `head` is the Prelude's `flat_map`, `Option.bind` or
    /// `Result.bind`.
    fn binds(&self, head: &Node) -> bool {
        match self.typing.names.get(&unparen(head).range()) {
            Some(Ref::Method { module, name }) => lang::is(Lang::FlatMap, module, name),
            Some(Ref::Value { module, name }) => {
                lang::is(Lang::OptionBind, module, name) || lang::is(Lang::ResultBind, module, name)
            }
            _ => false,
        }
    }

    /// `match source with | Some x -> e | None -> None`, or `Ok x` and
    /// `Err e -> Err e`, as the step `x <- source` and `e`.
    fn short_circuit<'n>(&self, node: &'n Node, source: &'n Node) -> Option<(Step<'n>, &'n Node)> {
        let arms: Vec<&Node> = node.nodes().filter(|n| n.kind == K::MatchArm).collect();
        let [a, b] = arms[..] else {
            return None;
        };
        [(a, b), (b, a)].into_iter().find_map(|(kept, passed)| {
            let (con, var, body) = self.arm_parts(kept)?;
            let passes = match con {
                "Some" => {
                    let parts: Vec<&Node> = passed.nodes().collect();
                    matches!(&parts[..], [pattern, value]
                        if words(self.source_of(pattern)) == "None"
                            && words(self.source_of(value)) == "None")
                }
                "Ok" => self.arm_parts(passed).is_some_and(|(con, e, value)| {
                    con == "Err"
                        && words(self.source_of(value))
                            == format!("Err {}", self.source_of(e).trim())
                }),
                _ => false,
            };
            let prelude = kept.nodes().next().is_some_and(|pattern| {
                matches!(self.typing.names.get(&pattern.range()),
                    Some(Ref::Constructor(c)) if c.module == lang::prelude())
            });
            (passes && prelude).then_some((Step { var, source }, body))
        })
    }

    /// The fix of each FLICK-E001 that a runtime call in the pure rest of
    /// the `do` block `node` got: the call moves before the block, bound
    /// with `let` unless it is a whole statement. Not offered when the call
    /// uses a variable the block binds.
    pub(super) fn bind_before(&mut self, node: &Node) {
        let (start, end) = node.range();
        let line = self.text[..start as usize].rfind('\n').map_or(0, |i| i + 1);
        let indent = self.text[line..start as usize].to_owned();
        if !indent.chars().all(|c| c == ' ') || !self.source_of(node).contains('\n') {
            return;
        }
        let bound = binders(node, self.text);
        for i in 0..self.diagnostics.len() {
            let d = &self.diagnostics[i];
            let Some(span) = d.span.filter(|s| {
                d.code == codes::E001
                    && d.message.contains("rest of a `do` block")
                    && s.file == self.file
                    && start <= s.start
                    && s.end <= end
            }) else {
                continue;
            };
            let Some(call) = find(node, (span.start, span.end)) else {
                continue;
            };
            let text = self.source_of(call).trim().to_owned();
            if text.contains('\n') || self.uses(call, &bound) {
                continue;
            }
            let edits = if block_statements(node).any(|s| s.range() == call.range()) {
                // the whole statement moves, with the line break before it
                let line = &self.text[..span.start as usize];
                let from = line.rfind('\n').unwrap_or(0);
                if !line[from..].trim().is_empty() {
                    continue;
                }
                let from = u32::try_from(from).expect("source offsets fit in u32");
                vec![
                    self.edit(start, start, &format!("{text}\n{indent}")),
                    self.edit(from, span.end, ""),
                ]
            } else {
                let name = fresh(self.source_of(node));
                vec![
                    self.edit(start, start, &format!("let {name} = {text}\n{indent}")),
                    self.edit(span.start, span.end, &name),
                ]
            };
            let d = self.diagnostics[i].clone();
            self.diagnostics[i] =
                d.with_fix(|| ("bind the call with `let` before the `do`", edits));
        }
    }

    /// Whether `node` uses one of the local variables `names`.
    fn uses(&self, node: &Node, names: &[String]) -> bool {
        (node.kind == K::NameExpr
            && matches!(self.typing.names.get(&node.range()), Some(Ref::Local))
            && names.iter().any(|n| n == self.source_of(node).trim()))
            || node.nodes().any(|n| self.uses(n, names))
    }
}

/// The variables the statements of the `do` block `node` bind.
fn binders(node: &Node, text: &str) -> Vec<String> {
    fn vars(node: &Node, text: &str, out: &mut Vec<String>) {
        if node.kind == K::VarPat {
            out.push(node.text(text).trim().to_owned());
        }
        node.nodes().for_each(|n| vars(n, text, out));
    }
    let mut out = Vec::new();
    for statement in block_statements(node) {
        let binds: Vec<&Node> = match statement.kind {
            K::BindGroup => statement
                .nodes()
                .filter(|n| n.kind == K::BindStmt)
                .collect(),
            K::BindStmt | K::LetDecl => vec![statement],
            _ => continue,
        };
        for bind in binds {
            if let Some(pattern) = bind.nodes().next() {
                vars(pattern, text, &mut out);
            }
        }
    }
    out
}

/// The node of `node` with exactly the range `range`.
fn find(node: &Node, range: (u32, u32)) -> Option<&Node> {
    if node.range() == range {
        return Some(node);
    }
    node.nodes()
        .filter(|n| n.range().0 <= range.0 && range.1 <= n.range().1)
        .find_map(|n| find(n, range))
}

/// `result`, or `result2` … when the block already says it.
fn fresh(block: &str) -> String {
    let taken = |name: &str| {
        block
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|w| w == name)
    };
    let mut name = "result".to_owned();
    for i in 2.. {
        if !taken(&name) {
            break;
        }
        name = format!("result{i}");
    }
    name
}

/// The one expression of a block or parentheses.
fn single(node: &Node) -> &Node {
    let node = unparen(node);
    let parts: Vec<&Node> = node.nodes().collect();
    match (node.kind, &parts[..]) {
        (K::Block, [one]) => single(one),
        _ => node,
    }
}

#[cfg(test)]
#[path = "chains_tests.rs"]
mod tests;
