//! The predicate of a refined type, folded for a constant: what the plugin
//! that checks `T.parse` of a constant gets as its verdict.

use super::Checker;
use super::lang::{self, Lang};
use crate::consteval::{self, Const};
use crate::iface::Namespace;
use crate::source::SourceMap;
use crate::syntax;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::env::Found;

impl Checker<'_> {
    /// The check of the compiler's that `predicate` is, as the plugins know
    /// it: `email`, `non_empty` or `positive`.
    pub(super) fn known_check(&self, predicate: &Node) -> Option<&'static str> {
        let body = predicate.nodes().next()?;
        let written: String = body.text(self.text).split_whitespace().collect();
        let known = |module: &str, name: &str| {
            [
                (Lang::CheckEmail, "email"),
                (Lang::CheckNonEmpty, "non_empty"),
                (Lang::CheckPositive, "positive"),
            ]
            .into_iter()
            .find(|(l, _)| lang::is(*l, module, name))
            .map(|(_, check)| check)
        };
        match written.rsplit_once('.') {
            Some((qualifier, name)) => known(self.env.qualified_module(qualifier)?.0, name),
            None => match self.env.find(Namespace::Value, &written) {
                Found::Entity(e) => known(&e.module, &e.name),
                Found::Unknown | Found::Missing => None,
            },
        }
    }
}

/// Whether `constant` passes the predicate of a refined type, when the
/// compiler can fold it: a `fun` of literals.
pub(super) fn holds(predicate: &Node, text: &str, constant: &Const) -> Option<bool> {
    let body = predicate.nodes().next()?;
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
