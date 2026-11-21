//! The host primitives `Host.live` adapts, on the deterministic host the
//! model runs on: no environment variables and no arguments, as the
//! playground's host; a wall clock stopped at [`MILLIS`] and a monotonic
//! clock at zero; every random draw [`DRAW`]; no processes to start. The
//! model observes stdout only, so what goes to stderr is dropped. The
//! counted executor answers the same primitives with the same values.

use num_bigint::BigInt;

use super::{data, prelude};
use crate::syntax::{Lit, Term};

/// The wall clock, in milliseconds since the epoch: 2025-06-15T17:46:40Z.
pub const MILLIS: f64 = 1_750_000_000_000.0;

/// The wall clock in nanoseconds, [`MILLIS`] exactly.
pub const NANOS: i64 = 1_750_000_000_000_000_000;

/// The monotonic clock, in nanoseconds.
pub const MONOTONIC_NANOS: i64 = 0;

/// Every draw of `Host.Random.next`, in [0, 1).
pub const DRAW: f64 = 0.5;

fn text(args: &[Term]) -> Result<&str, String> {
    match args {
        [Term::Lit(Lit::String(s))] => Ok(s),
        _ => Err(format!("{args:?}")),
    }
}

pub(super) fn stderr_line(args: &[Term], _: &mut String) -> Result<Term, String> {
    text(args)?;
    Ok(Term::unit())
}

pub(super) fn stdout_write(args: &[Term], out: &mut String) -> Result<Term, String> {
    out.push_str(text(args)?);
    Ok(Term::unit())
}

pub(super) fn env(args: &[Term], _: &mut String) -> Result<Term, String> {
    text(args)?;
    Ok(data(prelude("Option", "None", 0), Vec::new()))
}

pub(super) fn args(_: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::List([].into()))
}

pub(super) fn vars(_: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::List([].into()))
}

pub(super) fn current_time_millis(_: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Lit(Lit::Float(MILLIS)))
}

pub(super) fn current_time_nanos(_: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Lit(Lit::BigInt(BigInt::from(NANOS))))
}

pub(super) fn monotonic_time_nanos(_: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Lit(Lit::BigInt(BigInt::from(MONOTONIC_NANOS))))
}

pub(super) fn next(_: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::Lit(Lit::Float(DRAW)))
}
