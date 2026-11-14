//! Types of flick-core to types of the model. Class constraints become
//! dictionary parameters (`docs/formal/core.md` §1.6), `Fiber.Outlives`
//! the facts `Θ` of a scheme (`docs/formal/regions.md` §1.2),
//! `Fiber.Provided` an element of a row (§2.7); regions are
//! variables of kind `Region`, qualifiers `τ^C`; the prelude's `Eff`, `Ref`,
//! `Layer`, `Errors` and `S.Impl` become the forms of §1.2.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use flick_core::types::lang::Lang;
use flick_core::types::{self as fc, Binder};

use crate::syntax::Path;
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};

#[cfg(test)]
mod tests;

const PRELUDE: &str = "Prelude";

pub(crate) fn kind(k: &fc::Kind) -> Option<Kind> {
    Some(match k {
        fc::Kind::ServiceRow => Kind::Services,
        fc::Kind::ErrorRow => Kind::Errors,
        fc::Kind::Row(inner) if **inner == fc::Kind::Type => Kind::Fields,
        fc::Kind::Arrow(a, b) => Kind::Arrow(Rc::new(kind(a)?), Rc::new(kind(b)?)),
        fc::Kind::Region => Kind::Region,
        fc::Kind::CaptureRow => Kind::Captures,
        // the rest is not in the core
        _ => Kind::Type,
    })
}

/// Whether a constraint passes a dictionary: `Fiber.Outlives` does not.
fn passes(c: &fc::Type) -> bool {
    fc::passes_dictionary(c)
}

/// `∀ᾱ. C̄ ⇒ τ` as `∀ᾱ. C₁.Dict τ̄₁ →P … →P τ`, with `Outlives C ϱ` in `Θ`.
pub fn scheme(t: &fc::Type) -> Scheme {
    match t {
        fc::Type::Forall {
            vars,
            constraints,
            body,
        } => {
            let mut out = ty(body);
            for c in constraints.iter().rev().filter(|c| passes(c)) {
                out = Type::fun(constraint(c), Stage::Pure, out);
            }
            for c in constraints.iter().filter(|c| !passes(c)) {
                if let (head, args) = c.spine()
                    && let [r, s] = &args[..]
                    && head.is_lang(Lang::Provided)
                {
                    out = out.with_provided(&row(r), &ty(s));
                }
            }
            let outlives = constraints
                .iter()
                .filter(|c| !passes(c))
                .filter_map(|c| match c.spine() {
                    (head, args) if head.is_lang(Lang::Outlives) => match &args[..] {
                        [c, s] => Some((caps(c), ty(s))),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            Scheme {
                vars: binders(vars),
                body: out,
                outlives,
            }
        }
        other => Scheme {
            vars: Vec::new(),
            body: ty(other),
            outlives: Vec::new(),
        },
    }
}

pub fn binders(vars: &[Binder]) -> Vec<(Rc<str>, Kind)> {
    vars.iter()
        .filter_map(|b| Some((b.name.as_str().into(), kind(&b.kind)?)))
        .collect()
}

/// The dictionary type of a constraint `C τ̄`.
pub fn constraint(c: &fc::Type) -> Type {
    let (head, args) = c.spine();
    match head {
        fc::Type::Con { module, name } => {
            Type::Dict(Path::new(module, name), args.into_iter().map(ty).collect())
        }
        other => Type::Con(Path::new("?", &format!("{other:?}"))),
    }
}

pub fn ty(t: &fc::Type) -> Type {
    match t {
        fc::Type::Con { module, name } => {
            if let Some(class) = name.strip_suffix("$Dict") {
                Type::Dict(Path::new(module, class), Rc::from([]))
            } else if let Some(service) = name.strip_suffix(".Impl") {
                Type::Impl(Row::one(Path::new(module, service)))
            } else {
                Type::con(module, name)
            }
        }
        fc::Type::Var(v) => Type::Var(Var::Named(v.as_str().into())),
        fc::Type::App { .. } => app(t),
        fc::Type::Fun {
            param,
            effects,
            result,
        } => {
            let stage = match effects {
                None => Stage::Pure,
                Some(e) => Stage::Runtime(row(&e.services), row(&e.errors)),
            };
            Type::fun(ty(param), stage, ty(result))
        }
        fc::Type::Tuple(ts) if ts.is_empty() => Type::con(PRELUDE, "Unit"),
        fc::Type::Tuple(ts) => Type::Tuple(ts.iter().map(ty).collect()),
        fc::Type::Record(r) => Type::Record(fields(r)),
        fc::Type::Captured {
            ty: inner,
            captures,
            ..
        } => Type::Captured(Rc::new(ty(inner)), caps(captures)),
        fc::Type::Bounded {
            ty: inner,
            captures,
        } => Type::Bounded(Rc::new(ty(inner)), caps(captures)),
        fc::Type::Forall { .. } => Type::Forall(Rc::new(scheme(t))),
        // a row written as the argument of a type: `Atom.Async a none`, as
        // instantiating a row variable there gives it (`generalize::substitute`)
        fc::Type::Row(_) | fc::Type::Union(..) | fc::Type::Without { .. } => Type::Errors(row(t)),
        other => Type::Con(Path::new("?", &format!("{other:?}"))),
    }
}

fn app(t: &fc::Type) -> Type {
    let (head, args) = t.spine();
    if let fc::Type::Con { module, name } = head
        && let Some(class) = name.strip_suffix("$Dict")
    {
        return Type::Dict(Path::new(module, class), args.into_iter().map(ty).collect());
    }
    // the forms of the core, by the lang item of their constructor: the
    // library may move or rename what it declares
    let form = |item| head.is_lang(item);
    match &args[..] {
        [r, e, a] if form(Lang::Eff) => return Type::Eff(row(r), row(e), Rc::new(ty(a))),
        [r, e] if form(Lang::Eff) => return Type::EffOf(row(r), row(e)),
        [i, e, o] if form(Lang::Layer) => return Type::Layer(row(i), row(e), row(o)),
        [r] if form(Lang::Errors) => return Type::Errors(row(r)),
        [a] if form(Lang::Ref) => return Type::Ref(Rc::new(ty(a))),
        [r] if form(Lang::ServiceImpl) => return Type::Impl(row(r)),
        [r] if form(Lang::ServiceTag) => return Type::Tag(row(r)),
        _ => {}
    }
    args.into_iter().fold(ty(head), |h, a| Type::app(h, ty(a)))
}

/// A row of services or errors.
pub fn row(t: &fc::Type) -> Row {
    let mut out = Row::none();
    add(t, &mut out);
    out
}

fn add(t: &fc::Type, out: &mut Row) {
    match t {
        fc::Type::Row(r) => {
            for (label, element) in &r.entries {
                let (_, args) = element.spine();
                let args: Rc<[Type]> = args.into_iter().map(ty).collect();
                out.elems.insert(Path::parse(&label.to_string()), args);
            }
            if let Some(tail) = &r.tail {
                // The explicitly named instance is the only one of that
                // name in the row. Its tail cannot supply the same service
                // or error at other arguments (E11); subtraction preserves
                // this fact when the named instance is handled.
                let gone = r
                    .entries
                    .iter()
                    .filter(|(label, element)| label.is_instance(element))
                    .map(|(label, _)| Path::parse(&label.to_string()))
                    .collect();
                out.add_less(&row(tail), &gone);
            }
        }
        fc::Type::Union(a, b) => {
            add(a, out);
            add(b, out);
        }
        fc::Type::Var(v) => {
            out.add_var(&Var::Named(v.as_str().into()), &BTreeSet::new());
        }
        fc::Type::Without { row: inner, gone } => {
            let gone = gone.iter().map(|l| Path::parse(&l.to_string())).collect();
            out.add_less(&row(inner), &gone);
        }
        fc::Type::Captured { ty: inner, .. } | fc::Type::Bounded { ty: inner, .. } => {
            add(inner, out);
        }
        _ => {}
    }
}

/// A set of captures: the atoms a row of them names, and its tails. A tail
/// may be a region the scheme binds; instantiating it makes it an atom.
pub fn caps(t: &fc::Type) -> Caps {
    let mut out = Caps::default();
    add_caps(t, &mut out);
    out
}

fn add_caps(t: &fc::Type, out: &mut Caps) {
    match t {
        fc::Type::Row(r) => {
            for (_, atom) in &r.entries {
                match atom {
                    fc::Type::Var(v) => {
                        out.atoms.insert(Var::Named(v.as_str().into()));
                    }
                    other => add_caps(other, out),
                }
            }
            if let Some(tail) = &r.tail {
                add_caps(tail, out);
            }
        }
        fc::Type::Union(a, b) => {
            add_caps(a, out);
            add_caps(b, out);
        }
        fc::Type::Var(v) => {
            out.vars.insert(Var::Named(v.as_str().into()));
        }
        _ => {}
    }
}

fn fields(r: &fc::Row) -> Fields {
    let fields: BTreeMap<Rc<str>, Type> = r
        .entries
        .iter()
        .map(|(l, t)| (Rc::from(l.to_string()), ty(t)))
        .collect();
    let tail = match r.tail.as_deref() {
        Some(fc::Type::Var(v)) => Some(Var::Named(v.as_str().into())),
        _ => None,
    };
    Fields { fields, tail }
}
