//! Structural record and tuple dictionary operations for the reference model.
use std::rc::Rc;

use crate::syntax::Term;

use super::{concat, cond, list};

/// The values of the fields, and their labels if a record.
type Fields = (Vec<Term>, Option<Vec<Rc<str>>>);

/// The fields of a record in label order, or of a tuple by position.
fn fields(v: &Term) -> Result<Fields, String> {
    match v {
        Term::Tuple(xs) => Ok((xs.to_vec(), None)),
        Term::Record(fs) => {
            let mut fs = fs.to_vec();
            fs.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            let labels = fs.iter().map(|(l, _)| l.clone()).collect();
            Ok((fs.into_iter().map(|(_, x)| x).collect(), Some(labels)))
        }
        other => Err(format!("not a record or tuple: {other:?}")),
    }
}

fn field_call(dict: &Term, method: &str, args: impl IntoIterator<Item = Term>) -> Term {
    Term::apps(Term::Field(Rc::new(dict.clone()), method.into()), args)
}

/// `(a, b)` or `{ x = a, y = b }`.
pub(super) fn struct_show(args: &[Term], _: &mut String) -> Result<Term, String> {
    let dicts = list(&args[0])?;
    let (xs, labels) = fields(&args[1])?;
    let shown = xs
        .iter()
        .zip(dicts)
        .map(|(x, d)| field_call(d, "show", [x.clone()]));
    let mut parts = Vec::new();
    match labels {
        None => {
            parts.push(Term::string("("));
            for (i, s) in shown.enumerate() {
                if i > 0 {
                    parts.push(Term::string(", "));
                }
                parts.push(s);
            }
            parts.push(Term::string(")"));
        }
        Some(labels) => {
            parts.push(Term::string("{ "));
            for (i, (l, s)) in labels.iter().zip(shown).enumerate() {
                if i > 0 {
                    parts.push(Term::string(", "));
                }
                parts.push(Term::string(&format!("{l} = ")));
                parts.push(s);
            }
            parts.push(Term::string(" }"));
        }
    }
    Ok(concat(parts))
}

pub(super) fn struct_eq(args: &[Term], _: &mut String) -> Result<Term, String> {
    let dicts = list(&args[0])?;
    let ((xs, _), (ys, _)) = (fields(&args[1])?, fields(&args[2])?);
    let pairs: Vec<_> = xs.into_iter().zip(ys).zip(dicts).collect();
    Ok(pairs
        .into_iter()
        .rev()
        .fold(Term::bool(true), |rest, ((x, y), d)| {
            cond(field_call(d, "eq", [x, y]), rest, Term::bool(false))
        }))
}
