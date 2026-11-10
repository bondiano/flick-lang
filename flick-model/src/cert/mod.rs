//! Lean certificates of the programs of the generator
//! (`docs/formal/certificates.md`): a program the model accepts, with what
//! the check typed each subterm at (`check::trace`), as a proof of
//! `Typ S [] Facts.none [] ⟦e⟧ (.ex ⟦τ⟧ .pure)` in the core of
//! `formal/Formal/Rules.lean`, which `lake build` checks. A certificate
//! writes the instance of every variable and the arrow of every λ and call;
//! Lean infers the contexts and stages from them, and the tactics of
//! `formal/FormalCert/Tactics.lean` close the side conditions. A form the
//! certificates do not write yet is `Unsupported`, with its reason.

mod term;

use std::fmt::Write;

use crate::check::trace::Trace;
use crate::generate::Program;
use crate::translate::lean::elide::{Env, ty};

/// Why a program has no certificate yet.
pub type Unsupported = String;

/// The certificate of `p`, named `name`, from the trace of its check.
pub fn certificate(name: &str, p: &Program, trace: &Trace) -> Result<String, Unsupported> {
    if !p.scheme.vars.is_empty() {
        return Err("a polymorphic program".into());
    }
    let mut cx = term::Cx::new(trace);
    let proof = cx.typing(&p.term, &p.scheme.body, 2)?;
    let tm = term::tm(&p.term, &[])?;
    let t = ty(&p.scheme.body, &Env::default())?;
    let mut out = String::new();
    let _ = write!(
        out,
        "theorem {name} (S : Sig) :\n    Typ S [] Facts.none [] {tm}\n      (.ex {t} .pure) :=\n  {proof}\n"
    );
    Ok(out)
}
