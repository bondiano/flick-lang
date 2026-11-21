//! `formal/FormalCert/GenSig.lean`: the signature the programs of the
//! generator take from `Gen.fl` and the Prelude. `GenSig S` says that `S`
//! has its constructors and globals; a certificate assumes it of `S`, and
//! `GenSig.inhabited` gives a signature that has them, so the assumption is
//! no contradiction. A global is at its elided scheme, as the check
//! instantiates it (`check::elided`).

use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use flick_core::numeric::{Arith, Num};

use crate::check::{elided, prim_scheme};
use crate::decls::Decls;
use crate::generate::fixture_module;
use crate::syntax::{Name, Path, Prim};
use crate::translate::lean::elide::{Env, Types, hides, ty};
use crate::types::{Caps, Kind, Scheme, Stage, Type, Var};
use crate::variance::Sign;

/// A constructor of the core: its variables, fields and type.
pub(crate) struct Ctor {
    pub(crate) k: usize,
    pub(crate) fields: Vec<String>,
    pub(crate) adt: String,
}

/// A global by what it stands for.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Key {
    Prim(String),
    Value(Path),
}

/// The constructors and globals of the generator's programs.
pub struct Sig {
    pub(crate) ctors: BTreeMap<String, Ctor>,
    /// Each global with its index and scheme.
    pub(crate) globs: Vec<(Key, Scheme)>,
    /// The operations of each service, all `tail`.
    pub(crate) ops: BTreeMap<String, Vec<Op>>,
    /// The number of parameters of each service with operations.
    pub(crate) arity: BTreeMap<String, usize>,
    /// The classes of the generator, by the index of their dictionaries.
    pub(crate) classes: Vec<String>,
    /// Each class's number of parameters and fields, `$S` of a superclass
    /// `S` first, as schemes in the core.
    pub(super) dicts: Vec<(usize, Vec<(String, String)>)>,
    /// The kinds of the parameters of the data types, and those with hidden
    /// storage.
    pub(crate) types: Rc<Types>,
    /// The sign of each parameter of each type of the constructors, at its
    /// index in the core: after the hidden row of data with hidden storage.
    pub(super) vars: BTreeMap<String, Vec<Sign>>,
    /// The sign of each parameter of each class in `classes` (`variance::class_signs`).
    pub(super) dict_vars: Vec<Vec<Sign>>,
}

/// An operation `o : a ->{} b` of a service.
pub(crate) struct Op {
    pub(crate) name: Name,
    /// The service's parameters, which `ty` is over.
    pub(crate) vars: Vec<Name>,
    /// `a ->{} b`, as the model types it.
    pub(crate) ty: Type,
    pub(crate) a: String,
    pub(crate) b: String,
}

/// The number of parameters and the fields of class `p`: `$S` of each
/// superclass `S`, then each method, its own variables bound before the
/// class's parameters.
fn dict(
    decls: &Decls,
    p: &Path,
    classes: &[String],
    types: &Rc<Types>,
) -> Option<(usize, Vec<(String, String)>)> {
    let class = decls.classes.get(p)?;
    let params: Vec<String> = class.params.iter().map(ToString::to_string).collect();
    let env = |own: &[Name]| Env {
        vars: own
            .iter()
            .map(ToString::to_string)
            .chain(params.iter().cloned())
            .collect(),
        classes: classes.to_vec(),
        types: Some(types.clone()),
        ..Env::default()
    };
    let mut fields = Vec::new();
    for (i, sup) in class.supers.iter().enumerate() {
        let t = ty(sup, &env(&[])).ok()?;
        fields.push((
            format!("${}", class.super_slot(i)?),
            format!("⟨0, Facts.none, {t}⟩"),
        ));
    }
    for m in &class.methods {
        let (_, s) = decls.method(p, m)?;
        let own: Vec<Name> = s.vars.iter().map(|(v, _)| v.clone()).collect();
        let t = ty(&s.body, &env(&own)).ok()?;
        fields.push((m.to_string(), format!("⟨{}, Facts.none, {t}⟩", own.len())));
    }
    Some((params.len(), fields))
}

/// The operations of a service of `params` parameters, if each is `a ->{} b`
/// of types the core writes, over the parameters only: the variables of an
/// operation's scheme are the service's, `.tvar i` the `i`-th.
fn ops(schemes: &[(Name, Scheme)], params: usize, types: &Rc<Types>) -> Option<Vec<Op>> {
    schemes
        .iter()
        .map(|(name, s)| {
            let t = bare(&s.body);
            let Type::Fun(a, Stage::Runtime(r, e), b) = t else {
                return None;
            };
            let none = Env {
                vars: s.vars.iter().map(|(v, _)| v.to_string()).collect(),
                types: Some(types.clone()),
                ..Env::default()
            };
            let own = s.vars.iter().all(|(_, k)| *k != Kind::Captures);
            (s.vars.len() == params && own && r.is_none() && e.is_none()).then_some(())?;
            Some(Op {
                name: name.clone(),
                vars: s.vars.iter().map(|(v, _)| v.clone()).collect(),
                ty: t.clone(),
                a: ty(bare(a), &none).ok()?,
                b: ty(bare(b), &none).ok()?,
            })
        })
        .collect()
}

/// The globals of other modules the generator calls.
const EXTRA: [(&str, &str); 4] = [
    ("List.Core", "get"),
    ("Fiber", "spawn"),
    ("Resource", "acquire_release"),
    ("Scope", "add_finalizer"),
];

/// The operators the generator makes.
pub(crate) const PRIMS: [Prim; 4] = [
    Prim::Arith(Arith::Add, Num::Int),
    Prim::Arith(Arith::Sub, Num::Int),
    Prim::Concat,
    Prim::Lt,
];

pub(crate) fn prim_key(p: Prim) -> Key {
    Key::Prim(format!("{p:?}"))
}

/// `t` without the qualifier of its value: the core's data has none.
pub(crate) fn bare(t: &Type) -> &Type {
    match t {
        Type::Captured(x, _) | Type::Bounded(x, _) => bare(x),
        t => t,
    }
}

/// The variables of a constructor's scheme the core keeps: not the capture
/// rows the model qualifies its fields and value with.
pub(crate) fn kept(s: &Scheme) -> Vec<bool> {
    s.vars.iter().map(|(_, k)| *k != Kind::Captures).collect()
}

/// The fields and type of a constructor's scheme, if it builds `T ᾱ`.
fn ctor(s: &Scheme, types: &Rc<Types>) -> Option<Ctor> {
    let keep = kept(s);
    let vars: Vec<&(Name, Kind)> = s
        .vars
        .iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(v, _)| v)
        .collect();
    let mut t = bare(&s.body);
    let mut ps = Vec::new();
    while let Type::Fun(p, _, r) = t {
        ps.push(bare(p));
        t = bare(r);
    }
    let mut args = Vec::new();
    while let Type::App(h, a) = t {
        args.push(&**a);
        t = h;
    }
    args.reverse();
    let Type::Con(adt) = t else { return None };
    // data with hidden storage takes first the capture row its value hides,
    // which qualifies each field that hides captures
    let stored = types.stored.contains(adt);
    let hidden = Caps::var(Var::Named("$c".into()));
    let env = Env {
        vars: (stored.then(|| "$c".to_owned()).into_iter())
            .chain(vars.iter().map(|(v, _)| v.to_string()))
            .collect(),
        types: Some(types.clone()),
        ..Env::default()
    };
    let fields = ps
        .iter()
        .map(|p| {
            if stored && hides(p) {
                ty(&Type::Captured(Rc::new((*p).clone()), hidden.clone()), &env).ok()
            } else {
                ty(p, &env).ok()
            }
        })
        .collect::<Option<Vec<_>>>()?;
    let plain = args.len() == vars.len()
        && args
            .iter()
            .zip(&vars)
            .all(|(a, (v, _))| matches!(a, Type::Var(Var::Named(w)) if w == v));
    plain.then(|| Ctor {
        k: vars.len() + usize::from(stored),
        fields,
        adt: adt.to_string(),
    })
}

/// The data a constructor of which has a field that hides captures.
fn stored(decls: &Decls) -> HashSet<Path> {
    let mut out = HashSet::new();
    for s in decls.constructors.values() {
        let mut t = bare(&s.body);
        let mut hidden = false;
        while let Type::Fun(p, _, r) = t {
            hidden |= hides(p);
            t = bare(r);
        }
        while let Type::App(h, _) = t {
            t = h;
        }
        if let (true, Type::Con(adt)) = (hidden, t) {
            out.insert(adt.clone());
        }
    }
    out
}

/// The sign of each parameter of the types of `ctors`, at its index in the
/// core: after the hidden row of data with hidden storage.
fn signs(
    decls: &Decls,
    ctors: &BTreeMap<String, Ctor>,
    types: &Types,
) -> BTreeMap<String, Vec<Sign>> {
    (ctors.values())
        .map(|c| {
            let signs = match &*c.adt {
                "Tuple" => vec![Sign::Pos, Sign::Pos],
                adt => {
                    let p = Path::parse(adt);
                    let hidden = types.stored.contains(&p).then_some(Sign::Inv);
                    (hidden.into_iter())
                        .chain(decls.signs.get(&p).into_iter().flatten().copied())
                        .collect()
                }
            };
            (c.adt.clone(), signs)
        })
        .collect()
}

impl Sig {
    pub fn of(decls: &Decls) -> Sig {
        let module = fixture_module();
        let types = Rc::new(Types {
            params: decls.params.clone(),
            stored: stored(decls),
        });
        let mut ctors = BTreeMap::new();
        for (p, s) in &decls.constructors {
            let ours = &*p.module == module
                || (&*p.module == "Prelude" && ["Some", "None"].contains(&&*p.name));
            if let (true, Some(c)) = (ours, ctor(s, &types)) {
                ctors.insert(p.to_string(), c);
            }
        }
        ctors.insert(
            "Tuple".into(),
            Ctor {
                k: 2,
                fields: vec!["(.tvar 0)".into(), "(.tvar 1)".into()],
                adt: "Tuple".into(),
            },
        );
        // the core has no list: a literal is the spine of these, which only
        // the signature has
        let list = "(.tapp (.adt \"Prelude.List\") (.tvar 0))";
        ctors.insert(
            "Prelude.Nil".into(),
            Ctor {
                k: 1,
                fields: Vec::new(),
                adt: "Prelude.List".into(),
            },
        );
        ctors.insert(
            "Prelude.Cons".into(),
            Ctor {
                k: 1,
                fields: vec!["(.tvar 0)".into(), list.into()],
                adt: "Prelude.List".into(),
            },
        );
        let mut globs: Vec<(Key, Scheme)> = PRIMS
            .iter()
            .map(|p| (prim_key(*p), prim_scheme(*p)))
            .collect();
        let mut values: Vec<_> = decls
            .values
            .iter()
            .filter(|(p, _)| &*p.module == module)
            .collect();
        values.sort_by(|a, b| a.0.cmp(b.0));
        for (p, s) in values {
            globs.push((Key::Value(p.clone()), elided(decls, s)));
        }
        // after the Fixture values, so that their indices stay
        for (module, name) in EXTRA {
            let p = Path::new(module, name);
            if let Some(s) = decls.values.get(&p) {
                globs.push((Key::Value(p), elided(decls, s)));
            }
        }
        let mut paths: Vec<&Path> = (decls.classes.keys())
            .filter(|p| &*p.module == module)
            .collect();
        paths.sort();
        let classes: Vec<String> = paths.iter().map(ToString::to_string).collect();
        let dicts = paths
            .iter()
            .map(|p| dict(decls, p, &classes, &types).unwrap_or_default())
            .collect();
        let dict_vars = paths
            .iter()
            .map(|p| decls.class_signs.get(*p).cloned().unwrap_or_default())
            .collect();
        // the instances, after the values the generator calls
        let mut instances: Vec<_> = (decls.instances.iter())
            .filter(|(p, _)| &*p.module == module)
            .collect();
        instances.sort_by(|a, b| a.0.cmp(b.0));
        for (p, s) in instances {
            globs.push((Key::Value(p.clone()), elided(decls, s)));
        }
        let params = |p: &Path| decls.service_params.get(p).copied().unwrap_or(0);
        let ops: BTreeMap<String, Vec<Op>> = (decls.services.iter())
            .filter(|(p, _)| &*p.module == module)
            .filter_map(|(p, schemes)| Some((p.to_string(), ops(schemes, params(p), &types)?)))
            .collect();
        let arity = (decls.services.keys())
            .filter(|p| ops.contains_key(&p.to_string()))
            .map(|p| (p.to_string(), params(p)))
            .collect();
        let vars = signs(decls, &ctors, &types);
        Sig {
            ctors,
            globs,
            ops,
            arity,
            classes,
            dicts,
            types,
            vars,
            dict_vars,
        }
    }

    pub(crate) fn glob(&self, key: &Key) -> Option<(usize, &Scheme)> {
        self.globs
            .iter()
            .position(|(k, _)| k == key)
            .map(|i| (i, &self.globs[i].1))
    }
}
