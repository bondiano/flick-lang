//! Types of flick-core to types of the model. Class constraints become
//! dictionary parameters (`docs/formal/core.md` §1.6), `Fiber.Outlives`
//! the facts `Θ` of a scheme (`docs/formal/regions.md` §1.2),
//! `Fiber.Provided` an element of a row (§2.7); regions are
//! variables of kind `Region`, qualifiers `τ^C`; the prelude's `Eff`, `Ref`,
//! `Layer`, `Errors` and `S.Impl` become the forms of §1.2.

use std::collections::{BTreeMap, BTreeSet, HashMap};
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

pub fn binders(vars: &[Binder]) -> Vec<(Rc<str>, Kind)> {
    vars.iter()
        .filter_map(|b| Some((b.name.as_str().into(), kind(&b.kind)?)))
        .collect()
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

/// Translation uses the declared kinds of nominal parameters. An empty
/// capture row and an empty service row have the same source representation.
pub(crate) struct Types<'a> {
    params: &'a HashMap<Path, Vec<Kind>>,
    vars: Vec<(Rc<str>, Kind)>,
}

impl<'a> Types<'a> {
    pub fn new(params: &'a HashMap<Path, Vec<Kind>>) -> Self {
        Self {
            params,
            vars: Vec::new(),
        }
    }

    fn under(&self, vars: &[Binder]) -> Self {
        let mut inner = Self {
            params: self.params,
            vars: self.vars.clone(),
        };
        inner.vars.extend(binders(vars));
        inner
    }
    /// Class constraints pass dictionaries; Outlives stays in Θ.
    pub fn scheme(&self, t: &fc::Type) -> Scheme {
        match t {
            fc::Type::Forall {
                vars,
                constraints,
                body,
            } => {
                let inner = self.under(vars);
                let mut out = inner.ty(body);
                for c in constraints.iter().rev().filter(|c| passes(c)) {
                    out = Type::fun(inner.constraint(c), Stage::Pure, out);
                }
                for c in constraints.iter().filter(|c| !passes(c)) {
                    if let (head, args) = c.spine()
                        && let [r, s] = &args[..]
                        && head.is_lang(Lang::Provided)
                    {
                        out = out.with_provided(&inner.row(r), &inner.ty(s));
                    }
                }
                let outlives = constraints
                    .iter()
                    .filter(|c| !passes(c))
                    .filter_map(|c| match c.spine() {
                        (head, args) if head.is_lang(Lang::Outlives) => match &args[..] {
                            [c, s] => Some((caps(c), inner.ty(s))),
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
                body: self.ty(other),
                outlives: Vec::new(),
            },
        }
    }

    /// The dictionary type of a constraint C τ̄, with kinded arguments.
    pub fn constraint(&self, c: &fc::Type) -> Type {
        let (head, args) = c.spine();
        match head {
            fc::Type::Con { module, name } => {
                Type::Dict(Path::new(module, name), self.arguments(head, &args).into())
            }
            other => Type::Con(Path::new("?", &format!("{other:?}"))),
        }
    }

    pub fn ty(&self, t: &fc::Type) -> Type {
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
            fc::Type::App { .. } => self.app(t),
            fc::Type::Fun {
                param,
                effects,
                result,
            } => {
                let stage = match effects {
                    None => Stage::Pure,
                    Some(e) => Stage::Runtime(self.row(&e.services), self.row(&e.errors)),
                };
                Type::fun(self.ty(param), stage, self.ty(result))
            }
            fc::Type::Tuple(ts) if ts.is_empty() => Type::con(PRELUDE, "Unit"),
            fc::Type::Tuple(ts) => Type::Tuple(ts.iter().map(|t| self.ty(t)).collect()),
            fc::Type::Record(r) => Type::Record(self.fields(r)),
            fc::Type::Captured {
                ty: inner,
                captures,
                ..
            } => Type::Captured(Rc::new(self.ty(inner)), caps(captures)),
            fc::Type::Bounded {
                ty: inner,
                captures,
            } => Type::Bounded(Rc::new(self.ty(inner)), caps(captures)),
            fc::Type::Forall { .. } => Type::Forall(Rc::new(self.scheme(t))),
            // a row written as the argument of a type: `Atom.Async a none`, as
            // instantiating a row variable there gives it (`generalize::substitute`)
            fc::Type::Row(_) | fc::Type::Union(..) | fc::Type::Without { .. } => {
                Type::Errors(self.row(t))
            }
            other => Type::Con(Path::new("?", &format!("{other:?}"))),
        }
    }

    fn app(&self, t: &fc::Type) -> Type {
        let (head, args) = t.spine();
        if let fc::Type::Con { module, name } = head
            && let Some(class) = name.strip_suffix("$Dict")
        {
            return Type::Dict(
                Path::new(module, class),
                self.arguments(&fc::Type::con(module, class), &args).into(),
            );
        }
        // the forms of the core, by the lang item of their constructor: the
        // library may move or rename what it declares
        let form = |item| head.is_lang(item);
        match &args[..] {
            [r, e, a] if form(Lang::Eff) => {
                return Type::Eff(self.row(r), self.row(e), Rc::new(self.ty(a)));
            }
            [r, e] if form(Lang::Eff) => return Type::EffOf(self.row(r), self.row(e)),
            [i, e, o] if form(Lang::Layer) => {
                return Type::Layer(self.row(i), self.row(e), self.row(o));
            }
            [r] if form(Lang::Errors) => return Type::Errors(self.row(r)),
            [a] if form(Lang::Ref) => return Type::Ref(Rc::new(self.ty(a))),
            [r] if form(Lang::ServiceImpl) => return Type::Impl(self.row(r)),
            [r] if form(Lang::ServiceTag) => return Type::Tag(self.row(r)),
            _ => {}
        }
        self.arguments(head, &args)
            .into_iter()
            .fold(self.ty(head), Type::app)
    }

    fn arguments(&self, head: &fc::Type, args: &[&fc::Type]) -> Vec<Type> {
        let params = match head {
            fc::Type::Con { module, name } => self
                .params
                .get(&Path::new(module, name))
                .cloned()
                .unwrap_or_default(),
            fc::Type::Var(n) => {
                let mut result = Vec::new();
                let mut k = self
                    .vars
                    .iter()
                    .rev()
                    .find(|(v, _)| &**v == n)
                    .map(|(_, k)| k);
                while let Some(Kind::Arrow(a, b)) = k {
                    result.push((**a).clone());
                    k = Some(b);
                }
                result
            }
            _ => Vec::new(),
        };
        args.iter()
            .enumerate()
            .map(|(i, a)| {
                if params.get(i) == Some(&Kind::Captures) {
                    Type::Caps(caps(a))
                } else {
                    self.ty(a)
                }
            })
            .collect()
    }

    pub fn row(&self, t: &fc::Type) -> Row {
        let mut out = Row::none();
        self.add(t, &mut out);
        out
    }

    fn add(&self, t: &fc::Type, out: &mut Row) {
        match t {
            fc::Type::Row(r) => {
                for (label, element) in &r.entries {
                    let (head, args) = element.spine();
                    let args: Rc<[Type]> = self.arguments(head, &args).into();
                    out.elems.insert(Path::parse(&label.to_string()), args);
                }
                if let Some(tail) = &r.tail {
                    // Union retains an overlapping instance. Well-formedness
                    // requires its arguments to agree; it is not subtraction.
                    out.add_less(&self.row(tail), &BTreeSet::new());
                }
            }
            fc::Type::Union(a, b) => {
                self.add(a, out);
                self.add(b, out);
            }
            fc::Type::Var(v) => {
                out.add_var(Var::Named(v.as_str().into()), &BTreeSet::new());
            }
            fc::Type::Without { row: inner, gone } => {
                let gone = gone.iter().map(|l| Path::parse(&l.to_string())).collect();
                out.add_less(&self.row(inner), &gone);
            }
            fc::Type::Captured { ty: inner, .. } | fc::Type::Bounded { ty: inner, .. } => {
                self.add(inner, out);
            }
            _ => {}
        }
    }

    fn fields(&self, r: &fc::Row) -> Fields {
        let fields: BTreeMap<Rc<str>, Type> = r
            .entries
            .iter()
            .map(|(l, t)| (Rc::from(l.to_string()), self.ty(t)))
            .collect();
        let tail = match r.tail.as_deref() {
            Some(fc::Type::Var(v)) => Some(Var::Named(v.as_str().into())),
            _ => None,
        };
        Fields { fields, tail }
    }
}

/// Translate an isolated scheme (the Prelude certificate generator).
pub fn scheme(t: &fc::Type) -> Scheme {
    Types::new(&HashMap::new()).scheme(t)
}
