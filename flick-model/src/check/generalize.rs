//! Instances of schemes (`docs/formal/core.md` §2.3 (Var), (Let-Sig)):
//! `τ[ᾱ := τ̄]` with fresh metas or with the rigid variables of a
//! signature, and `Gen` (§2.5) by levels: a meta made inside the right
//! side of a `let` and not reachable from `Γ` is quantified.

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use super::regions::regions_of;
use super::{Checker, Solved};
use crate::syntax::Name;
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};

/// What a variable of a scheme becomes.
#[derive(Clone, Debug)]
pub(crate) enum Inst {
    Type(Type),
    Row(Row),
    Tail(Var),
    Caps(Caps),
}

/// `s` with its regions and capture rows the rigid variables `rigid` its
/// body is checked at, by the order of `vars`: a recursive use of a signed
/// function goes through the regions of its signature, not fresh ones.
pub(super) fn own_regions(s: &Scheme, vars: &[(Name, Kind)], rigid: &[Name]) -> Scheme {
    let own = |k: &Kind| matches!(k, Kind::Region | Kind::Captures);
    let map: HashMap<Name, Inst> = (vars.iter().zip(rigid))
        .filter(|((_, k), _)| own(k))
        .map(|((n, k), r)| (n.clone(), rigid_inst(r, k)))
        .collect();
    Scheme {
        vars: s
            .vars
            .iter()
            .filter(|(n, _)| !map.contains_key(n))
            .cloned()
            .collect(),
        body: substitute(&s.body, &map),
        outlives: (s.outlives.iter())
            .map(|(c, r)| (substitute_caps(c, &map), substitute(r, &map)))
            .collect(),
    }
}

/// What a variable of kind `k` becomes as the rigid variable `name`.
pub(super) fn rigid_inst(name: &str, k: &Kind) -> Inst {
    let v = Var::Named(name.into());
    match k {
        Kind::Services | Kind::Errors => Inst::Row(Row::var(v)),
        Kind::Fields => Inst::Tail(v),
        Kind::Captures => Inst::Caps(Caps::var(v)),
        _ => Inst::Type(Type::Var(v)),
    }
}

impl Checker<'_> {
    pub(super) fn fresh_inst(&mut self, k: &Kind) -> Inst {
        let v = Var::Meta(self.meta());
        if *k == Kind::Region {
            self.regions.kinds.insert(v.clone());
        }
        match k {
            Kind::Services | Kind::Errors => Inst::Row(Row::var(v)),
            Kind::Fields => Inst::Tail(v),
            Kind::Captures => Inst::Caps(Caps::var(v)),
            _ => Inst::Type(Type::Var(v)),
        }
    }

    /// `τ[ᾱ := τ̄]` with fresh metas: the instance (Var) takes, with the
    /// premises of (Inst) on it (`regions.md` §2.3).
    pub(super) fn instantiate(&mut self, s: &Scheme) -> Type {
        let insts: Vec<Inst> = s.vars.iter().map(|(_, k)| self.fresh_inst(k)).collect();
        let map = names(&s.vars, &insts);
        self.instance_premises(s, &map);
        substitute(&s.body, &map)
    }

    /// [`Self::instantiate`], and what each variable became.
    pub(super) fn instantiate_with(&mut self, s: &Scheme) -> (Type, Vec<Inst>) {
        let insts: Vec<Inst> = s.vars.iter().map(|(_, k)| self.fresh_inst(k)).collect();
        let map = names(&s.vars, &insts);
        self.instance_premises(s, &map);
        (substitute(&s.body, &map), insts)
    }

    /// Eliminate a constructor that already established its instance
    /// premises. Bounds on its payload stay in the field types; Outlives
    /// premises become facts of the matching arm, not new obligations.
    pub(super) fn instantiate_pattern(&mut self, s: &Scheme) -> (Type, Vec<Inst>) {
        let insts: Vec<Inst> = s.vars.iter().map(|(_, k)| self.fresh_inst(k)).collect();
        let map = names(&s.vars, &insts);
        for (c, r) in &s.outlives {
            self.regions
                .facts
                .push((substitute_caps(c, &map), substitute(r, &map)));
        }
        let body = substitute(&s.body, &map);
        self.remember_row_instances(&body);
        (body, insts)
    }

    /// The body of `s` with its variables rigid and the stages of its
    /// signature holding their parameters: what (Let-Sig) checks, with the
    /// names of the rigid variables. Its facts `Θ` are given from here on.
    /// When `bind_tyvars` is true, its binders enter scope for local
    /// signatures inside its body.
    pub(super) fn skolemize(&mut self, s: &Scheme, bind_tyvars: bool) -> (Type, Vec<Name>) {
        let mut rigid = Vec::new();
        let insts: Vec<Inst> = s
            .vars
            .iter()
            .map(|(n, k)| {
                self.rigid += 1;
                let name = format!("{n}'{}", self.rigid);
                if *k == Kind::Region {
                    self.regions.kinds.insert(Var::Named(name.as_str().into()));
                }
                rigid.push(Name::from(name.as_str()));
                rigid_inst(&name, k)
            })
            .collect();
        if bind_tyvars {
            for ((n, _), inst) in s.vars.iter().zip(&insts) {
                self.tyvars.push((n.clone(), inst.clone()));
            }
        }
        let map = names(&s.vars, &insts);
        self.give_bounds(s, &map);
        for (c, r) in &s.outlives {
            let fact = (substitute_caps(c, &map), substitute(r, &map));
            self.regions.facts.push(fact);
        }
        let body = self.elide(
            &s.body,
            &Caps::default(),
            &regions_of(&s.vars, &BTreeSet::new()),
        );
        let body = substitute(&body, &map);
        self.remember_row_instances(&body);
        self.give_provided(&body);
        (body, rigid)
    }

    /// `s` with free variables from enclosing signatures and `scope`s replaced
    /// by their active rigid variables, respecting shadowing by `s.vars`.
    pub(super) fn in_enclosing(&self, s: &Scheme) -> Scheme {
        if self.tyvars.is_empty() {
            return s.clone();
        }
        let mut map = HashMap::new();
        for (n, inst) in &self.tyvars {
            if !s.vars.iter().any(|(v, _)| v == n) {
                map.insert(n.clone(), inst.clone());
            }
        }
        if map.is_empty() {
            return s.clone();
        }
        Scheme {
            vars: s.vars.clone(),
            body: substitute(&s.body, &map),
            outlives: s
                .outlives
                .iter()
                .map(|(c, r)| (substitute_caps(c, &map), substitute(r, &map)))
                .collect(),
        }
    }

    /// `Gen(Γ, τ)`: the type metas of `t` deeper than the current level.
    /// Rows are not quantified: they are settled to the least (`docs/formal/core.md#unknown-call-rows-in-a-generalized-binding-close-to-none`).
    pub(super) fn generalize(&mut self, t: &Type) -> Scheme {
        let mut ms = BTreeSet::new();
        self.metas(t, &mut ms);
        let region_metas = self.open_region_metas();
        // a scheme carries no inclusion: a meta one still waits on stays
        // monomorphic, and the inclusion settles it later
        let mut waiting = BTreeSet::new();
        for (a, b, _) in &self.pending {
            self.metas(&Type::Errors(a.clone()), &mut waiting);
            self.metas(&Type::Errors(b.clone()), &mut waiting);
        }
        for (src, elem, _) in &self.iterables {
            self.metas(src, &mut waiting);
            self.metas(elem, &mut waiting);
        }
        for m in region_metas.into_iter().chain(waiting) {
            self.slots[m as usize].level = self.slots[m as usize].level.min(self.level);
        }
        // Widened row tails local to this pure binding have finished gathering
        // constraints; close them before generalizing so distinct uses widen
        // independently.
        let widened = self.widened_rows.clone();
        for row in widened {
            for v in self.zonk_row(&row).vars {
                if let Var::Meta(m) = v
                    && self.slots[m as usize].level > self.level
                    && self.slots[m as usize].solved.is_none()
                {
                    self.slots[m as usize].solved = Some(Solved::Row(Row::none()));
                }
            }
        }
        let zonked = self.zonk(t);
        let mut vars = Vec::new();
        for m in ms {
            let slot = &self.slots[m as usize];
            if slot.level <= self.level || slot.solved.is_some() {
                continue;
            }
            let name: Name = format!("g{m}").into();
            let named = Var::Named(name.clone());
            if self.is_type_meta(m, t) {
                self.slots[m as usize].solved = Some(Solved::Type(Type::Var(named)));
                vars.push((name, Kind::Type));
            } else if in_errors(&Var::Meta(m), &zonked) {
                // a row a type takes, as `Errors e`; a row only of a stage
                // is closed instead (`core.md` §2.5, item 5)
                self.slots[m as usize].solved = Some(Solved::Row(Row::var(named)));
                vars.push((name, Kind::Errors));
            }
        }
        Scheme {
            vars,
            body: self.zonk(t),
            outlives: Vec::new(),
        }
    }

    /// Whether meta `m` stands for a type in `t`, not for a row.
    fn is_type_meta(&self, m: u32, t: &Type) -> bool {
        self.mentions(&Var::Meta(m), t)
    }

    /// Whether the variable `v` stands for a type in `t`, not for a row.
    pub(super) fn mentions(&self, v: &Var, t: &Type) -> bool {
        match self.zonk(t) {
            Type::Var(x) => x == *v,
            Type::App(h, a) => self.mentions(v, &h) || self.mentions(v, &a),
            Type::Fun(p, _, r) => self.mentions(v, &p) || self.mentions(v, &r),
            Type::Eff(_, _, t) | Type::Ref(t) | Type::Captured(t, _) | Type::Bounded(t, _) => {
                self.mentions(v, &t)
            }
            Type::Record(f) => f.fields.values().any(|t| self.mentions(v, t)),
            Type::Tuple(ts) | Type::Dict(_, ts) => ts.iter().any(|t| self.mentions(v, t)),
            Type::Forall(s) => self.mentions(v, &s.body),
            Type::Exists(_, _, b) | Type::OpenedExists(_, _, b) => self.mentions(v, &b),
            _ => false,
        }
    }
}

/// Whether the row variable `v` is in a row a type of `t` takes, `Errors ρ`.
fn in_errors(v: &Var, t: &Type) -> bool {
    let any = |ts: &[Type]| ts.iter().any(|t| in_errors(v, t));
    match t {
        Type::Errors(r) => r.vars.contains(v) || r.elems.values().any(|a| any(a)),
        Type::App(h, a) | Type::Fun(h, _, a) => in_errors(v, h) || in_errors(v, a),
        Type::Eff(_, _, t) | Type::Ref(t) | Type::Captured(t, _) | Type::Bounded(t, _) => {
            in_errors(v, t)
        }
        Type::Record(f) => f.fields.values().any(|t| in_errors(v, t)),
        Type::Tuple(ts) | Type::Dict(_, ts) => any(ts),
        Type::Forall(s) => in_errors(v, &s.body),
        Type::Exists(_, _, b) | Type::OpenedExists(_, _, b) => in_errors(v, b),
        _ => false,
    }
}

pub(super) fn substitute_all(t: &Type, vars: &[(Name, Kind)], insts: &[Inst]) -> Type {
    substitute(t, &names(vars, insts))
}

pub(super) fn names(vars: &[(Name, Kind)], insts: &[Inst]) -> HashMap<Name, Inst> {
    vars.iter()
        .map(|(n, _)| n.clone())
        .zip(insts.iter().cloned())
        .collect()
}

/// `C[ᾱ := τ̄, c̄ := C̄, ē := ρ̄]`: an atom or tail that becomes a type or
/// a row of errors is what that type or row names.
pub(super) fn substitute_caps(c: &Caps, map: &HashMap<Name, Inst>) -> Caps {
    let mut out = Caps::default();
    let all = c.atoms.iter().map(|v| (v, true));
    for (v, atom) in all.chain(c.vars.iter().map(|v| (v, false))) {
        let inst = match v {
            Var::Named(n) => map.get(n),
            Var::Meta(_) => None,
        };
        match inst {
            Some(Inst::Caps(more)) => out = out.union(more),
            Some(Inst::Type(t)) => free_atoms(t, &mut out),
            Some(Inst::Row(r)) => row_atoms(r, &mut out),
            _ if atom => {
                out.atoms.insert(v.clone());
            }
            _ => {
                out.vars.insert(v.clone());
            }
        }
    }
    out
}

/// The variables of `t` as atoms, its qualifiers as they are: `⌈t⌉` of a
/// type without hidden storage.
fn free_atoms(t: &Type, out: &mut Caps) {
    match t {
        Type::Var(v) => {
            out.atoms.insert(v.clone());
        }
        Type::App(a, b) | Type::Fun(a, _, b) => {
            free_atoms(a, out);
            free_atoms(b, out);
        }
        Type::Eff(_, _, x) | Type::Ref(x) => free_atoms(x, out),
        Type::Captured(x, c) | Type::Bounded(x, c) => {
            *out = out.union(c);
            free_atoms(x, out);
        }
        Type::Tuple(ts) => ts.iter().for_each(|t| free_atoms(t, out)),
        Type::Record(f) => f.fields.values().for_each(|t| free_atoms(t, out)),
        Type::Errors(r) => row_atoms(r, out),
        Type::Caps(c) => *out = out.union(c),
        Type::Exists(_, _, b) | Type::OpenedExists(_, _, b) => free_atoms(b, out),
        _ => {}
    }
}

/// `⌈Errors ρ⌉`: what the errors of `ρ` name and its variables.
fn row_atoms(r: &Row, out: &mut Caps) {
    for args in r.elems.values() {
        args.iter().for_each(|t| free_atoms(t, out));
    }
    out.atoms.extend(r.vars.iter().cloned());
}

/// `σ[s̄ := ϱ̄]`: `s` with each region of `renamed` in place of its name, but
/// for a name the signature binds itself.
pub(crate) fn in_regions(s: &Scheme, renamed: &[(Name, Name)]) -> Scheme {
    let map: HashMap<Name, Inst> = renamed
        .iter()
        .filter(|(n, r)| n != r && !s.vars.iter().any(|(v, _)| v == n))
        .map(|(n, r)| (n.clone(), Inst::Type(Type::Var(Var::Named(r.clone())))))
        .collect();
    if map.is_empty() {
        return s.clone();
    }
    Scheme {
        vars: s.vars.clone(),
        body: substitute(&s.body, &map),
        outlives: (s.outlives.iter())
            .map(|(c, r)| (substitute_caps(c, &map), substitute(r, &map)))
            .collect(),
    }
}

/// `τ[ᾱ := τ̄]`.
pub(crate) fn substitute(t: &Type, map: &HashMap<Name, Inst>) -> Type {
    if map.is_empty() {
        return t.clone();
    }
    let s = |t: &Rc<Type>| Rc::new(substitute(t, map));
    match t {
        Type::Var(Var::Named(n)) => match map.get(n) {
            Some(Inst::Type(t)) => t.clone(),
            // a row as the argument of a type: `Fiber e a`, `Failure e`
            Some(Inst::Row(r)) => Type::Errors(r.clone()),
            Some(Inst::Caps(c)) => Type::Caps(c.clone()),
            _ => t.clone(),
        },
        Type::Captured(x, c) => Type::Captured(s(x), substitute_caps(c, map)),
        Type::Bounded(x, c) => Type::Bounded(s(x), substitute_caps(c, map)),
        Type::Var(_) | Type::Con(_) | Type::Module(_) | Type::OpenedModule(..) => t.clone(),
        Type::App(h, a) => Type::App(s(h), s(a)),
        Type::Fun(p, st, r) => Type::Fun(s(p), substitute_stage(st, map), s(r)),
        Type::Eff(r, e, x) => Type::Eff(substitute_row(r, map), substitute_row(e, map), s(x)),
        Type::EffOf(r, e) => Type::EffOf(substitute_row(r, map), substitute_row(e, map)),
        Type::Ref(x) => Type::Ref(s(x)),
        Type::Layer(i, e, o) => Type::Layer(
            substitute_row(i, map),
            substitute_row(e, map),
            substitute_row(o, map),
        ),
        Type::Record(f) => Type::Record(Fields {
            fields: f
                .fields
                .iter()
                .map(|(l, t)| (l.clone(), substitute(t, map)))
                .collect(),
            tail: match &f.tail {
                Some(Var::Named(n)) => match map.get(n) {
                    Some(Inst::Tail(v)) => Some(v.clone()),
                    _ => f.tail.clone(),
                },
                other => other.clone(),
            },
        }),
        Type::Tuple(ts) => Type::Tuple(ts.iter().map(|t| substitute(t, map)).collect()),
        Type::Errors(r) => Type::Errors(substitute_row(r, map)),
        Type::Caps(c) => Type::Caps(substitute_caps(c, map)),
        Type::Impl(r) => Type::Impl(substitute_row(r, map)),
        Type::Tag(r) => Type::Tag(substitute_row(r, map)),
        Type::Dict(c, ts) => Type::Dict(c.clone(), ts.iter().map(|t| substitute(t, map)).collect()),
        Type::Env(r) => Type::Env(substitute_row(r, map)),
        Type::Forall(sc) => {
            let inner: HashMap<Name, Inst> = map
                .iter()
                .filter(|(n, _)| !sc.vars.iter().any(|(v, _)| v == *n))
                .map(|(n, i)| (n.clone(), i.clone()))
                .collect();
            Type::Forall(Rc::new(Scheme {
                vars: sc.vars.clone(),
                body: substitute(&sc.body, &inner),
                outlives: sc
                    .outlives
                    .iter()
                    .map(|(c, r)| (substitute_caps(c, &inner), substitute(r, &inner)))
                    .collect(),
            }))
        }
        Type::Exists(k, sig, b) => Type::Exists(k.clone(), sig.clone(), s(b)),
        Type::OpenedExists(sig, w, b) => Type::OpenedExists(sig.clone(), *w, s(b)),
    }
}

fn substitute_stage(s: &Stage, map: &HashMap<Name, Inst>) -> Stage {
    match s {
        Stage::Pure => Stage::Pure,
        Stage::Runtime(r, e) => Stage::Runtime(substitute_row(r, map), substitute_row(e, map)),
    }
}

pub(super) fn substitute_row(r: &Row, map: &HashMap<Name, Inst>) -> Row {
    let mut out = Row {
        elems: r
            .elems
            .iter()
            .map(|(n, args)| (n.clone(), args.iter().map(|a| substitute(a, map)).collect()))
            .collect(),
        ..Row::default()
    };
    for v in &r.vars {
        let gone = r.less_of(v).cloned().collect();
        match v {
            Var::Named(n) => match map.get(n) {
                Some(Inst::Row(inner)) => out.add_less(inner, &gone),
                _ => out.add_var(v, &gone),
            },
            Var::Meta(_) => out.add_var(v, &gone),
        }
    }
    out
}

/// The parameters of a class as `args`.
pub(super) fn params(params: &[Name], args: &[Type]) -> HashMap<Name, Inst> {
    params
        .iter()
        .cloned()
        .zip(args.iter().map(|a| Inst::Type(a.clone())))
        .collect()
}
