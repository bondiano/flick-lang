//! `show` and `compare` of primitives and lists, as
//! `runtime/src/core/data.ts` computes them: numbers as JS `String(x)`,
//! strings as `JSON.stringify`, a constructor by its name.

use std::cmp::Ordering;
use std::rc::Rc;

use flick_core::opt::{js_number, json_string};

use super::{concat, data, list, prelude};
use crate::syntax::{Arm, Head, Lit, Pat, Term};

/// `Show` of a primitive.
pub fn show(args: &[Term], _: &mut String) -> Result<Term, String> {
    let text = match &args[0] {
        Term::Lit(Lit::Int(i)) => i.to_string(),
        Term::Lit(Lit::Float(x)) => js_number(*x),
        Term::Lit(Lit::BigInt(n)) => format!("{n}n"),
        Term::Lit(Lit::String(s)) => json_string(s),
        Term::Lit(Lit::Bool(b)) => b.to_string(),
        Term::Lit(Lit::Unit) => "undefined".to_owned(),
        Term::Apply(Head::Con(c), _) => c.name.to_string(),
        other => return Err(format!("{other:?}")),
    };
    Ok(Term::string(&text))
}

/// `hashString` of `runtime/src/core/data.ts`, over UTF-16 code units.
fn hash_string(s: &str) -> i32 {
    s.encode_utf16()
        .fold(5381_i32, |h, c| h.wrapping_mul(33) ^ i32::from(c))
}

/// `hash` of a primitive, as the runtime without Effect computes it.
pub fn hash(args: &[Term], _: &mut String) -> Result<Term, String> {
    let h = match &args[0] {
        Term::Lit(Lit::Int(i)) => hash_string(&i.to_string()),
        Term::Lit(Lit::Float(x)) if *x == 0.0 => hash_string("0"),
        Term::Lit(Lit::Float(x)) => hash_string(&js_number(*x)),
        Term::Lit(Lit::BigInt(n)) => hash_string(&format!("bigint:{n}")),
        Term::Lit(Lit::String(s)) => hash_string(&format!("string:{s}")),
        Term::Lit(Lit::Bool(b)) => hash_string(&format!("boolean:{b}")),
        other => return Err(format!("{other:?}")),
    };
    Ok(Term::Lit(Lit::Int(h)))
}

/// `[s x₁, s x₂, …]`.
pub fn list_show(args: &[Term], _: &mut String) -> Result<Term, String> {
    let xs = list(&args[1])?;
    let mut parts = vec![Term::string("[")];
    for (i, x) in xs.iter().enumerate() {
        if i > 0 {
            parts.push(Term::string(", "));
        }
        parts.push(Term::app(args[0].clone(), x.clone()));
    }
    parts.push(Term::string("]"));
    Ok(concat(parts))
}

fn ordering(o: Ordering) -> Term {
    let name = match o {
        Ordering::Less => "LT",
        Ordering::Equal => "EQ",
        Ordering::Greater => "GT",
    };
    data(prelude("Ordering", name, 0), Vec::new())
}

/// The total order of `Ord`: on numbers −∞ < finite < +∞ < NaN, signed
/// zeros and all NaNs equal; strings by UTF-16 code units.
pub fn compare(args: &[Term], _: &mut String) -> Result<Term, String> {
    let o = match (&args[0], &args[1]) {
        (Term::Lit(Lit::Int(a)), Term::Lit(Lit::Int(b))) => a.cmp(b),
        (Term::Lit(Lit::BigInt(a)), Term::Lit(Lit::BigInt(b))) => a.cmp(b),
        (Term::Lit(Lit::Float(a)), Term::Lit(Lit::Float(b))) => match (a.is_nan(), b.is_nan()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        },
        (Term::Lit(Lit::String(a)), Term::Lit(Lit::String(b))) => {
            a.encode_utf16().cmp(b.encode_utf16())
        }
        (Term::Lit(Lit::Bool(a)), Term::Lit(Lit::Bool(b))) => a.cmp(b),
        (a, b) => return Err(format!("{a:?} and {b:?}")),
    };
    Ok(ordering(o))
}

/// The first pair `cmp` does not find equal decides; then the lengths.
pub fn list_compare(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (xs, ys) = (list(&args[1])?, list(&args[2])?);
    let last = ordering(xs.len().cmp(&ys.len()));
    let pairs = xs.iter().zip(ys).rev();
    Ok(pairs.fold(last, |rest, (x, y)| {
        let equal = Arm {
            pat: Pat::Con(prelude("Ordering", "EQ", 0), Rc::from([])),
            guard: None,
            body: rest,
        };
        let other = Arm {
            pat: Pat::Var("$o".into()),
            guard: None,
            body: Term::Var("$o".into()),
        };
        let call = Term::apps(args[0].clone(), [x.clone(), y.clone()]);
        Term::Match(Rc::new(call), Rc::from([equal, other]))
    }))
}
