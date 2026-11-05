//! The operators of Γ₀, as the runtimes compute them: `Int` wraps around
//! (`flick_core::numeric::int_arith`), `Float` is IEEE, `==` is structural
//! with `SameValueZero` on numbers, strings order by UTF-16 code units.

use std::cmp::Ordering;

use flick_core::numeric::{Arith, Num, int_arith};

use crate::subst::lit_eq;
use crate::syntax::{Head, Lit, Prim, Term};

pub fn run(p: Prim, args: &[Term]) -> Result<Term, String> {
    let lit = |t: Lit| Ok(Term::Lit(t));
    match (p, args) {
        (Prim::Arith(op, Num::Int), [Term::Lit(Lit::Int(a)), Term::Lit(Lit::Int(b))]) => {
            lit(Lit::Int(int_arith(op, *a, *b)))
        }
        (Prim::Arith(op, Num::Float), [Term::Lit(Lit::Float(a)), Term::Lit(Lit::Float(b))]) => {
            lit(Lit::Float(float_arith(op, *a, *b)))
        }
        (Prim::Neg(Num::Int), [Term::Lit(Lit::Int(a))]) => lit(Lit::Int(a.wrapping_neg())),
        (Prim::Neg(Num::Float), [Term::Lit(Lit::Float(a))]) => lit(Lit::Float(-a)),
        (Prim::Eq | Prim::NotEq, [a, b]) => match equal(a, b) {
            Some(same) => lit(Lit::Bool(same == (p == Prim::Eq))),
            None => Err(format!("`==` of {a:?} and {b:?}")),
        },
        (Prim::Same | Prim::NotSame, [Term::Lit(a), Term::Lit(b)]) => {
            lit(Lit::Bool(lit_eq(a, b) == (p == Prim::Same)))
        }
        // the values of an enum, which the builds compare as their literals
        (
            Prim::Same | Prim::NotSame,
            [Term::Apply(Head::Con(a), x), Term::Apply(Head::Con(b), y)],
        ) if x.is_empty() && y.is_empty() && a.ty == b.ty => {
            lit(Lit::Bool((a.name == b.name) == (p == Prim::Same)))
        }
        (Prim::Lt | Prim::Le | Prim::Gt | Prim::Ge, [Term::Lit(a), Term::Lit(b)]) => {
            let order = order(a, b).ok_or_else(|| format!("`<` of {a:?} and {b:?}"))?;
            lit(Lit::Bool(match (p, order) {
                (_, None) => false,
                (Prim::Lt, Some(o)) => o == Ordering::Less,
                (Prim::Le, Some(o)) => o != Ordering::Greater,
                (Prim::Gt, Some(o)) => o == Ordering::Greater,
                (_, Some(o)) => o != Ordering::Less,
            }))
        }
        (Prim::Not, [Term::Lit(Lit::Bool(b))]) => lit(Lit::Bool(!b)),
        (Prim::Concat, [Term::Lit(Lit::String(a)), Term::Lit(Lit::String(b))]) => {
            lit(Lit::String(format!("{a}{b}").into()))
        }
        (Prim::ListConcat, [Term::List(a), Term::List(b)]) => {
            Ok(Term::List(a.iter().chain(b.iter()).cloned().collect()))
        }
        (Prim::TagOf, [Term::Apply(Head::Con(c), _)]) => {
            lit(Lit::String(format!("{}.{}", c.ty, c.name).into()))
        }
        (Prim::Atom, [Term::Lit(Lit::String(s))]) => lit(Lit::String(atom(s).into())),
        _ => Err(format!("{p:?} of {args:?}")),
    }
}

fn float_arith(op: Arith, a: f64, b: f64) -> f64 {
    match op {
        Arith::Add => a + b,
        Arith::Sub => a - b,
        Arith::Mul => a * b,
        Arith::Div => a / b,
        // JS `%` keeps the sign of the dividend, as Rust's does
        Arith::Rem => a % b,
        Arith::Pow => a.powf(b),
    }
}

/// `<` as JS orders primitives: `None` inside when a `NaN` makes every
/// comparison false; `None` outside for values `<` does not compare.
#[expect(
    clippy::option_option,
    reason = "the two `None`s mean different things, as above"
)]
fn order(a: &Lit, b: &Lit) -> Option<Option<Ordering>> {
    Some(match (a, b) {
        (Lit::Int(x), Lit::Int(y)) => Some(x.cmp(y)),
        (Lit::Float(x), Lit::Float(y)) => x.partial_cmp(y),
        (Lit::String(x), Lit::String(y)) => Some(x.encode_utf16().cmp(y.encode_utf16())),
        (Lit::Bool(x), Lit::Bool(y)) => Some(x.cmp(y)),
        _ => return None,
    })
}

/// Structural equality of two values; `None` for values without it.
#[expect(
    clippy::float_cmp,
    reason = "`==` on `Float` is `SameValueZero`, exact"
)]
pub fn equal(a: &Term, b: &Term) -> Option<bool> {
    Some(match (a, b) {
        (Term::Lit(Lit::Float(x)), Term::Lit(Lit::Float(y))) => {
            x == y || (x.is_nan() && y.is_nan())
        }
        (Term::Lit(x), Term::Lit(y)) => x == y,
        (Term::Apply(Head::Con(c), xs), Term::Apply(Head::Con(d), ys)) => {
            c == d && all_equal(xs, ys)?
        }
        (Term::Tuple(xs), Term::Tuple(ys)) | (Term::List(xs), Term::List(ys)) => {
            xs.len() == ys.len() && all_equal(xs, ys)?
        }
        (Term::Record(xs), Term::Record(ys)) => {
            xs.len() == ys.len()
                && xs.iter().try_fold(true, |acc, (l, x)| {
                    let (_, y) = ys.iter().find(|(m, _)| m == l)?;
                    Some(acc && equal(x, y)?)
                })?
        }
        (Term::Loc(x), Term::Loc(y)) => x == y,
        _ => return None,
    })
}

fn all_equal(xs: &[Term], ys: &[Term]) -> Option<bool> {
    xs.iter()
        .zip(ys)
        .try_fold(true, |acc, (x, y)| Some(acc && equal(x, y)?))
}

/// `atom` of `runtime/src/core/data.ts`: a shown argument of a constructor
/// in parentheses when it has a space and is not already bracketed.
pub fn atom(text: &str) -> String {
    let bracketed = text.starts_with(['[', '(', '{', '"']);
    if bracketed || !text.contains(' ') {
        text.to_owned()
    } else {
        format!("({text})")
    }
}
