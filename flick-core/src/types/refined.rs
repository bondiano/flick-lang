//! A refined type's `parse` of a constant, checked when the program
//! compiles if the compiler knows the check.

use super::Checker;
use super::typing::Ref;
use crate::consteval::{self, Const};
use crate::diagnostics::codes;
use crate::source::SourceMap;
use crate::syntax;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// `T.parse c` of a constant `c`: the check of `T`, applied when the
    /// program compiles if the compiler knows it.
    pub fn refined_constant(&mut self, head: &Node, arg: &Node) {
        let Some(Ref::Value { module, name }) = self.typing.names.get(&head.range()).cloned()
        else {
            return;
        };
        let Some(ty) = name.strip_suffix("$parse") else {
            return;
        };
        if module != self.env.module {
            return;
        }
        let Some(constant) = consteval::eval(arg, self.text) else {
            return;
        };
        let Some(predicate) = self.predicates.get(ty).copied() else {
            return;
        };
        if holds(predicate, self.text, &constant) == Some(false) {
            self.error(
                codes::D006,
                self.span(arg),
                format!(
                    "this constant fails the check of `{ty}`, so `{ty}.parse` of it always fails"
                ),
            );
        }
    }
}

/// Whether `constant` passes the predicate of a refined type, when the
/// compiler can tell: a known check, or a `fun` of literals.
fn holds(predicate: &Node, text: &str, constant: &Const) -> Option<bool> {
    let body = predicate.nodes().next()?;
    let written: String = body.text(text).split_whitespace().collect();
    match (written.as_str(), constant) {
        ("Checks.email", Const::String(s)) => return Some(email(s)),
        ("Checks.non_empty", Const::String(s)) => return Some(!s.is_empty()),
        ("Checks.positive", Const::Number(n)) => return Some(*n > 0.0),
        _ => {}
    }
    if body.kind != K::FunExpr {
        return None;
    }
    let param = body.nodes().find(|n| n.kind == K::VarPat)?;
    let param = param.text(text).trim().to_owned();
    let inner = body.nodes().find(|n| n.kind == K::Block)?;
    let literal = match constant {
        Const::Number(n) if n.is_finite() => format!("({})", js_number(*n)),
        Const::String(s) => serde_json::to_string(s).ok()?,
        Const::Bool(b) => b.to_string(),
        _ => return None,
    };
    // the body with the constant in place of the parameter, folded as a literal
    let mut source = String::new();
    for token in inner.tokens() {
        let written = token.text(text);
        if token.kind == K::Ident && written == param {
            source.push_str(&literal);
        } else {
            source.push_str(written);
        }
    }
    let src = format!("let x = {}\n", source.trim());
    let mut map = SourceMap::default();
    let file = map.add("<check>", src.as_str());
    let parsed = syntax::parse(&src, file);
    let decl = parsed.tree.nodes().next()?;
    let value = decl.nodes().find(|n| n.kind == K::Block)?;
    match consteval::eval(value, &src)? {
        Const::Bool(b) => Some(b),
        _ => None,
    }
}

fn js_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{n:.0}")
    } else {
        format!("{n}")
    }
}

/// `Checks.email`, as the runtime checks it: one `@` with text on both
/// sides, no spaces, and a dot inside the domain.
fn email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !s.chars().any(char::is_whitespace)
        && !domain.contains('@')
        && domain
            .char_indices()
            .any(|(i, c)| c == '.' && i > 0 && i + 1 < domain.len())
}
