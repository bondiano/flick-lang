//! The reference model of λFlick, the core of `docs/formal/core.md`: a
//! checker and a small-step interpreter whose code follows the rules one
//! to one, and a translation of flick-core's checked IR into λFlick, so
//! the model runs the programs of `tests/run` and their output can be
//! compared with the JS and Go builds (`tests/run.rs`), and a generator of
//! well-typed programs that the model properties run both checkers and the
//! machine on (`tests/props.rs`).
//!
//! Not part of `just test`: `just model` runs it.

#![allow(
    clippy::many_single_char_names,
    reason = "the code names what it handles as the rules of docs/formal/core.md do: e, x, t for τ, s for φ"
)]

pub mod cert;
pub mod check;
pub mod decls;
pub mod eval;
pub mod generate;
pub mod handler;
#[cfg(test)]
mod modules;
pub mod native;
pub mod rt;
pub mod run;
pub mod subst;
pub mod syntax;
pub mod translate;
pub mod types;
pub mod variance;
