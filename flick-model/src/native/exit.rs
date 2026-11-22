//! Model natives for `Exit` snapshot predicates.

use crate::syntax::Term;

#[allow(
    clippy::unnecessary_wraps,
    reason = "every native has the signature of `Native::run`, which may fail"
)]
pub(super) fn is_failure(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Cap(p, None, _)] if &*p.module == "Exit" => Ok(Term::bool(&*p.name == "fail")),
        _ => Err(format!("{args:?}")),
    }
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "every native has the signature of `Native::run`, which may fail"
)]
pub(super) fn is_success(args: &[Term], _: &mut String) -> Result<Term, String> {
    match args {
        [Term::Cap(p, None, _)] if &*p.module == "Exit" => Ok(Term::bool(&*p.name != "fail")),
        _ => Err(format!("{args:?}")),
    }
}
