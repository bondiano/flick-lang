//! The numeric coercions the standard Numeric instances declare as foreign.

use num_bigint::BigInt;

use crate::syntax::{Lit, Term};

pub(super) fn number(args: &[Term], _: &mut String) -> Result<Term, String> {
    let value = match args {
        [Term::Lit(Lit::Int(n))] => f64::from(*n),
        [Term::Lit(Lit::Float(n))] => *n,
        [Term::Lit(Lit::BigInt(n))] => n.to_string().parse().map_err(|e| format!("{e}"))?,
        _ => return Err(format!("Number of {args:?}")),
    };
    Ok(Term::Lit(Lit::Float(value)))
}

pub(super) fn bigint(args: &[Term], _: &mut String) -> Result<Term, String> {
    let value = match args {
        [Term::Lit(Lit::Int(n))] => BigInt::from(*n),
        [Term::Lit(Lit::BigInt(n))] => n.clone(),
        [Term::Lit(Lit::Float(n))] if n.is_finite() && n.trunc().to_bits() == n.to_bits() => {
            format!("{n:.0}").parse().map_err(|e| format!("{e}"))?
        }
        [Term::Lit(Lit::Float(_))] => return Ok(Term::Die("not an integer".into())),
        _ => return Err(format!("BigInt of {args:?}")),
    };
    Ok(Term::Lit(Lit::BigInt(value)))
}
