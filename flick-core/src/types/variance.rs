//! The variance of each parameter of a declared type (`docs/formal/regions.md`
//! §2.2.1, Lean `Sig.var` with `Sig.var_ok`): the least fixed point over the
//! declarations, read from the fields of the constructors, as the model reads
//! it (`flick-model/src/variance.rs`). Subtyping compares the arguments of
//! data by it, and records and tuples field by field.

use std::collections::HashMap;
use std::rc::Rc;

use super::Checker;
use super::elem::{Atom, Elem};
use super::env::Form;
use super::kinds::Kind;
use super::lang::Lang;
use super::regions::{Hold, Visible};
use super::ty::Type;
use super::unify::Unified;
use crate::iface::{Entity, Namespace};
use crate::source::Span;

/// `+`, `−`, `inv` or `bi` (unused).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sign {
    Bi,
    Pos,
    Neg,
    Inv,
}

impl Sign {
    fn join(self, other: Sign) -> Sign {
        match (self, other) {
            (Sign::Bi, s) | (s, Sign::Bi) => s,
            (a, b) if a == b => a,
            _ => Sign::Inv,
        }
    }
}

/// A type by its module and name.
type Key = (String, String);

/// The signs found so far, by type; `None` for a type without them.
pub(super) type Signs = HashMap<Key, Option<Rc<[Sign]>>>;

/// What the positions of a parameter in the fields allow, joined. A `+`
/// parameter stands under no flip (Lean `Ty.Pol.arrow`): there its argument
/// would grow what the field reaches where it must shrink.
struct Uses(Sign);

impl Uses {
    fn at(&mut self, q: bool, flipped: bool) {
        self.0 = self.0.join(match (q, flipped) {
            (true, false) => Sign::Pos,
            (true, true) => Sign::Inv,
            (false, _) => Sign::Neg,
        });
    }

    /// Where only `≡` relates its argument, if it stands there.
    fn fixed(&mut self, stands: bool) {
        if stands {
            self.0 = Sign::Inv;
        }
    }
}

fn names(l: &Elem, a: &str) -> bool {
    match l {
        Elem::Capture(Atom::Var(n)) | Elem::Error(Atom::Var(n)) | Elem::Service(Atom::Var(n)) => {
            n == a
        }
        Elem::Provided(e) => names(e, a),
        _ => false,
    }
}

/// Whether `a` stands anywhere in `t`, rows and qualifiers included.
fn occurs(a: &str, t: &Type) -> bool {
    t.contains(&mut |t| match t {
        Type::Var(n) => n == a,
        Type::Record(r) | Type::Row(r) => r.entries.iter().any(|(l, _)| names(l, a)),
        _ => false,
    })
}

/// `t` without the qualifiers of a value of it.
fn bare(mut t: &Type) -> &Type {
    while let Type::Captured { ty, .. } | Type::Bounded { ty, .. } = t {
        t = ty;
    }
    t
}

/// The fields of a constructor's scheme, and the type it builds.
fn fields(scheme: &Type) -> (Vec<&Type>, &Type) {
    let mut t = match scheme {
        Type::Forall { body, .. } => &**body,
        t => t,
    };
    let mut out = Vec::new();
    while let Type::Fun { param, result, .. } = t {
        out.push(&**param);
        t = result;
    }
    (out, bare(t))
}

/// The kinds of the parameters a kind takes.
fn params(mut k: &Kind) -> Vec<&Kind> {
    let mut out = Vec::new();
    while let Kind::Arrow(p, r) = k {
        out.push(&**p);
        k = r;
    }
    out
}

/// A declared type: the kinds of its parameters and its constructors'
/// schemes, `None` for one of them it cannot see.
struct Data<'t> {
    kinds: Vec<Kind>,
    ctors: Option<Vec<&'t Type>>,
}

impl Checker<'_> {
    /// The sign of each parameter of the type `module.name`, `None` but for
    /// a declared type or the built-in list.
    pub(super) fn signs(&mut self, module: &str, name: &str) -> Option<Rc<[Sign]>> {
        let key = (module.to_owned(), name.to_owned());
        if let Some(found) = self.signs.get(&key) {
            return found.clone();
        }
        // the types the fields reach, to solve together
        let mut group: Vec<Key> = Vec::new();
        let mut todo = vec![key.clone()];
        while let Some(k) = todo.pop() {
            if group.contains(&k) || self.signs.contains_key(&k) {
                continue;
            }
            let Some(data) = self.data(&k) else { continue };
            for scheme in data.ctors.iter().flatten() {
                scheme.visit(&mut |t| {
                    if let Type::Con { module, name } = t {
                        todo.push((module.clone(), name.clone()));
                    }
                });
            }
            group.push(k);
        }
        let mut current: HashMap<Key, Vec<Sign>> = HashMap::new();
        for k in &group {
            let n = self.data(k).map_or(0, |d| d.kinds.len());
            current.insert(k.clone(), vec![Sign::Bi; n]);
        }
        loop {
            let mut changed = false;
            for k in &group {
                let next = self.sign_of(k, &current);
                let old = &current[k];
                let joined: Vec<Sign> = old.iter().zip(&next).map(|(a, b)| a.join(*b)).collect();
                if &joined != old {
                    current.insert(k.clone(), joined);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for (k, s) in current {
            self.signs.insert(k, Some(s.into()));
        }
        let found = self.signs.get(&key).cloned().flatten().or_else(|| {
            let list = Type::Con {
                module: module.to_owned(),
                name: name.to_owned(),
            };
            // the built-in list is immutable and has no declaration
            list.is_lang(Lang::List).then(|| Rc::from([Sign::Pos]))
        });
        self.signs.insert(key, found.clone());
        found
    }

    fn data(&self, (module, name): &Key) -> Option<Data<'_>> {
        let def = self.env.type_def(module, name)?;
        let names: Vec<&str> = match &def.form {
            Form::Adt { constructors, .. } => constructors.iter().map(String::as_str).collect(),
            Form::Error => vec![name.as_str()],
            _ => return None,
        };
        let ctors = names
            .into_iter()
            .map(|c| {
                self.env.value(&Entity {
                    module: module.clone(),
                    namespace: Namespace::Constructor,
                    name: c.to_owned(),
                })
            })
            .collect();
        Some(Data {
            kinds: params(&def.kind).into_iter().cloned().collect(),
            ctors,
        })
    }

    /// The signs the constructors of `k` justify, given the current signs.
    fn sign_of(&self, k: &Key, current: &HashMap<Key, Vec<Sign>>) -> Vec<Sign> {
        let Some(data) = self.data(k) else {
            return Vec::new();
        };
        let n = data.kinds.len();
        let Some(ctors) = data.ctors else {
            return vec![Sign::Inv; n];
        };
        let mut uses: Vec<Uses> = (0..n).map(|_| Uses(Sign::Bi)).collect();
        let mut fixed = vec![false; n];
        for scheme in ctors {
            let (fields, res) = fields(scheme);
            let (_, res) = res.spine();
            for (j, u) in uses.iter_mut().enumerate() {
                let Some(r) = res.get(j) else {
                    u.0 = Sign::Inv;
                    continue;
                };
                let alone =
                    |v: &str| !(res.iter().enumerate()).any(|(m, o)| m != j && occurs(v, o));
                match bare(r) {
                    // an ordinary variable of the constructor, which nothing
                    // else it builds fixes (A1)
                    Type::Var(v) if alone(v) => {
                        for f in &fields {
                            self.walk(current, v, f, true, false, u);
                        }
                    }
                    // a GADT constructor that fixes the argument to a type
                    // subtyping does not move (A2)
                    c if self.closed(c, &mut Vec::new()) => fixed[j] = true,
                    _ => u.0 = Sign::Inv,
                }
            }
        }
        (data.kinds.iter().zip(uses.iter().zip(&fixed)))
            .map(|(k, (u, f))| match (k, u.0) {
                (Kind::Type, Sign::Bi) if *f => Sign::Pos,
                (Kind::Type, s) => s,
                _ => Sign::Inv,
            })
            .collect()
    }

    /// Where `a` stands in `t`, at polarity `q` (`true`: covariant),
    /// `flipped` once a flip is above it.
    fn walk(
        &self,
        current: &HashMap<Key, Vec<Sign>>,
        a: &str,
        t: &Type,
        q: bool,
        flipped: bool,
        out: &mut Uses,
    ) {
        match t {
            Type::Var(n) if n == a => out.at(q, flipped),
            Type::Fun {
                param,
                effects,
                result,
            } => {
                self.walk(current, a, param, !q, true, out);
                out.fixed(
                    effects
                        .as_ref()
                        .is_some_and(|e| occurs(a, &e.services) || occurs(a, &e.errors)),
                );
                self.walk(current, a, result, q, flipped, out);
            }
            Type::Captured { ty, captures, .. } | Type::Bounded { ty, captures } => {
                if occurs(a, captures) {
                    out.at(q, flipped);
                }
                self.walk(current, a, ty, q, flipped, out);
            }
            Type::Record(r) if !r.tail.as_ref().is_some_and(|t| occurs(a, t)) => {
                for (l, f) in &r.entries {
                    out.fixed(names(l, a));
                    self.walk(current, a, f, q, flipped, out);
                }
            }
            Type::Tuple(items) => {
                for x in items {
                    self.walk(current, a, x, q, flipped, out);
                }
            }
            Type::App { .. } => {
                let (head, args) = t.spine();
                match (head, &args[..]) {
                    (h, [r, e, x]) if h.is_lang(Lang::Eff) => {
                        out.fixed(occurs(a, r) || occurs(a, e));
                        self.walk(current, a, x, q, flipped, out);
                    }
                    (Type::Con { module, name }, _) => {
                        let key = (module.clone(), name.clone());
                        let theirs = (current.get(&key).cloned())
                            .or_else(|| self.signs.get(&key).cloned().flatten().map(|s| s.to_vec()))
                            .or_else(|| head.is_lang(Lang::List).then(|| vec![Sign::Pos]));
                        for (j, x) in args.into_iter().enumerate() {
                            // an unused parameter holds `a` only where `a`
                            // itself may stand (Lean `Ty.Pol.data`)
                            match theirs.as_ref().and_then(|s| s.get(j)).copied() {
                                Some(Sign::Pos | Sign::Bi) => {
                                    self.walk(current, a, x, q, flipped, out);
                                }
                                Some(Sign::Neg) => self.walk(current, a, x, !q, true, out),
                                Some(Sign::Inv) | None => out.fixed(occurs(a, x)),
                            }
                        }
                    }
                    _ => out.fixed(occurs(a, t)),
                }
            }
            _ => out.fixed(occurs(a, t)),
        }
    }

    /// A type subtyping moves neither up nor down (Lean `Ty.Closed`): a
    /// nominal type without parameters, representation or hidden storage
    /// whose fields are such types (a cycle of them is).
    fn closed(&self, t: &Type, seen: &mut Vec<Key>) -> bool {
        let Type::Con { module, name } = t else {
            return false;
        };
        let Some(def) = self.env.type_def(module, name) else {
            return false;
        };
        if !params(&def.kind).is_empty() {
            return false;
        }
        let key = (module.clone(), name.clone());
        match &def.form {
            Form::Nominal { hidden, .. } => !hidden,
            Form::Adt { .. } | Form::Error if seen.contains(&key) => true,
            Form::Adt { .. } | Form::Error => {
                let Some(Data {
                    ctors: Some(ctors), ..
                }) = self.data(&key)
                else {
                    return false;
                };
                seen.push(key);
                let all =
                    (ctors.iter()).all(|s| fields(s).0.iter().all(|f| self.closed(bare(f), seen)));
                seen.pop();
                all
            }
            _ => false,
        }
    }

    /// `⌈x⌉ ⊆ ⌈y⌉` of a `−` argument, which narrows by its type but keeps
    /// what it reaches.
    fn keeps_reach(&mut self, x: &Type, y: &Type, span: Span) {
        self.hold(Hold {
            held: vec![(None, x.clone())],
            visible: Visible::Late(y.clone()),
            bound: Type::empty_row(),
            expected: Some(y.clone()),
            written: true,
            what: "value",
            span,
        });
    }

    /// `T ū ≤ T ū'` by the sign of each parameter (≤-Data), and a record or
    /// a tuple field by field; else `≡`.
    pub(super) fn subsume_data(&mut self, a: &Type, e: &Type, span: Span) -> Unified {
        match (a, e) {
            (Type::Tuple(xs), Type::Tuple(ys)) if xs.len() == ys.len() => xs
                .iter()
                .zip(ys)
                .try_for_each(|(x, y)| self.subsume(x, y, span)),
            (Type::Record(r), Type::Record(s))
                if r.tail.is_none()
                    && s.tail.is_none()
                    && r.entries.len() == s.entries.len()
                    && r.entries.iter().all(|(l, _)| s.get(l).is_some()) =>
            {
                r.entries.iter().try_for_each(|(l, x)| {
                    s.get(l)
                        .map_or(Ok(()), |y| self.subsume(x, &y.clone(), span))
                })
            }
            (Type::App { .. }, Type::App { .. }) => {
                let ((h, xs), (g, ys)) = (a.spine(), e.spine());
                let signs = match (h, g) {
                    (Type::Con { module, name }, Type::Con { module: m, name: n })
                        if module == m && name == n && xs.len() == ys.len() =>
                    {
                        self.signs(module, name).filter(|s| s.len() == xs.len())
                    }
                    _ => None,
                };
                let Some(signs) = signs else {
                    return self.unify(a, e);
                };
                let (xs, ys): (Vec<Type>, Vec<Type>) = (
                    xs.into_iter().cloned().collect(),
                    ys.into_iter().cloned().collect(),
                );
                for ((x, y), s) in xs.iter().zip(&ys).zip(signs.iter()) {
                    // an argument inference has yet to solve takes the other
                    // as it is, its bound too, as `≡` of the whole type did
                    let open = |t: &Type| matches!(self.whnf(t), Type::Meta(_));
                    match s {
                        _ if open(x) || open(y) => self.unify(x, y)?,
                        Sign::Pos => self.subsume(x, y, span)?,
                        Sign::Neg => {
                            self.subsume(y, x, span)?;
                            self.keeps_reach(x, y, span);
                        }
                        Sign::Inv | Sign::Bi => self.unify(x, y)?,
                    }
                }
                Ok(())
            }
            _ => self.unify(a, e),
        }
    }
}
