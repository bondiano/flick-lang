//! Concrete counterexample rendering from Omega models.

use crate::logic::{TUPLES, Ty};

use super::super::Counterexample;
use super::super::state::{BoolFormula, PathState, SymId, SymVal, lin_var};

/// Renders a real Omega model into parameter bindings in declaration order,
/// stripping lowering suffixes (`x$1` -> `x`) and including any relevant call
/// result witnesses.
pub(super) fn format_counterexample(
    state: &PathState,
    model: &[i128],
    bool_override: Option<(SymId, bool)>,
) -> Counterexample {
    let mut bindings = Vec::new();
    for (raw_name, val) in &state.param_vals {
        let Some(name) = display_param_name(raw_name) else {
            continue;
        };
        if let Some(rendered) = render_sym_val(val, state, model, bool_override, false) {
            bindings.push((name.to_owned(), rendered));
        }
    }
    for (label, var) in &state.call_results {
        if let Some(val) = lin_var(*var).eval(model) {
            bindings.push((label.clone(), val.to_string()));
        }
    }
    Counterexample { bindings }
}

fn display_param_name(raw: &str) -> Option<&str> {
    if raw.starts_with("$unused") {
        return None;
    }
    if let Some((base, suffix)) = raw.rsplit_once('$')
        && !base.is_empty()
        && !suffix.is_empty()
        && suffix.chars().all(|c| c.is_ascii_digit())
    {
        return Some(base);
    }
    Some(raw)
}

fn render_sym_val(
    val: &SymVal,
    state: &PathState,
    model: &[i128],
    bool_override: Option<(SymId, bool)>,
    nested: bool,
) -> Option<String> {
    match state.whnf(val.clone()) {
        SymVal::Int(lin) => lin.eval(model).map(|v| v.to_string()),
        SymVal::Bool(b) => Some(b.to_string()),
        SymVal::BoolExpr(BoolFormula::Atom(id)) | SymVal::Sym(id, Ty::Bool) => {
            let b = bool_override
                .filter(|(oid, _)| *oid == id)
                .map(|(_, b)| b)
                .or_else(|| state.bool_fact(id))
                .unwrap_or(false);
            Some(b.to_string())
        }
        SymVal::Unit => Some("()".into()),
        SymVal::Con(ctor, fields) => {
            if ctor.module == TUPLES {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|f| {
                        render_sym_val(f, state, model, bool_override, false)
                            .unwrap_or_else(|| "_".into())
                    })
                    .collect();
                return Some(format!("({})", parts.join(", ")));
            }
            if fields.is_empty() {
                return Some(ctor.name.clone());
            }
            let parts: Vec<String> = fields
                .iter()
                .map(|f| {
                    render_sym_val(f, state, model, bool_override, true)
                        .unwrap_or_else(|| "_".into())
                })
                .collect();
            let app = format!("{} {}", ctor.name, parts.join(" "));
            Some(if nested { format!("({app})") } else { app })
        }
        _ => None,
    }
}
