//! Regions and captures (`docs/formal/regions.md` §2), written from the
//! document and not from flick-core's `types/regions/`: ⌈·⌉ (§2.1), the
//! obligations of (≤-Cap), (Lam), (Task), (Con) and (Inst) and their least
//! solution (§2.6), (Scope) (§2.3), `Outlives` (§2.4) and (Err-Raise)
//! (§2.5). An obligation waits for the end of the declaration, when every
//! type it names is known: a capture row grows to the least set its
//! obligations need, and a meta outside a `scope` never takes its region.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use super::{Check, Checker, error};

mod live;
mod provided;
mod scope;
mod solve;
use crate::syntax::{Con, Name, Path, Term};
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};
pub(crate) use live::live_service;
pub(in crate::check) use live::{
    add_live, arg_caps, drop_empty_live, live_arg, live_stage, regions_of,
};
pub(in crate::check) use scope::names_into;

/// `⋃ ⌈held⌉ ⊆ ⋃ ⌈within⌉ ∪ allowed`.
#[derive(Clone, Debug)]
pub(super) struct Hold {
    held: Vec<Type>,
    within: Vec<Type>,
    allowed: Caps,
    rule: &'static str,
    /// Scheme-bound facts in force where the obligation was generated.
    bounds: Vec<(Var, Caps)>,
}

/// `Outlives C ϱ` and the facts in force where it is needed.
type Need = (Caps, Type, Vec<(Caps, Type)>);

/// A row obligation whose owner is not known until its enclosing call closes.
#[derive(Clone)]
struct ProvidedNeed {
    row: Row,
    within: Row,
    givens: Vec<(Row, Type)>,
}

/// What the rules of regions keep while a declaration is checked.
#[derive(Clone, Default)]
pub(super) struct Regions {
    holds: Vec<Hold>,
    /// Each region a `scope` of the declaration binds: its level and the
    /// region of the `scope` around it.
    scopes: HashMap<Name, (u32, Option<Name>)>,
    /// The `scope`s around the position being checked, innermost last.
    open: Vec<Name>,
    /// The variables of kind `Region`: a signature's and metas.
    pub(super) kinds: HashSet<Var>,
    /// The errors of each step under a `scope`, with the `scope`s open.
    raised: Vec<(Row, Vec<Name>)>,
    /// `Outlives C ϱ` an instance needs, with the facts `Θ` in force there.
    needs: Vec<Need>,
    /// `Θ` of the signatures around the position being checked.
    pub(super) facts: Vec<(Caps, Type)>,
    /// The region each `scope` around binds its name to, innermost last.
    renamed: Vec<(Name, Name)>,
    /// The regions of the `scope`s and `provide`s around, innermost last.
    nest: Vec<Name>,
    /// `Provided R ϱ` of the signatures around (§2.7).
    givens: Vec<(Row, Type)>,
    /// Value bounds of rigid variables introduced by a signed declaration.
    bounds: Vec<(Var, Caps)>,
    pending_provided: Vec<ProvidedNeed>,
    /// The calls being checked, innermost last: the unknown `Scope`
    /// regions of steps of their task arguments, each with the `scope`
    /// innermost there.
    calls: Vec<Vec<(Type, Name)>>,
    /// While an argument of type `Eff` is checked: its call, and how many
    /// `scope`s are open there.
    task_argument: Option<(usize, usize)>,
    /// The region of `Scope` the stage of the λ or task being checked
    /// names, once known, with how many `scope`s are open there.
    declared: Option<(Type, usize)>,
}

/// The element `Scope ϱ` of a row of services.
pub(crate) fn scope_service() -> Path {
    Path::new("Prelude", "Scope")
}

pub(super) fn owner(s: &Name) -> Type {
    Type::app(
        Type::con("Fiber", "Owner"),
        Type::Var(Var::Named(s.clone())),
    )
}

const SCALARS: [&str; 7] = ["Int", "Float", "BigInt", "String", "Bool", "Unit", "Never"];

include!("reachability.rs");
include!("ownership_bounds.rs");

/// What `{Live C}` of `r` needs alive: `C`, or nothing.
fn row_live(r: &Row) -> Caps {
    r.elems
        .get(&live_service())
        .map(|a| arg_caps(&a[0]))
        .unwrap_or_default()
}

/// The variables `α` of a scheme under a value bound `α^D`.
fn bounded_vars(t: &Type, vars: &[(Name, Kind)], out: &mut Vec<(Name, Caps)>) {
    let go = |t: &Type, out: &mut Vec<(Name, Caps)>| bounded_vars(t, vars, out);
    match t {
        Type::Bounded(x, d) => {
            if let Type::Var(Var::Named(a)) = &**x
                && vars.iter().any(|(v, _)| v == a)
            {
                out.push((a.clone(), d.clone()));
            }
            go(x, out);
        }
        Type::Captured(x, _) | Type::Eff(_, _, x) | Type::Ref(x) => go(x, out),
        Type::App(a, b) | Type::Fun(a, _, b) => {
            go(a, out);
            go(b, out);
        }
        Type::Tuple(ts) | Type::Dict(_, ts) => ts.iter().for_each(|t| go(t, out)),
        Type::Record(Fields { fields, .. }) => fields.values().for_each(|t| go(t, out)),
        _ => {}
    }
}
