//! The model's elision of the Prelude schemes as theorems about `Ty.elide`
//! of the Lean core (`formal/Formal/Live/ElideSchemes.lean`): each scheme
//! and its reading by `check::elided` printed as core types, de Bruijn, with
//! `K` the binders of kind `Region` and `CaptureRow`, which the model's
//! `shown` counts. A capture row is a set of atoms, a row a set of elements,
//! both as predicates; `Live C` in a row is the atoms of `C`, `Scope ϱ` the
//! atom `ϱ`. A value bound `a^c` on a variable of the scheme is the fact
//! `Facts.bnd a ⟦c⟧` of the scheme and `a` in its type, as rule 1 of the
//! scheme translation reads it; `∀v. a^v → …` of a parameter is `v := a`,
//! as rule 2 reads it. What the core has no form for is skipped, with its
//! reason.

use std::cell::RefCell;
use std::fmt::Write;
use std::rc::Rc;

use flick_core::types::{builtins, show_type};

use crate::check::{elided, live_service, scope_service};
use crate::decls::Decls;
use crate::syntax::Path;
use crate::translate::types::scheme;
use crate::types::{Caps, Kind, Row, Scheme, Stage, Type, Var};

/// Types with hidden storage: the qualifier of a value is their first
/// argument, as `appQ` of the scheme translation.
const STORED: [&str; 1] = ["Seq"];

/// The value bounds a translation meets, by their variable.
type Bounds = Rc<RefCell<Vec<(String, Caps)>>>;

/// The binders in scope, innermost first, with the capture rows rule 2 has
/// instantiated: `v ↦ a`.
#[derive(Clone, Default)]
pub(crate) struct Env {
    pub(crate) vars: Vec<String>,
    pub(crate) inst: Vec<(String, String)>,
    /// A meta the check left open is the least instance of its kind: a type
    /// `Unit`, a row, capture row or record's rest empty. A certificate's typing holds at
    /// any instance of them, and Lean checks it at this one.
    pub(crate) ground: bool,
    /// The classes by the index the core gives their dictionaries; with
    /// none, every dictionary is of class 0.
    pub(crate) classes: Vec<String>,
    /// The value bounds on variables of the scheme met so far, rule 1: each
    /// is a fact of the scheme. Without it a bound is not read, as a
    /// certificate's type has no facts.
    pub(crate) bounds: Option<Bounds>,
    /// The binders of the `∀` of parameters around: the variables of the
    /// scheme come after them in `vars`.
    pub(crate) inner: usize,
}

impl Env {
    /// The variables of `v` the core writes: an open meta of a row or a
    /// capture row is empty in a grounded one.
    fn kept<'v>(&self, vs: impl Iterator<Item = &'v Var>) -> impl Iterator<Item = &'v Var> {
        let ground = self.ground;
        vs.filter(move |v| !(ground && matches!(v, Var::Meta(_))))
    }

    fn index(&self, v: &str) -> Result<usize, String> {
        self.vars
            .iter()
            .position(|w| w == v)
            .ok_or_else(|| format!("a free variable `{v}`"))
    }

    /// The index of the atom `v`, after rule 2.
    fn index_of(&self, v: &Var) -> Result<usize, String> {
        match v {
            Var::Named(n) => {
                let n = self
                    .inst
                    .iter()
                    .find(|(v, _)| v == &**n)
                    .map_or(&**n, |(_, a)| a);
                self.index(n)
            }
            Var::Meta(_) => Err("a meta".into()),
        }
    }
}

fn disjunction(parts: &[String], empty: &str) -> String {
    if parts.is_empty() {
        empty.to_owned()
    } else {
        format!("(fun x => {})", parts.join(" ∨ "))
    }
}

pub(crate) fn caps(c: &Caps, env: &Env) -> Result<String, String> {
    let atoms = c
        .atoms
        .iter()
        .chain(env.kept(c.vars.iter()))
        .map(|v| Ok(format!("x = .v {}", env.index_of(v)?)))
        .collect::<Result<Vec<_>, String>>()?;
    Ok(disjunction(&atoms, "Caps.none"))
}

pub(crate) fn row(r: &Row, env: &Env) -> Result<String, String> {
    let mut parts = Vec::new();
    for (name, args) in &r.elems {
        if *name == live_service() || *name == scope_service() {
            let c = match &args[..] {
                [Type::Captured(_, c)] => c.clone(),
                [Type::Var(v)] => Caps::atom(v.clone()),
                _ => return Err(format!("`{name}` with arguments the core does not write")),
            };
            for a in c.atoms.iter().chain(&c.vars) {
                parts.push(format!("x = .atm (.v {})", env.index_of(a)?));
            }
        } else if args.is_empty() {
            parts.push(format!("x = .nm \"{name}\""));
        } else {
            return Err(format!("the element `{name}` with arguments"));
        }
    }
    for v in env.kept(r.vars.iter()) {
        parts.push(format!("x = .rv (.v {})", env.index_of(v)?));
    }
    Ok(disjunction(&parts, "Row.none"))
}

pub(crate) fn stage(s: &Stage, env: &Env) -> Result<String, String> {
    Ok(match s {
        Stage::Pure => ".pure".into(),
        Stage::Runtime(r, e) => format!("(.rt {} {})", row(r, env)?, row(e, env)?),
    })
}

fn con(p: &Path) -> String {
    match (&*p.module, &*p.name) {
        ("Prelude", "Int") => "(.base .int)".into(),
        ("Prelude", "Bool") => "(.base .bool)".into(),
        ("Prelude", "Unit") => "(.base .unit)".into(),
        ("Prelude", "String") => "(.base .string)".into(),
        _ => format!("(.adt \"{p}\")"),
    }
}

/// `T ā` with the qualifier `c` of its value: the first argument of a type
/// with hidden storage, dropped by data without it.
fn data(t: &Type, c: Option<&Caps>, env: &Env) -> Result<String, String> {
    let mut args = Vec::new();
    let mut head = t;
    while let Type::App(h, a) = head {
        args.push(&**a);
        head = h;
    }
    args.reverse();
    // the owner of a region is the core's own type
    if let (Type::Con(p), [s]) = (head, &args[..])
        && p.to_string() == "Fiber.Owner"
    {
        return Ok(format!("(.owner {})", ty(s, env)?));
    }
    let mut out = ty(head, env)?;
    if let (Type::Con(p), Some(c)) = (head, c)
        && STORED.contains(&&*p.name)
    {
        out = format!("(.tapp {out} (.caps {}))", caps(c, env)?);
    }
    for a in args {
        out = format!("(.tapp {out} {})", ty(a, env)?);
    }
    Ok(out)
}

pub(crate) fn ty(t: &Type, env: &Env) -> Result<String, String> {
    qualified(t, None, env)
}

fn qualified(t: &Type, q: Option<&Caps>, env: &Env) -> Result<String, String> {
    let c = |env: &Env| q.map_or(Ok("Caps.none".into()), |q| caps(q, env));
    Ok(match t {
        Type::Var(Var::Named(n)) => format!("(.tvar {})", env.index(n)?),
        Type::Var(Var::Meta(_)) if env.ground => "(.base .unit)".into(),
        Type::Con(p) => con(p),
        Type::App(..) => data(t, q, env)?,
        Type::Fun(p, s, r) => {
            let (p, s, r) = (ty(p, env)?, stage(s, env)?, ty(r, env)?);
            format!("(.arrow {p} {s} {r} {})", c(env)?)
        }
        Type::Eff(r, e, a) => {
            let (r, e, a) = (row(r, env)?, row(e, env)?, ty(a, env)?);
            format!("(.eff {r} {e} {a} {})", c(env)?)
        }
        Type::Captured(x, inner) => qualified(x, Some(inner), env)?,
        Type::Bounded(x, b) => {
            let bound: Vec<&Var> = b.atoms.iter().chain(&b.vars).collect();
            match (&**x, &bound[..]) {
                // `a^v` of a `∀v` rule 2 instantiated
                (Type::Var(Var::Named(a)), [Var::Named(v)])
                    if env.inst.iter().any(|(w, i)| w == &**v && i == &**a) =>
                {
                    ty(x, env)?
                }
                // a certificate reads a bound as the core does: on data the
                // data, `⌈τ⌉ ⊆ C` checked where a cell takes it (`Const.ok`);
                // on a function or a computation the qualifier of its use
                _ if env.ground && env.bounds.is_none() => {
                    let mut inner = &**x;
                    while let Type::Captured(y, _) | Type::Bounded(y, _) = inner {
                        inner = y;
                    }
                    match inner {
                        Type::Fun(..) | Type::Eff(..) => qualified(inner, Some(b), env)?,
                        _ => ty(x, env)?,
                    }
                }
                _ => {
                    let Some(bounds) = &env.bounds else {
                        return Err("a value bound, which the core does not have".into());
                    };
                    let Type::Var(Var::Named(a)) = &**x else {
                        return Err("a value bound on a type that is not a variable".into());
                    };
                    let outer = |v: &Var| env.index_of(v).is_ok_and(|i| i >= env.inner);
                    if !outer(&Var::Named(a.clone())) || !bound.iter().all(|v| outer(v)) {
                        return Err("a value bound over a variable of a `∀` of a parameter".into());
                    }
                    bounds.borrow_mut().push((a.to_string(), b.clone()));
                    ty(x, env)?
                }
            }
        }
        Type::Ref(x) => format!("(.ref {})", ty(x, env)?),
        Type::Record(f)
            if f.tail.is_none() || env.ground && matches!(f.tail, Some(Var::Meta(_))) =>
        {
            let mut out = ".rnil".to_owned();
            for (l, t) in f.fields.iter().rev() {
                out = format!("(.rcons \"{l}\" {} {out})", ty(t, env)?);
            }
            out
        }
        Type::Tuple(ts) => {
            let ts = ts
                .iter()
                .map(|t| ty(t, env))
                .collect::<Result<Vec<_>, _>>()?;
            ts.into_iter()
                .reduce(|a, b| format!("(.tapp (.tapp (.adt \"Tuple\") {a}) {b})"))
                .unwrap_or_else(|| "(.base .unit)".into())
        }
        Type::Errors(r) => format!("(.errors {})", row(r, env)?),
        Type::Dict(c, args) => {
            let args = args
                .iter()
                .map(|a| ty(a, env))
                .collect::<Result<Vec<_>, _>>()?;
            let class = if env.classes.is_empty() {
                0
            } else {
                (env.classes.iter().position(|k| *k == c.to_string()))
                    .ok_or_else(|| format!("the class `{c}`, not in the signature"))?
            };
            format!("(.dict {class} (Ty.spine [{}]))", args.join(", "))
        }
        Type::Forall(s) => forall(s, env)?,
        other => return Err(format!("no core form for `{other}`")),
    })
}

/// A `∀` of a parameter: rule 2 instantiates `v` of `a^v`; a binder of kind
/// `Region` or `CaptureRow` that stays is one the model's `elide` counts and
/// `Ty.elide` under `.all` does not.
fn forall(s: &Scheme, env: &Env) -> Result<String, String> {
    let mut inner = env.clone();
    let mut kept = Vec::new();
    for (v, k) in &s.vars {
        match bound_to(&s.body, v) {
            Some(a) if *k == Kind::Captures => inner.inst.push((v.to_string(), a)),
            _ if matches!(k, Kind::Region | Kind::Captures) => {
                return Err(format!("a `∀` of a parameter binds `{v}` of kind {k:?}"));
            }
            _ => kept.push(v.to_string()),
        }
    }
    inner.inner = env.inner + kept.len();
    inner.vars = kept
        .iter()
        .cloned()
        .chain(env.vars.iter().cloned())
        .collect();
    let body = ty(&s.body, &inner)?;
    Ok(if kept.is_empty() {
        body
    } else {
        format!("(.all {} {body})", kept.len())
    })
}

/// `a` when the capture row `v` only bounds a variable `a`, as in `a^v`.
fn bound_to(t: &Type, v: &str) -> Option<String> {
    let mut found = None;
    let ok = visit(t, &mut |t| match t {
        Type::Bounded(x, b)
            if b.vars
                .iter()
                .any(|w| matches!(w, Var::Named(w) if &**w == v)) =>
        {
            match (&**x, b.vars.len() + b.atoms.len()) {
                (Type::Var(Var::Named(a)), 1) if found.as_deref().is_none_or(|f| f == &**a) => {
                    found = Some(a.to_string());
                    true
                }
                _ => false,
            }
        }
        _ => true,
    });
    found.filter(|_| ok)
}

/// `f` on each type of `t`, outer first, while it holds.
fn visit(t: &Type, f: &mut dyn FnMut(&Type) -> bool) -> bool {
    if !f(t) {
        return false;
    }
    match t {
        Type::App(a, b) | Type::Fun(a, _, b) => visit(a, f) && visit(b, f),
        Type::Eff(_, _, a) | Type::Ref(a) | Type::Captured(a, _) | Type::Bounded(a, _) => {
            visit(a, f)
        }
        Type::Record(r) => r.fields.values().all(|t| visit(t, f)),
        Type::Tuple(ts) | Type::Dict(_, ts) => ts.iter().all(|t| visit(t, f)),
        Type::Forall(s) => visit(&s.body, f),
        _ => true,
    }
}

/// The facts of the value bounds of a scheme, rule 1: `Facts.bnd a ⟦c⟧`
/// for each, joined by `Facts.union`.
pub(crate) fn facts(bounds: &[(String, Caps)], env: &Env) -> Result<Option<String>, String> {
    let mut parts: Vec<String> = Vec::new();
    for (a, c) in bounds {
        let fact = format!("Facts.bnd {} {}", env.index(a)?, caps(c, env)?);
        if !parts.contains(&fact) {
            parts.push(fact);
        }
    }
    Ok(parts
        .into_iter()
        .reduce(|a, b| format!("Facts.union ({a}) ({b})")))
}

/// The theorem of one scheme: `Ty.elide K ⟦σ⟧ Caps.none = ⟦elided σ⟧`, or
/// of a scheme with value bounds `Scheme.elide K ⟨k, Θ, ⟦σ⟧⟩ = ⟨k, Θ, ⟦elided σ⟧⟩`.
pub(super) fn theorem(name: &str, doc: &str, s: &Scheme, live: &Scheme) -> Result<String, String> {
    let bounds = Rc::new(RefCell::new(Vec::new()));
    let env = Env {
        vars: s.vars.iter().map(|(v, _)| v.to_string()).collect(),
        bounds: Some(bounds.clone()),
        ..Env::default()
    };
    let k: Vec<String> = s
        .vars
        .iter()
        .enumerate()
        .filter(|(_, (_, k))| matches!(k, Kind::Region | Kind::Captures))
        .map(|(i, _)| format!("x = .v {i}"))
        .collect();
    let k = disjunction(&k, "Caps.none");
    let src = ty(&s.body, &env)?;
    let facts = facts(&bounds.take(), &env)?;
    let out = ty(&live.body, &env)?;
    let mut text = format!("/-- `{doc}` -/\ntheorem {name} :\n");
    let runtime = !visit(&s.body, &mut |t| {
        !matches!(t, Type::Fun(_, Stage::Runtime(..), _) | Type::Eff(..))
    });
    match facts {
        None => {
            let _ = write!(text, "    Ty.elide {k} {src} Caps.none =\n      {out}");
        }
        // the model's elision keeps the bound live after its parameter, `Ty.elide` the variable
        Some(_) if runtime => {
            return Err("a value bound in a scheme with a runtime stage".into());
        }
        Some(facts) => {
            let n = s.vars.len();
            let _ = write!(
                text,
                "    Scheme.elide {k} ⟨{n}, {facts},\n      {src}⟩ =\n      ⟨{n}, {facts},\n      {out}⟩"
            );
        }
    }
    text.push_str(" := by\n  elide_eq\n");
    Ok(text)
}

/// The text of `formal/Formal/Live/ElideSchemes.lean`.
pub fn elide_schemes() -> String {
    let mut decls = Decls::default();
    for i in builtins() {
        decls.add(i);
    }
    let mut out = String::from(
        "-- Code generated by `just formal-schemes`. DO NOT EDIT.\n\n\
         import Formal.Live.Elide\n\n\
         /-!\n\
         # The model's elision is `Ty.elide`\n\n\
         Each export of `Prelude`, `Seq`, `Option`, `Result` and `List` the core can write, as the model\n\
         takes it (`flick-model/src/translate/types.rs`) and as the model reads it (`check::elided`):\n\
         `Ty.elide K` of the first is the second, `K` the binders of kind `Region` and `CaptureRow`\n\
         (`flick-model/src/translate/lean/elide.rs`). A value bound `a^c` on a variable of the scheme\n\
         is the fact `Facts.bnd a c`, which `Scheme.elide` keeps. The exports the core cannot write\n\
         close the file.\n\
         -/\n\n\
         namespace Flick.ElideSchemes\n",
    );
    let mut skipped = String::new();
    for (module, export) in super::exports() {
        let t = export.scheme.as_ref().expect("filtered");
        let s = scheme(t);
        let live = elided(&decls, &s);
        let name = &export.entity.name;
        let doc = format!("{module}.{name} : {}", show_type(t));
        match theorem(&super::lean_name(module, name), &doc, &s, &live) {
            Ok(text) => {
                out.push('\n');
                out.push_str(&text);
            }
            Err(why) => {
                let _ = writeln!(skipped, "-- skip {module}.{name}: {why}");
            }
        }
    }
    out.push_str("\nend Flick.ElideSchemes\n");
    if !skipped.is_empty() {
        out.push_str("\n/-! ## Skipped -/\n\n");
        out.push_str(&skipped);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    /// `ElideSchemes.lean` is what the model generates; with
    /// `FLICK_FORMAL_SCHEMES` set the test writes it.
    #[test]
    fn elide_schemes_are_generated() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../formal/Formal/Live/ElideSchemes.lean"
        );
        let text = super::elide_schemes();
        if env::var_os("FLICK_FORMAL_SCHEMES").is_some() {
            fs::write(path, &text).expect("write ElideSchemes.lean");
        }
        let committed = fs::read_to_string(path).unwrap_or_default();
        assert!(
            committed == text,
            "ElideSchemes.lean is stale: run `just formal-schemes`"
        );
    }

    /// Rule 1 reads a bound on a variable of the scheme as a fact; a bound
    /// it cannot read so is skipped with its reason.
    #[test]
    fn a_value_bound_is_a_fact_of_the_scheme() {
        use std::rc::Rc;

        use crate::types::{Caps, Kind, Row, Scheme, Stage, Type, Var};

        let var = |n: &str| Var::Named(n.into());
        let a = Rc::new(Type::Var(var("a")));
        let unit = || Rc::new(Type::Tuple(Rc::new([])));
        let bounded = |t: Rc<Type>| Rc::new(Type::Bounded(t, Caps::atom(var("c"))));
        let scheme = |param: Rc<Type>, stage: Stage| {
            Scheme::new(
                vec![("a".into(), Kind::Type), ("c".into(), Kind::Captures)],
                Type::Fun(param, stage, unit()),
            )
        };
        let theorem = |s: &Scheme| super::theorem("t", "t", s, s);
        let pure = theorem(&scheme(bounded(a.clone()), Stage::Pure)).expect("a fact");
        assert!(
            pure.contains("Scheme.elide (fun x => x = .v 1) ⟨2, Facts.bnd 0 (fun x => x = .v 1),")
        );
        let runtime = Stage::Runtime(Row::none(), Row::none());
        assert_eq!(
            theorem(&scheme(bounded(a), runtime)),
            Err("a value bound in a scheme with a runtime stage".into())
        );
        assert_eq!(
            theorem(&scheme(bounded(unit()), Stage::Pure)),
            Err("a value bound on a type that is not a variable".into())
        );
    }
}
