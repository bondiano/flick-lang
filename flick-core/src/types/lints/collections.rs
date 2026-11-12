//! List and option code a library function says directly: T047
//! `List.head` of `List.filter` (`List.find`), T048 the length of a
//! `List.filter` compared with zero (`List.any`), T049 a length compared
//! with zero (`List.is_empty`), T050 `List.concat` of `List.map`
//! (`List.flat_map`) and T051 a `match` that unwraps an `Option` with a
//! default (`Option.with_default`).

use super::super::Checker;
use super::super::sugar_lints::{piped, unparen};
use super::super::typing::Ref;
use super::single;
use crate::diagnostics::codes;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::sugar_lints::is_list_module;

impl Checker<'_> {
    pub(super) fn collection_lints(&mut self, node: &Node) {
        match node.kind {
            K::MatchExpr => self.option_default(node),
            K::BinExpr => self.length_compared(node),
            _ => {}
        }
        let Some((name, args)) = self.list_applied(node) else {
            return;
        };
        let [inner] = args[..] else {
            return;
        };
        let (wanted, code, message) = match name.as_str() {
            "head" => (
                "filter",
                codes::T047,
                "`List.head` of `List.filter`: `List.find` stops at the first match",
            ),
            "concat" => (
                "map",
                codes::T050,
                "`List.concat` of `List.map`: `List.flat_map` does both",
            ),
            _ => return,
        };
        let Some((found, inner_args)) = self.list_applied(unparen(inner)) else {
            return;
        };
        let [f, xs] = inner_args[..] else {
            return;
        };
        if found != wanted {
            return;
        }
        let function = if code == codes::T047 {
            "find"
        } else {
            "flat_map"
        };
        self.lint(code, message.to_owned(), self.span(node), |this| {
            let replacement = format!("List.{function} {} {}", this.operand(f), this.operand(xs));
            (
                format!("use `List.{function}`"),
                vec![this.replace(node, &replacement)],
                true,
            )
        });
    }

    /// T048 and T049: `List.length xs == 0` and the like, of a
    /// `List.filter` or of any list.
    fn length_compared(&mut self, node: &Node) {
        let parts: Vec<&Node> = node.nodes().collect();
        let [lhs, rhs] = parts[..] else {
            return;
        };
        let Some(op) = node
            .child_tokens()
            .map(|t| t.kind)
            .find(|k| matches!(k, K::EqEq | K::NotEq | K::Gt | K::Ge | K::Lt | K::Le))
        else {
            return;
        };
        let zero =
            |n: &Node| unparen(n).kind == K::Literal && self.source_of(unparen(n)).trim() == "0";
        let one =
            |n: &Node| unparen(n).kind == K::Literal && self.source_of(unparen(n)).trim() == "1";
        // `length == 0` is empty, `length > 0` (`!= 0`, `>= 1`) is not
        let empty = match op {
            K::EqEq if zero(rhs) => true,
            K::NotEq | K::Gt if zero(rhs) => false,
            K::Ge if one(rhs) => false,
            K::Lt if one(rhs) => true,
            _ => return,
        };
        let Some((name, args)) = self.list_applied(unparen(lhs)) else {
            return;
        };
        let ("length", [list]) = (name.as_str(), &args[..]) else {
            return;
        };
        let filtered = self
            .list_applied(unparen(list))
            .filter(|(n, a)| n == "filter" && a.len() == 2);
        let (code, message) = if filtered.is_some() {
            (
                codes::T048,
                "the length of a `List.filter` compared with zero: `List.any` stops at the first match",
            )
        } else {
            (
                codes::T049,
                "a length compared with zero: `List.is_empty` says it",
            )
        };
        self.lint(code, message.to_owned(), self.span(node), |this| {
            let test = match &filtered {
                Some((_, a)) => format!("List.any {} {}", this.operand(a[0]), this.operand(a[1])),
                None => format!("List.is_empty {}", this.operand(list)),
            };
            let replacement = match (code == codes::T048, empty) {
                (true, true) | (false, false) => format!("not ({test})"),
                _ => test,
            };
            (
                "say it directly",
                vec![this.replace(node, &replacement)],
                true,
            )
        });
    }

    /// T051: `match o with | Some x -> x | None -> d` is
    /// `Option.with_default d o`.
    fn option_default(&mut self, node: &Node) {
        let mut parts = node.nodes();
        let Some(scrutinee) = parts.next() else {
            return;
        };
        let arms: Vec<&Node> = parts.filter(|n| n.kind == K::MatchArm).collect();
        let [first, second] = arms[..] else {
            return;
        };
        let default = match (self.option_arm(first), self.option_arm(second)) {
            (Some((Some(x), v)), Some((None, d))) | (Some((None, d)), Some((Some(x), v))) => {
                if self.source_of(v).trim() != x {
                    return;
                }
                d
            }
            _ => return,
        };
        // `with_default` takes the default as a value, made before the match:
        // a call there could fail or do work the `match` skipped
        if self.contains_mutable(default)
            || !matches!(
                unparen(default).kind,
                K::Literal | K::NameExpr | K::FieldExpr
            )
        {
            return;
        }
        self.lint(
            codes::T051,
            "a `match` that unwraps an `Option` with a default: `Option.with_default` says it"
                .to_owned(),
            self.span(node),
            |this| {
                let replacement = format!(
                    "Option.with_default {} {}",
                    this.operand(default),
                    this.operand(scrutinee)
                );
                (
                    "use `Option.with_default`",
                    vec![this.replace(node, &replacement)],
                    true,
                )
            },
        );
    }

    /// An arm without a guard: `Some x` as the variable, `None` as `None`,
    /// with its one-line body.
    fn option_arm<'n>(&self, arm: &'n Node) -> Option<(Option<String>, &'n Node)> {
        let mut parts = arm.nodes();
        let (pattern, body) = (parts.next()?, parts.next()?);
        if pattern.kind != K::ConPat || body.kind != K::Block || !self.one_line(body) {
            return None;
        }
        let body = single(body)?;
        let con = pattern.child_tokens().find(|t| t.kind == K::Con)?;
        let vars: Vec<&Node> = pattern.nodes().collect();
        match (con.text(self.text), &vars[..]) {
            ("Some", [var]) if var.kind == K::VarPat => {
                Some((Some(self.source_of(var).trim().to_owned()), body))
            }
            ("None", []) => Some((None, body)),
            _ => None,
        }
    }

    /// The `List` function `node` applies and its arguments, written
    /// `List.f a b`, `b |> List.f a` or `List.f a <| b`.
    fn list_applied<'n>(&self, node: &'n Node) -> Option<(String, Vec<&'n Node>)> {
        let parts: Vec<&Node> = node.nodes().collect();
        match (node.kind, &parts[..]) {
            (K::AppExpr, [head, args @ ..]) => Some((self.list_function(head)?, args.to_vec())),
            (K::BinExpr, _) if let Some((function, argument)) = piped(node) => {
                let (name, mut args) = match unparen(function).kind {
                    K::AppExpr => self.list_applied(unparen(function))?,
                    _ => (self.list_function(function)?, Vec::new()),
                };
                args.push(argument);
                Some((name, args))
            }
            _ => None,
        }
    }

    /// The name of the `List` function `head` refers to.
    fn list_function(&self, head: &Node) -> Option<String> {
        match self.typing.names.get(&unparen(head).range())? {
            Ref::Value { module, name } if is_list_module(module) => Some(name.clone()),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "collections_tests.rs"]
mod tests;
