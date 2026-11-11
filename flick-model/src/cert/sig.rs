//! `formal/FormalCert/GenSig.lean`: the signature the programs of the
//! generator take from `Gen.fl` and the Prelude. `GenSig S` says that `S`
//! has its constructors and globals; a certificate assumes it of `S`, and
//! `GenSig.inhabited` gives a signature that has them, so the assumption is
//! no contradiction. A global is at its elided scheme, as the check
//! instantiates it (`check::elided`).

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use std::rc::Rc;

use flick_core::numeric::{Arith, Num};

use crate::check::{elided, prim_scheme};
use crate::decls::Decls;
use crate::generate::fixture_module;
use crate::syntax::{Name, Path, Prim};
use crate::translate::lean::elide::{Env, scheme_facts, ty};
use crate::types::{Kind, Scheme, Stage, Type, Var};

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
    /// The operations of each service without parameters, all `tail`.
    pub(crate) ops: BTreeMap<String, Vec<Op>>,
    /// The classes of the generator, by the index of their dictionaries.
    pub(crate) classes: Vec<String>,
    /// Each class's number of parameters and fields, `$S` of a superclass
    /// `S` first, as schemes in the core.
    dicts: Vec<(usize, Vec<(String, String)>)>,
    /// The kinds of the parameters of each type constructor.
    pub(crate) params: Rc<HashMap<Path, Vec<Kind>>>,
}

/// An operation `o : a ->{} b` of a service.
pub(crate) struct Op {
    pub(crate) name: Name,
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
    kinds: &Rc<HashMap<Path, Vec<Kind>>>,
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
        params: Some(kinds.clone()),
        ..Env::default()
    };
    let mut fields = Vec::new();
    for sup in &class.supers {
        let Type::Dict(c, _) = sup else { return None };
        let t = ty(sup, &env(&[])).ok()?;
        fields.push((format!("${}", c.name), format!("⟨0, Facts.none, {t}⟩")));
    }
    for m in &class.methods {
        let (_, s) = decls.method(p, m)?;
        let own: Vec<Name> = s.vars.iter().map(|(v, _)| v.clone()).collect();
        let t = ty(&s.body, &env(&own)).ok()?;
        fields.push((m.to_string(), format!("⟨{}, Facts.none, {t}⟩", own.len())));
    }
    Some((params.len(), fields))
}

/// The operations of a service, if each is `a ->{} b` of types the core
/// writes.
fn ops(schemes: &[(Name, Scheme)], kinds: &Rc<HashMap<Path, Vec<Kind>>>) -> Option<Vec<Op>> {
    schemes
        .iter()
        .map(|(name, s)| {
            let t = bare(&s.body);
            let Type::Fun(a, Stage::Runtime(r, e), b) = t else {
                return None;
            };
            let none = Env {
                params: Some(kinds.clone()),
                ..Env::default()
            };
            (s.vars.is_empty() && r.is_none() && e.is_none()).then_some(())?;
            Some(Op {
                name: name.clone(),
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
fn ctor(s: &Scheme, kinds: &Rc<HashMap<Path, Vec<Kind>>>) -> Option<Ctor> {
    let keep = kept(s);
    let vars: Vec<&(Name, Kind)> = s
        .vars
        .iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(v, _)| v)
        .collect();
    let env = Env {
        vars: vars.iter().map(|(v, _)| v.to_string()).collect(),
        params: Some(kinds.clone()),
        ..Env::default()
    };
    let mut fields = Vec::new();
    let mut t = bare(&s.body);
    while let Type::Fun(p, _, r) = t {
        fields.push(ty(bare(p), &env).ok()?);
        t = bare(r);
    }
    let mut args = Vec::new();
    while let Type::App(h, a) = t {
        args.push(&**a);
        t = h;
    }
    args.reverse();
    let Type::Con(adt) = t else { return None };
    let plain = args.len() == vars.len()
        && args
            .iter()
            .zip(&vars)
            .all(|(a, (v, _))| matches!(a, Type::Var(Var::Named(w)) if w == v));
    plain.then(|| Ctor {
        k: vars.len(),
        fields,
        adt: adt.to_string(),
    })
}

impl Sig {
    pub fn of(decls: &Decls) -> Sig {
        let module = fixture_module();
        let params = Rc::new(decls.params.clone());
        let mut ctors = BTreeMap::new();
        for (p, s) in &decls.constructors {
            let ours = &*p.module == module
                || (&*p.module == "Prelude" && ["Some", "None"].contains(&&*p.name));
            if let (true, Some(c)) = (ours, ctor(s, &params)) {
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
            .map(|p| dict(decls, p, &classes, &params).unwrap_or_default())
            .collect();
        // the instances, after the values the generator calls
        let mut instances: Vec<_> = (decls.instances.iter())
            .filter(|(p, _)| &*p.module == module)
            .collect();
        instances.sort_by(|a, b| a.0.cmp(b.0));
        for (p, s) in instances {
            globs.push((Key::Value(p.clone()), elided(decls, s)));
        }
        let ops = (decls.services.iter())
            .filter(|(p, _)| &*p.module == module)
            .filter_map(|(p, schemes)| Some((p.to_string(), ops(schemes, &params)?)))
            .collect();
        Sig {
            ctors,
            globs,
            ops,
            classes,
            dicts,
            params,
        }
    }

    pub(crate) fn glob(&self, key: &Key) -> Option<(usize, &Scheme)> {
        self.globs
            .iter()
            .position(|(k, _)| k == key)
            .map(|i| (i, &self.globs[i].1))
    }

    /// The text of `formal/FormalCert/GenSig.lean`; a global the core
    /// cannot write is left out, with its reason.
    pub fn lean(&self) -> String {
        let mut out = String::from(
            "-- Code generated by `just formal-certs`. DO NOT EDIT.\n\n\
             import FormalCert.Rules\n\n\
             /-!\n\
             # The signature of the generator's programs\n\n\
             The constructors and globals the programs of the model's generator take from `Gen.fl`\n\
             and the Prelude (`flick-model/src/cert/sig.rs`), a global at its elided scheme. A\n\
             certificate assumes `GenSig S`; `GenSig.inhabited` gives a signature that has them.\n\
             -/\n\n\
             namespace Flick.Cert\n\n\
             def genCtor : Name → Option (Nat × List Ty × Name)\n",
        );
        for (name, c) in &self.ctors {
            let _ = writeln!(
                out,
                "  | {name:?} => some ({}, [{}], {:?})",
                c.k,
                c.fields.join(", "),
                c.adt
            );
        }
        out.push_str("  | _ => none\n\n");
        out.push_str(
            "def genRes (K : Name) : List Ty :=\n  match genCtor K with\n  | some (k, _, _) => idTs k\n  | none => []\n\n",
        );
        out.push_str("/-- The constructors of each type, with their arity. -/\ndef genAdtCtors : Name → List (Name × Nat)\n");
        let mut adts: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for (name, c) in &self.ctors {
            adts.entry(&c.adt)
                .or_default()
                .push(format!("({name:?}, {})", c.fields.len()));
        }
        for (adt, cs) in &adts {
            let _ = writeln!(out, "  | {adt:?} => [{}]", cs.join(", "));
        }
        out.push_str("  | _ => []\n\n");
        out.push_str("def genGlob : Nat → Option Scheme\n");
        let mut skipped = String::new();
        for (i, (key, s)) in self.globs.iter().enumerate() {
            match scheme(s, &self.classes, &self.params) {
                Ok(text) => {
                    let _ = writeln!(out, "  -- {key:?}\n  | {i} => some {text}");
                }
                Err(why) => {
                    let _ = writeln!(skipped, "-- skip {key:?}: {why}");
                }
            }
        }
        out.push_str("  | _ => none\n\n");
        out.push_str("def genOps : Name → List Name\n");
        for (svc, ops) in &self.ops {
            let names: Vec<String> = ops
                .iter()
                .map(|o| format!("{:?}", o.name.to_string()))
                .collect();
            let _ = writeln!(out, "  | {svc:?} => [{}]", names.join(", "));
        }
        out.push_str("  | _ => []\n\ndef genOp : Name → Name → Option (OpClass × Ty × Ty)\n");
        for (svc, ops) in &self.ops {
            for o in ops {
                let _ = writeln!(
                    out,
                    "  | {svc:?}, {:?} => some (.tail, {}, {})",
                    o.name.to_string(),
                    o.a,
                    o.b
                );
            }
        }
        out.push_str("  | _, _ => none\n\n");
        out.push_str("def genDict : Nat → Option (Nat × List (Label × Scheme))\n");
        let mut most = 1;
        for (i, (k, fields)) in self.dicts.iter().enumerate() {
            if fields.is_empty() {
                continue;
            }
            most = most.max(fields.len());
            let fs: Vec<String> = fields
                .iter()
                .map(|(l, s)| format!("({l:?}, {s})"))
                .collect();
            let _ = writeln!(out, "  | {i} => some ({k}, [{}])", fs.join(", "));
        }
        out.push_str("  | _ => none\n\n");
        let each = vec!["⟨-, rfl⟩"; most].join(" | ");
        let _ = write!(
            out,
            "theorem genDict_closed : ∀ c k fs, genDict c = some (k, fs) → ∀ l s, (l, s) ∈ fs →\n    \
             s.Closed (s.k + k) := by\n  intro c k fs h l s hm\n  unfold genDict at h\n  \
             split at h <;> cases h <;>\n    \
             simp only [List.mem_cons, List.mem_nil_iff, or_false, Prod.mk.injEq] at hm <;>\n    \
             rcases hm with {each} <;> subst_vars <;> closed_tac\n\n"
        );
        out.push_str(WITNESS);
        if !skipped.is_empty() {
            out.push_str("\n/-! ## Skipped -/\n\n");
            out.push_str(&skipped);
        }
        out
    }
}

/// `⟨k, Facts.none, τ⟩` of a scheme without facts.
fn scheme(
    s: &Scheme,
    classes: &[String],
    kinds: &Rc<HashMap<Path, Vec<Kind>>>,
) -> Result<String, String> {
    let env = Env {
        vars: s.vars.iter().map(|(v, _)| v.to_string()).collect(),
        classes: classes.to_vec(),
        params: Some(kinds.clone()),
        ..Env::default()
    };
    let (cs, t) = scheme_facts(s, &env)?;
    Ok(format!("⟨{}, {cs}, {t}⟩", s.vars.len()))
}

/// The structure, and the signature that has it.
const WITNESS: &str = r"structure GenSig (S : Sig) : Prop where
  ctor : ∀ K, (genCtor K).isSome → S.ctor K = genCtor K
  res : ∀ K, (genCtor K).isSome → S.res K = genRes K
  glob : ∀ g, (genGlob g).isSome → S.glob g = genGlob g
  /-- A type of the generator has no constructors but its own. -/
  closed : ∀ K k ts T, S.ctor K = some (k, ts, T) → genAdtCtors T ≠ [] → genCtor K = some (k, ts, T)
  /-- A service of the generator has its operations and no other. -/
  op : ∀ E o, genOps E ≠ [] → S.op E o = genOp E o
  dict : ∀ c, (genDict c).isSome → S.dict c = genDict c

theorem genAdtCtors_complete : ∀ K k ts T, genCtor K = some (k, ts, T) →
    (K, ts.length) ∈ genAdtCtors T := by
  intro K k ts T h
  unfold genCtor at h
  split at h <;> cases h <;> decide

theorem genCtor_closed : ∀ K k ts T σ, genCtor K = some (k, ts, T) →
    (∀ i, i < k → σ (.v i) = .tvar i) → ts.map (Ty.substT σ) = ts := by
  intro K k ts T σ hK hσ
  unfold genCtor at hK
  split at hK <;> cases hK <;> simp [Ty.substT, Stage.substT, hσ]

theorem genCtor_reach : ∀ K k ts T, genCtor K = some (k, ts, T) →
    ∀ t ∈ ts, ∀ a, t.reach a → ∃ i, i < k ∧ a = .v i := by
  intro K k ts T hK t ht a ha
  unfold genCtor at hK
  split at hK <;> cases hK <;> simp at ht <;>
    (try rcases ht with rfl | rfl) <;> (try subst ht) <;>
    simp [Ty.reach, Caps.none] at ha <;>
    (try rcases ha with ha | ha) <;> (try subst ha) <;> refine ⟨_, ?_, rfl⟩ <;> decide

theorem genRes_closed : ∀ K k ts T σ, genCtor K = some (k, ts, T) →
    (∀ i, i < k → σ (.v i) = .tvar i) → (genRes K).map (Ty.substT σ) = genRes K := by
  intro K k ts T σ h hσ
  simp only [genRes, h, idTs, List.map_map]
  apply List.map_congr_left
  intro i hi
  simp only [Function.comp, Ty.substT]
  exact hσ i (List.mem_range.mp hi)

theorem genGlob_closed : ∀ g s, genGlob g = some s → s.Closed s.k := by
  intro g s h
  unfold genGlob at h
  split at h <;> cases h <;> closed_tac

theorem genOp_closed : ∀ E o c a b σ, genOp E o = some (c, a, b) →
    a.substT σ = a ∧ b.substT σ = b := by
  intro E o c a b σ h
  unfold genOp at h
  split at h <;> cases h <;> simp [Ty.substT]

theorem genOp_mem : ∀ E o x, genOp E o = some x → o ∈ genOps E := by
  intro E o x h
  unfold genOp at h
  split at h <;> cases h <;> simp [genOps]

/-- The arm of a service's one operation is all a handler of it needs. -/
theorem GenSig.complete {S : Sig} (hS : GenSig S) {E o : Name} (h : genOps E = [o]) :
    S.Complete E (recArm o) := by
  intro o' c a b hop
  rw [hS.op E o' (by simp [h])] at hop
  have hm := genOp_mem E o' _ hop
  rw [h, List.mem_singleton] at hm
  subst hm
  simp [Tm.armOf, Pat.spine, Pat.headName]

def genSig : Sig where
  ctor := genCtor
  impl _ := .errors Row.none
  ctor_closed := genCtor_closed
  ctor_reach := genCtor_reach
  res := genRes
  res_closed := genRes_closed
  impl_closed _ _ := by simp [Ty.substT, Row.none_substT]
  dict := genDict
  glob := genGlob
  dict_closed := genDict_closed
  glob_closed := genGlob_closed
  op := genOp
  op_closed := genOp_closed
  op_impl _ _ _ _ _ _ _ := rfl
  op_field _ _ _ _ _ h := (h rfl).elim

/-- The assumption of a certificate holds of a signature. -/
theorem GenSig.inhabited : ∃ S, GenSig S :=
  ⟨genSig, fun _ _ => rfl, fun _ _ => rfl, fun _ _ => rfl, fun _ _ _ _ h _ => h, fun _ _ _ => rfl,
    fun _ _ => rfl⟩

end Flick.Cert
";
