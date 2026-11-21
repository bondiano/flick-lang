//! Type substitution, existential binder rewriting, witness scanning and
//! projection checks supporting `check/modules.rs`.

use std::collections::HashMap;
use std::rc::Rc;

use super::super::dicts::arg;
use super::super::generalize::{Inst, substitute};
use crate::decls::ModuleContract;
use crate::subst::binders;
use crate::syntax::{Arm, Name, Path, Term};
use crate::types::{
    CONTRACT_SCOPE, EXISTS_PREFIX, Fields, Row, Scheme, Stage, Type, manifest_module, witness_id,
};

pub(super) fn abstract_member_map<'a>(
    contract: &'a ModuleContract,
    args: &[Type],
) -> HashMap<&'a str, Type> {
    contract
        .abstract_types
        .iter()
        .zip(args)
        .map(|((name, _), ty)| (&**name, ty.clone()))
        .collect()
}

pub(super) fn map_type_children(
    t: &Type,
    on_ty: &impl Fn(&Type) -> Type,
    on_path: &impl Fn(&Path) -> Path,
) -> Type {
    let row = |r: &Row| Row {
        elems: r
            .elems
            .iter()
            .map(|(n, args)| (on_path(n), args.iter().map(on_ty).collect()))
            .collect(),
        vars: r.vars.clone(),
        less: r
            .less
            .iter()
            .map(|(v, n)| (v.clone(), on_path(n)))
            .collect(),
    };
    let stage = |s: &Stage| match s {
        Stage::Pure => Stage::Pure,
        Stage::Runtime(sr, er) => Stage::Runtime(row(sr), row(er)),
    };
    match t {
        Type::App(h, a) => Type::app(on_ty(h), on_ty(a)),
        Type::Fun(p, s, r) => Type::fun(on_ty(p), stage(s), on_ty(r)),
        Type::Eff(r, e, a) => Type::Eff(row(r), row(e), Rc::new(on_ty(a))),
        Type::EffOf(r, e) => Type::EffOf(row(r), row(e)),
        Type::Ref(a) => Type::Ref(Rc::new(on_ty(a))),
        Type::Layer(i, e, o) => Type::Layer(row(i), row(e), row(o)),
        Type::Record(f) => Type::Record(Fields {
            fields: f
                .fields
                .iter()
                .map(|(l, x)| (l.clone(), on_ty(x)))
                .collect(),
            tail: f.tail.clone(),
        }),
        Type::Tuple(ts) => Type::Tuple(ts.iter().map(on_ty).collect()),
        Type::Dict(c, args) => Type::Dict(on_path(c), args.iter().map(on_ty).collect()),
        Type::Errors(r) => Type::Errors(row(r)),
        Type::Impl(r) => Type::Impl(row(r)),
        Type::Tag(r) => Type::Tag(row(r)),
        Type::Env(r) => Type::Env(row(r)),
        Type::Forall(s) => Type::Forall(Rc::new(Scheme {
            vars: s.vars.clone(),
            body: on_ty(&s.body),
            outlives: s
                .outlives
                .iter()
                .map(|(c, r)| (c.clone(), on_ty(r)))
                .collect(),
        })),
        Type::Exists(k, sig, b) => Type::Exists(k.clone(), sig.clone(), Rc::new(on_ty(b))),
        Type::OpenedExists(sig, w, b) => Type::OpenedExists(sig.clone(), *w, Rc::new(on_ty(b))),
        Type::Captured(x, c) => Type::Captured(Rc::new(on_ty(x)), c.clone()),
        Type::Bounded(x, c) => Type::Bounded(Rc::new(on_ty(x)), c.clone()),
        Type::Var(_) | Type::Con(_) | Type::Caps(_) | Type::Module(_) | Type::OpenedModule(..) => {
            t.clone()
        }
    }
}

pub(super) fn substitute_contract_scheme(
    scheme: &Scheme,
    abstract_map: &HashMap<&str, Type>,
) -> Scheme {
    if abstract_map.is_empty() {
        return scheme.clone();
    }
    Scheme {
        vars: scheme.vars.clone(),
        body: substitute_contract_type(&scheme.body, abstract_map),
        outlives: scheme
            .outlives
            .iter()
            .map(|(c, r)| (c.clone(), substitute_contract_type(r, abstract_map)))
            .collect(),
    }
}

pub(super) fn substitute_contract_type(t: &Type, abstract_map: &HashMap<&str, Type>) -> Type {
    if abstract_map.is_empty() {
        return t.clone();
    }
    if let Type::Con(p) = t
        && &*p.module == CONTRACT_SCOPE
    {
        return abstract_map
            .get(&*p.name)
            .cloned()
            .unwrap_or_else(|| t.clone());
    }
    map_type_children(
        t,
        &|x| substitute_contract_type(x, abstract_map),
        &Path::clone,
    )
}

pub(super) struct ExistsRewriter<'a> {
    pub(super) binder: &'a Name,
    pub(super) prefix: &'a str,
    pub(super) sig: &'a Path,
    pub(super) contract: &'a ModuleContract,
    pub(super) abstract_map: &'a HashMap<&'a str, Type>,
}

impl ExistsRewriter<'_> {
    fn rewrite_path(&self, p: &Path) -> Path {
        if &*p.module == self.prefix {
            self.contract
                .nominals
                .get(&p.name)
                .cloned()
                .unwrap_or_else(|| p.clone())
        } else {
            p.clone()
        }
    }

    pub(super) fn rewrite(&self, t: &Type, fuel: usize) -> Type {
        if fuel == 0 {
            return t.clone();
        }
        match t {
            Type::Con(p) if &*p.module == self.prefix => {
                if let Some(ty) = self.abstract_map.get(&*p.name) {
                    return ty.clone();
                }
                if let Some((params, body)) = self.contract.manifest_types.get(&p.name) {
                    return if params.is_empty() {
                        substitute_contract_type(body, self.abstract_map)
                    } else {
                        Type::Con(Path::new(&manifest_module(self.sig), &p.name))
                    };
                }
                if let Some(entity) = self.contract.nominals.get(&p.name) {
                    return Type::Con(entity.clone());
                }
                t.clone()
            }
            Type::App(..) => {
                let mut spine_args = Vec::new();
                let mut head = t;
                while let Type::App(h, a) = head {
                    spine_args.push(&**a);
                    head = h;
                }
                spine_args.reverse();
                if let Type::Con(p) = head
                    && &*p.module == self.prefix
                    && let Some((params, body)) = self.contract.manifest_types.get(&p.name)
                    && !params.is_empty()
                    && spine_args.len() >= params.len()
                {
                    let mapped_args: Vec<Type> = spine_args
                        .into_iter()
                        .map(|a| self.rewrite(a, fuel - 1))
                        .collect();
                    let map: HashMap<Name, Inst> = params
                        .iter()
                        .cloned()
                        .zip(mapped_args[..params.len()].iter().map(arg))
                        .collect();
                    let body = substitute_contract_type(body, self.abstract_map);
                    let mut out = substitute(&body, &map);
                    for extra in &mapped_args[params.len()..] {
                        out = Type::app(out, extra.clone());
                    }
                    return out;
                }
                map_type_children(t, &|x| self.rewrite(x, fuel - 1), &|p| self.rewrite_path(p))
            }
            Type::Exists(inner_binder, ..) if inner_binder == self.binder => t.clone(),
            _ => map_type_children(t, &|x| self.rewrite(x, fuel - 1), &|p| self.rewrite_path(p)),
        }
    }
}

/// Whether `t` mentions witness `w` as a naked witness type outside an
/// `OpenedModule(_, w)` / `OpenedExists(_, w, _)` wrapper.
pub(super) fn has_naked_witness(t: &Type, w: u32) -> bool {
    let row = |r: &Row| {
        r.elems.iter().any(|(n, args)| {
            witness_id(&n.module) == Some(w) || args.iter().any(|a| has_naked_witness(a, w))
        })
    };
    match t {
        Type::Con(p) => witness_id(&p.module) == Some(w),
        Type::OpenedModule(_, id) | Type::OpenedExists(_, id, _) if *id == w => false,
        Type::Var(_) | Type::Caps(_) | Type::Module(_) | Type::OpenedModule(..) => false,
        Type::App(h, a) => has_naked_witness(h, w) || has_naked_witness(a, w),
        Type::Fun(p, s, r) => {
            has_naked_witness(p, w)
                || matches!(s, Stage::Runtime(sr, er) if row(sr) || row(er))
                || has_naked_witness(r, w)
        }
        Type::Eff(r, e, a) => row(r) || row(e) || has_naked_witness(a, w),
        Type::EffOf(r, e) => row(r) || row(e),
        Type::Ref(a) | Type::Captured(a, _) | Type::Bounded(a, _) => has_naked_witness(a, w),
        Type::Layer(i, e, o) => row(i) || row(e) || row(o),
        Type::Record(f) => f.fields.values().any(|x| has_naked_witness(x, w)),
        Type::Tuple(ts) | Type::Dict(_, ts) => ts.iter().any(|x| has_naked_witness(x, w)),
        Type::Errors(r) | Type::Impl(r) | Type::Tag(r) | Type::Env(r) => row(r),
        Type::Forall(s) => has_naked_witness(&s.body, w),
        Type::Exists(_, _, b) | Type::OpenedExists(_, _, b) => has_naked_witness(b, w),
    }
}

pub(super) fn close_opened_containers(t: &Type, w: u32) -> Type {
    match t {
        Type::OpenedModule(sig, id) if *id == w => Type::Module(sig.clone()),
        Type::OpenedExists(sig, id, body) if *id == w => {
            let binder: Name = "$w".into();
            let closed = replace_witness_with_exists(body, w, &binder);
            Type::Exists(binder, sig.clone(), Rc::new(closed))
        }
        _ => map_type_children(t, &|x| close_opened_containers(x, w), &Path::clone),
    }
}

fn replace_witness_with_exists(t: &Type, w: u32, binder: &Name) -> Type {
    let target = format!("{EXISTS_PREFIX}{binder}");
    let on_path = |p: &Path| {
        if witness_id(&p.module) == Some(w) {
            Path::new(&target, &p.name)
        } else {
            p.clone()
        }
    };
    if let Type::Con(p) = t
        && witness_id(&p.module) == Some(w)
    {
        return Type::Con(Path::new(&target, &p.name));
    }
    map_type_children(t, &|x| replace_witness_with_exists(x, w, binder), &on_path)
}

pub(super) fn find_witness(t: &Type, pred: &dyn Fn(u32) -> bool, out: &mut Option<u32>) {
    if out.is_some() {
        return;
    }
    let row = |r: &Row, out: &mut Option<u32>| {
        for (n, args) in &r.elems {
            if let Some(id) = witness_id(&n.module)
                && pred(id)
            {
                *out = Some(id);
                return;
            }
            for a in args.iter() {
                find_witness(a, pred, out);
            }
        }
    };
    match t {
        Type::Con(p) => {
            if let Some(id) = witness_id(&p.module)
                && pred(id)
            {
                *out = Some(id);
            }
        }
        Type::OpenedModule(_, id) => {
            if pred(*id) {
                *out = Some(*id);
            }
        }
        Type::OpenedExists(_, id, body) => {
            if pred(*id) {
                *out = Some(*id);
                return;
            }
            find_witness(body, pred, out);
        }
        Type::Var(_) | Type::Caps(_) | Type::Module(_) => {}
        Type::App(h, a) => {
            find_witness(h, pred, out);
            find_witness(a, pred, out);
        }
        Type::Fun(p, s, r) => {
            find_witness(p, pred, out);
            if let Stage::Runtime(sr, er) = s {
                row(sr, out);
                row(er, out);
            }
            find_witness(r, pred, out);
        }
        Type::Eff(r, e, a) => {
            row(r, out);
            row(e, out);
            find_witness(a, pred, out);
        }
        Type::EffOf(r, e) => {
            row(r, out);
            row(e, out);
        }
        Type::Ref(a) | Type::Captured(a, _) | Type::Bounded(a, _) => find_witness(a, pred, out),
        Type::Layer(i, e, o) => {
            row(i, out);
            row(e, out);
            row(o, out);
        }
        Type::Record(f) => f.fields.values().for_each(|x| find_witness(x, pred, out)),
        Type::Tuple(ts) | Type::Dict(_, ts) => ts.iter().for_each(|x| find_witness(x, pred, out)),
        Type::Errors(r) | Type::Impl(r) | Type::Tag(r) | Type::Env(r) => row(r, out),
        Type::Forall(s) => find_witness(&s.body, pred, out),
        Type::Exists(_, _, b) => find_witness(b, pred, out),
    }
}

/// Whether `e` contains a free field projection `x.field` (respecting shadowing).
pub(super) fn is_projected(x: &Name, e: &Term) -> bool {
    match e {
        Term::Field(target, _) => {
            matches!(&**target, Term::Var(v) if v == x) || is_projected(x, target)
        }
        Term::Lam(p, body) | Term::Fix(p, _, body) | Term::Scope(p, body) => {
            p != x && is_projected(x, body)
        }
        Term::Let(y, _, e1, e2) => is_projected(x, e1) || (y != x && is_projected(x, e2)),
        Term::LetRec(f, _, e1, e2) => f != x && (is_projected(x, e1) || is_projected(x, e2)),
        Term::App(a, b)
        | Term::WithOwnerEvidence(a, b)
        | Term::ActivateOwner(a, b)
        | Term::Update(a, _, b)
        | Term::Provide(a, b)
        | Term::Join(a, b) => is_projected(x, a) || is_projected(x, b),
        Term::Task(a)
        | Term::Await(a)
        | Term::Bind(_, a)
        | Term::With(_, a)
        | Term::Raise(a)
        | Term::Settle(_, a)
        | Term::Inside(_, _, a) => is_projected(x, a),
        Term::Tuple(xs) | Term::List(xs) | Term::Structural(_, xs) | Term::Apply(_, xs) => {
            xs.iter().any(|a| is_projected(x, a))
        }
        Term::Record(fs) => fs.iter().any(|(_, a)| is_projected(x, a)),
        Term::Match(scrut, arms) | Term::Try(scrut, arms) | Term::Handle(scrut, arms) => {
            is_projected(x, scrut) || arms_project(x, arms)
        }
        Term::Handler(h) => h.terms().any(|t| is_projected(x, t)) || arms_project(x, &h.errors),
        Term::Var(_)
        | Term::Global(_)
        | Term::Lit(_)
        | Term::Tag(_)
        | Term::Stuck(_)
        | Term::Loc(_)
        | Term::Env(_)
        | Term::Die(_)
        | Term::OwnerCandidate(_)
        | Term::ReadOwnerEvidence
        | Term::DefaultOwner
        | Term::Own(_)
        | Term::Cont(_)
        | Term::Signal(_)
        | Term::Cap(..) => false,
    }
}

fn arms_project(x: &Name, arms: &[Arm]) -> bool {
    arms.iter().any(|arm| {
        let mut names = Vec::new();
        binders(&arm.pat, &mut names);
        if names.contains(x) {
            return false;
        }
        arm.guard.as_ref().is_some_and(|g| is_projected(x, g)) || is_projected(x, &arm.body)
    })
}
