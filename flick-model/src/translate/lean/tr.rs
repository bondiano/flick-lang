//! Rules 1–4 of the translation of Prelude schemes
//! (`docs/formal/elaboration.md#value-qualifiers-in-prelude-schemes`) on one checker type: `Tr::ty`, then `Tr::resolve`,
//! with the elision of rule 6 on the way.

use std::collections::{BTreeMap, BTreeSet};
use std::{iter, mem};

use flick_core::types::{self as fc, Kind, show_type};

use super::Skip;

/// Types with hidden storage, with the number of their leading rows, which
/// the model drops: its `Eff q b` runs with empty rows (`docs/formal/elaboration.md#value-qualifiers-in-prelude-schemes`).
const STORED: [(&str, usize); 2] = [("Seq", 0), ("Eff", 2)];

/// A type of `Elab`, its variables by name.
#[derive(Clone)]
pub(super) enum Ty {
    Var(String),
    Con(String),
    App(Box<Ty>, Box<Ty>),
    /// `.arr φ a b q`: `P` without rows, `R r e` with them
    Arr(Option<(Box<Ty>, Box<Ty>)>, Box<Ty>, Box<Ty>, Box<Ty>),
    Nil,
    Un(Box<Ty>, Box<Ty>),
    /// `.rnm c t`: the row of one service or error type `t` named `c`
    Nm(String, Box<Ty>),
    /// `.run r r'`: a union of rows
    RUn(Box<Ty>, Box<Ty>),
    /// `.all k t`: the binder of a parameter, its variables by name
    All(Vec<String>, Box<Ty>),
    /// a positive qualifier, its variables of the checker, before rule 4
    Pos(Vec<String>),
}

/// `x₁ ∪ … ∪ xₙ`, nested to the left.
fn cun(xs: Vec<Ty>) -> Ty {
    xs.into_iter()
        .reduce(|a, b| Ty::Un(Box::new(a), Box::new(b)))
        .unwrap_or(Ty::Nil)
}

/// `∀names. t`, or `t` without names.
fn all(names: Vec<String>, t: Ty) -> Ty {
    if names.is_empty() {
        t
    } else {
        Ty::All(names, Box::new(t))
    }
}

fn app(head: Ty, args: impl IntoIterator<Item = Ty>) -> Ty {
    args.into_iter()
        .fold(head, |h, a| Ty::App(Box::new(h), Box::new(a)))
}

#[derive(Default)]
pub(super) struct Tr {
    /// the variables over containers: of kind `Type → Type`
    pub containers: BTreeSet<String>,
    /// `v := a` of rule 2
    inst: BTreeMap<String, String>,
    /// variables that stayed themselves at a negative position
    kept: BTreeSet<String>,
    /// what each qualified argument reaches, by its variable (rule 4)
    reach: BTreeMap<String, Vec<Ty>>,
    pub fresh: Vec<String>,
    /// the type constructors, in the order they appear
    pub cons: Vec<String>,
    /// the variables of kind `CaptureRow` or `Region`: what elision keeps live
    pub live_kinds: BTreeSet<String>,
    /// what the parameters of the stages before reach, for the next runtime arrow (rule 6)
    held: Vec<String>,
    /// rule 6 as the checker's `for_loop` reads `for … do`: only the first
    /// parameter, the body, is kept live, nothing of the source
    pub callback_only: bool,
    /// the arrows on the way to the result so far
    stages: usize,
    /// the value bounds `a^c` on variables of the scheme, rule 1: `(a, c)`,
    /// a fact of the scheme
    pub bounds: Vec<(Ty, Ty)>,
    /// the binders of the `∀` of parameters open here
    inner: Vec<String>,
}

/// What a qualified argument reaches besides its qualifier.
enum Site {
    Data(Vec<Ty>),
    /// a function: the new variables from this one on
    Fun(usize),
}

impl Tr {
    fn new_var(&mut self) -> Ty {
        let name = format!("'{}", self.fresh.len());
        self.fresh.push(name.clone());
        Ty::Var(name)
    }

    fn con(&mut self, c: String) -> Ty {
        if !self.cons.contains(&c) {
            self.cons.push(c.clone());
        }
        Ty::Con(c)
    }

    pub fn ty(&mut self, t: &fc::Type, neg: bool) -> Result<Ty, Skip> {
        // only the result of a stage, a qualified type and a `∀` pass on what is held
        let held = mem::take(&mut self.held);
        Ok(match t {
            fc::Type::Var(a) => Ty::Var(a.clone()),
            fc::Type::Con { name, .. } => self.con(name.to_lowercase()),
            fc::Type::App { .. } => self.data(t, None, neg)?,
            fc::Type::Fun {
                param,
                effects,
                result,
            } => self.arrow(param, effects.as_deref(), result, None, neg, held)?,
            // `(a, b, c)` is `tup (tup a b) c`, as `nest` of `Sugar`
            fc::Type::Tuple(ts) if ts.len() > 1 => {
                let tup = self.con("tup".into());
                let ts = ts
                    .iter()
                    .map(|t| self.ty(t, neg))
                    .collect::<Result<Vec<_>, _>>()?;
                ts.into_iter()
                    .reduce(|a, b| app(tup.clone(), [a, b]))
                    .expect("a pair")
            }
            // a closed record is the type `rec_ℓ̄` of its fields, by label
            fc::Type::Record(fc::Row {
                entries,
                tail: None,
                ..
            }) => {
                let labels: Vec<String> = entries.iter().map(|(l, _)| l.to_string()).collect();
                let h = self.con(format!("rec_{}", labels.join("_")));
                let fields = entries
                    .iter()
                    .map(|(_, t)| self.ty(t, neg))
                    .collect::<Result<Vec<_>, _>>()?;
                app(h, fields)
            }
            fc::Type::Row(_) | fc::Type::Union(..) => self.row(t)?,
            fc::Type::Bounded { ty, captures } => {
                let fc::Type::Var(a) = &**ty else {
                    return Err(Skip::Other(
                        "a value bound on a type that is not a variable".into(),
                    ));
                };
                self.bound(a, captures)?
            }
            fc::Type::Captured { ty, captures, .. } => {
                let (names, inner) = self.open(ty)?;
                let t = match inner {
                    fc::Type::Fun {
                        param,
                        effects,
                        result,
                    } => {
                        self.arrow(param, effects.as_deref(), result, Some(captures), neg, held)?
                    }
                    fc::Type::App { .. } => self.data(inner, Some(captures), neg)?,
                    // a hidden qualifier on a type variable adds nothing
                    _ => {
                        self.held = held;
                        self.ty(inner, neg)?
                    }
                };
                self.close(&names);
                all(names, t)
            }
            fc::Type::Forall { .. } => {
                let (names, body) = self.open(t)?;
                self.held = held;
                let t = self.ty(body, neg)?;
                self.close(&names);
                all(names, t)
            }
            other => return Err(Skip::Other(format!("no type {}", show_type(other)))),
        })
    }

    /// The variables of a `∀` that stay binders, and its body: `∀v. a^v → …`
    /// has `v := a` (rule 2). A constraint would need a dictionary in the type.
    fn open<'t>(&mut self, t: &'t fc::Type) -> Result<(Vec<String>, &'t fc::Type), Skip> {
        let fc::Type::Forall {
            vars,
            constraints,
            body,
        } = t
        else {
            return Ok((Vec::new(), t));
        };
        if !constraints.is_empty() {
            return Err(Skip::Rank);
        }
        let mut names = Vec::new();
        for v in vars {
            if matches!(v.kind, Kind::CaptureRow | Kind::Region) {
                self.live_kinds.insert(v.name.clone());
            }
            match bound(body, &v.name) {
                Some(a) => {
                    self.inst.insert(v.name.clone(), a);
                }
                None => names.push(v.name.clone()),
            }
        }
        self.inner.extend(names.iter().cloned());
        Ok((names, body))
    }

    /// The binders `names` of a `∀` of a parameter are out of scope again.
    fn close(&mut self, names: &[String]) {
        self.inner.truncate(self.inner.len() - names.len());
    }

    /// `a^c` on a type variable: `∀v. a^v` of rule 2 is `a`, and a bound on
    /// a variable of the scheme is `a` with the fact `(a, c)` (rule 1).
    fn bound(&mut self, a: &String, captures: &fc::Type) -> Result<Ty, Skip> {
        if matches!(captures, fc::Type::Var(v) if self.inst.get(v) == Some(a)) {
            return Ok(Ty::Var(a.clone()));
        }
        let mut atoms = Vec::new();
        self.atoms(captures, &mut atoms)?;
        if iter::once(a).chain(&atoms).any(|v| self.inner.contains(v)) {
            return Err(Skip::Other(
                "a value bound over a variable of a `∀` of a parameter".into(),
            ));
        }
        let c = cun(atoms.into_iter().map(Ty::Var).collect());
        self.bounds.push((Ty::Var(a.clone()), c));
        Ok(Ty::Var(a.clone()))
    }

    /// An arrow, its row with `Live` of what `held` and its parameter reach
    /// (rule 6, the checker's `elide`): `Live C` is the capture row `C` in
    /// the row, ending in `.cnil` so that one variable is not read as a row
    /// variable (`Ty.Keeps` of `Elab/Source.lean`).
    fn arrow(
        &mut self,
        param: &fc::Type,
        effects: Option<&fc::Effects>,
        result: &fc::Type,
        q: Option<&fc::Type>,
        neg: bool,
        mut held: Vec<String>,
    ) -> Result<Ty, Skip> {
        let rows = effects
            .map(|e| Ok((self.row(&e.services)?, Box::new(self.row(&e.errors)?))))
            .transpose()?;
        let start = self.fresh.len();
        let a = self.ty(param, !neg)?;
        let mut reached = Vec::new();
        if !neg {
            self.stages += 1;
        }
        if !(self.callback_only && !neg && self.stages > 1) {
            self.reached(&a, &[], &mut reached);
        }
        for v in reached {
            if !held.contains(&v) {
                held.push(v);
            }
        }
        let rows = rows.map(|(r, e)| {
            let live = (!held.is_empty()).then(|| {
                let c = cun(held.iter().cloned().map(Ty::Var).collect());
                Ty::Un(Box::new(c), Box::new(Ty::Nil))
            });
            let r = match live {
                Some(live) => Ty::RUn(Box::new(live), Box::new(r)),
                None => r,
            };
            (Box::new(r), e)
        });
        self.held = held;
        let b = self.ty(result, neg)?;
        let q = match q {
            Some(q) => self.qual(q, neg, Site::Fun(start))?,
            None => Ty::Nil,
        };
        Ok(Ty::Arr(rows, Box::new(a), Box::new(b), Box::new(q)))
    }

    /// The variables of kind `CaptureRow` or `Region` a value of `t` reaches,
    /// but those `bound` binds: `Ty.Shown`, without the rows of arrows.
    fn reached(&self, t: &Ty, bound: &[String], out: &mut Vec<String>) {
        let push = |v: &String, out: &mut Vec<String>| {
            let live = self.live_kinds.contains(v) || self.fresh.contains(v);
            if live && !bound.contains(v) && !out.contains(v) {
                out.push(v.clone());
            }
        };
        match t {
            Ty::Var(v) => push(v, out),
            Ty::Pos(vs) => vs.iter().for_each(|v| push(v, out)),
            Ty::App(a, b) | Ty::Un(a, b) | Ty::RUn(a, b) => {
                self.reached(a, bound, out);
                self.reached(b, bound, out);
            }
            Ty::Arr(_, a, b, q) => {
                self.reached(a, bound, out);
                self.reached(b, bound, out);
                self.reached(q, bound, out);
            }
            Ty::Nm(_, t) => self.reached(t, bound, out),
            Ty::All(names, t) => {
                let inner: Vec<String> = bound.iter().chain(names).cloned().collect();
                self.reached(t, &inner, out);
            }
            Ty::Con(_) | Ty::Nil => {}
        }
    }

    /// `T ā` with the qualifier `q` of its value.
    fn data(&mut self, t: &fc::Type, q: Option<&fc::Type>, neg: bool) -> Result<Ty, Skip> {
        let (head, args) = t.spine();
        let stored = match head {
            fc::Type::Var(m) => self.containers.contains(m).then_some(0),
            fc::Type::Con { name, .. } => STORED.iter().find(|(s, _)| s == name).map(|s| s.1),
            _ => None,
        };
        let h = self.ty(head, neg)?;
        let args = args
            .into_iter()
            .skip(stored.unwrap_or(0))
            .map(|a| self.ty(a, neg))
            .collect::<Result<Vec<_>, _>>()?;
        if stored.is_none() {
            return Ok(app(h, args));
        }
        let q = match q {
            Some(q) => self.qual(q, neg, Site::Data(args.clone()))?,
            None => self.new_var(),
        };
        Ok(app(h, iter::once(q).chain(args)))
    }

    fn qual(&mut self, q: &fc::Type, neg: bool, site: Site) -> Result<Ty, Skip> {
        let mut atoms = Vec::new();
        self.atoms(q, &mut atoms)?;
        if !neg {
            return Ok(Ty::Pos(atoms));
        }
        let var = match &atoms[..] {
            [v] if self.kept.insert(v.clone()) => Ty::Var(v.clone()),
            _ => self.new_var(),
        };
        if let [v] = &atoms[..] {
            let rest: Vec<Ty> = match site {
                // what the arguments reach, as rule 6 and the checker's
                // `live_atoms` count it: no type variable
                Site::Data(args) => {
                    let mut reached = Vec::new();
                    for a in &args {
                        self.reached(a, &[], &mut reached);
                    }
                    reached.into_iter().map(Ty::Var).collect()
                }
                Site::Fun(start) => self.fresh[start..]
                    .iter()
                    .filter(|w| !matches!(&var, Ty::Var(v) if v == *w))
                    .cloned()
                    .map(Ty::Var)
                    .collect(),
            };
            let reach = cun(iter::once(var.clone()).chain(rest).collect());
            self.reach.entry(v.clone()).or_default().push(reach);
        }
        Ok(var)
    }

    /// The variables of a capture row: `{a, b | c}` holds what `a` and `b`
    /// reach and `c`.
    fn atoms(&self, q: &fc::Type, out: &mut Vec<String>) -> Result<(), Skip> {
        match q {
            fc::Type::Var(v) => out.push(self.inst.get(v).unwrap_or(v).clone()),
            fc::Type::Union(a, b) => {
                self.atoms(a, out)?;
                self.atoms(b, out)?;
            }
            fc::Type::Row(row) => {
                for (_, t) in &row.entries {
                    self.atoms(t, out)?;
                }
                if let Some(tail) = &row.tail {
                    self.atoms(tail, out)?;
                }
            }
            other => return Err(Skip::Other(format!("no qualifier {}", show_type(other)))),
        }
        Ok(())
    }

    /// Rule 4 on the positive qualifiers.
    pub fn resolve(&self, t: Ty) -> Ty {
        self.resolve_under(t, &[])
    }

    /// [`Tr::resolve`] under the binders of `bound`, whose variables stay.
    fn resolve_under(&self, t: Ty, bound: &[String]) -> Ty {
        let go = |t: Box<Ty>| Box::new(self.resolve_under(*t, bound));
        match t {
            Ty::Pos(atoms) => cun(atoms
                .into_iter()
                .map(|v| match self.reach.get(&v) {
                    Some(sites) if !bound.contains(&v) => cun(sites.clone()),
                    _ => Ty::Var(v),
                })
                .collect()),
            Ty::App(a, b) => Ty::App(go(a), go(b)),
            Ty::Arr(rows, a, b, q) => Ty::Arr(rows, go(a), go(b), go(q)),
            Ty::Un(a, b) => Ty::Un(go(a), go(b)),
            Ty::Nm(c, t) => Ty::Nm(c, go(t)),
            Ty::RUn(a, b) => Ty::RUn(go(a), go(b)),
            Ty::All(names, t) => {
                let inner: Vec<String> = bound.iter().chain(&names).cloned().collect();
                let t = Box::new(self.resolve_under(*t, &inner));
                Ty::All(names, t)
            }
            leaf => leaf,
        }
    }
}

impl Tr {
    /// A row of services or errors: a variable, `.cnil`, `{S₁, …, Sₙ | r}`
    /// as the union of its elements and its tail, `ρ₁ + ρ₂`.
    fn row(&mut self, t: &fc::Type) -> Result<Ty, Skip> {
        Ok(match t {
            fc::Type::Var(v) => Ty::Var(v.clone()),
            fc::Type::Union(a, b) => Ty::RUn(Box::new(self.row(a)?), Box::new(self.row(b)?)),
            fc::Type::Row(fc::Row { entries, tail, .. }) => {
                let mut parts = Vec::new();
                for (_, element) in entries {
                    let fc::Type::Con { name, .. } = element.spine().0 else {
                        return Err(Skip::Other(format!("a row element {}", show_type(element))));
                    };
                    let c = name.to_lowercase();
                    let t = self.ty(element, false)?;
                    parts.push(Ty::Nm(c, Box::new(t)));
                }
                if let Some(tail) = tail {
                    parts.push(self.row(tail)?);
                }
                parts
                    .into_iter()
                    .reduce(|a, b| Ty::RUn(Box::new(a), Box::new(b)))
                    .unwrap_or(Ty::Nil)
            }
            other => return Err(Skip::Other(format!("no row {}", show_type(other)))),
        })
    }
}

/// The type variable `a` of `a^v` in `t`.
fn bound(t: &fc::Type, v: &str) -> Option<String> {
    match t {
        fc::Type::Bounded { ty, captures } => match (&**ty, &**captures) {
            (fc::Type::Var(a), fc::Type::Var(w)) if w == v => Some(a.clone()),
            _ => bound(ty, v),
        },
        fc::Type::Fun { param, result, .. } => bound(param, v).or_else(|| bound(result, v)),
        fc::Type::Captured { ty, .. } => bound(ty, v),
        _ => None,
    }
}

pub(super) fn occurs(t: &Ty, v: &str) -> bool {
    match t {
        Ty::Var(w) => w == v,
        Ty::App(a, b) | Ty::Un(a, b) | Ty::RUn(a, b) => occurs(a, v) || occurs(b, v),
        Ty::Nm(_, t) => occurs(t, v),
        Ty::All(names, t) => !names.iter().any(|n| n == v) && occurs(t, v),
        Ty::Arr(rows, a, b, q) => {
            occurs(a, v)
                || occurs(b, v)
                || occurs(q, v)
                || rows
                    .as_ref()
                    .is_some_and(|(r, e)| occurs(r, v) || occurs(e, v))
        }
        _ => false,
    }
}

#[cfg(test)]
mod resolve_tests {
    use super::{Tr, Ty};

    #[test]
    fn a_binder_keeps_its_own_variable_from_what_one_outside_reaches() {
        let mut tr = Tr::default();
        tr.reach.insert("s".into(), vec![Ty::Var("site".into())]);
        let shadowed = Ty::All(vec!["s".into()], Box::new(Ty::Pos(vec!["s".into()])));
        let Ty::All(_, inner) = tr.resolve(shadowed) else {
            panic!("a binder stays a binder");
        };
        assert!(matches!(*inner, Ty::Var(v) if v == "s"));
    }
}
