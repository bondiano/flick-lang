impl Checker<'_> {
    /// `⌈τ⌉` (§2.1): the atoms a value of `t` may reach, an atom `α^{≤C}`
    /// as `C`.
    pub(super) fn reach(&self, t: &Type) -> Caps {
        let (mut out, bounded) = self.reach_bounded(t);
        for (_, c) in bounded {
            out = out.union(&c);
        }
        out
    }

    /// `⌈τ⌉` with each atom `α^{≤C}` apart (`docs/formal/regions.md#a-bounded-variable-reaches-what-the-variable-reaches`): a type variable under a
    /// bound reaches what `α` does, no more than `C`.
    pub(super) fn reach_bounded(&self, t: &Type) -> (Caps, Vec<(Var, Caps)>) {
        let mut out = Caps::default();
        let mut bounded = Vec::new();
        self.reach_into(&self.zonk(t), &mut out, &mut bounded);
        (out, bounded)
    }

    fn reach_into(&self, t: &Type, out: &mut Caps, bounded: &mut Vec<(Var, Caps)>) {
        let mut go = |t: &Type, out: &mut Caps| self.reach_into(t, out, bounded);
        match t {
            Type::Var(v) => {
                out.atoms.insert(v.clone());
            }
            Type::App(..) => {
                let mut args = Vec::new();
                let mut head = t;
                while let Type::App(h, a) = head {
                    args.push(&**a);
                    head = h;
                }
                let params = match head {
                    Type::Con(p) => self.decls.params.get(p),
                    _ => None,
                };
                go(head, out);
                // an argument of kind SRow reaches nothing (§2.1)
                for (i, a) in args.iter().rev().enumerate() {
                    if params.and_then(|k| k.get(i)) != Some(&Kind::Services) {
                        go(a, out);
                    }
                }
            }
            Type::Fun(a, s, b) => {
                go(a, out);
                go(b, out);
                if let Stage::Runtime(r, _) = s {
                    *out = out.union(&row_live(r));
                }
            }
            Type::Eff(r, _, a) => {
                go(a, out);
                *out = out.union(&row_live(r));
            }
            Type::Ref(a) => go(a, out),
            Type::Record(f) => f.fields.values().for_each(|t| go(t, out)),
            Type::Tuple(ts) => ts.iter().for_each(|t| go(t, out)),
            // the variables a `∀` binds are no captures: each use instantiates them
            Type::Forall(s) => {
                let mut inner = Caps::default();
                let mut under = Vec::new();
                self.reach_into(&s.body, &mut inner, &mut under);
                let bound =
                    |v: &Var| matches!(v, Var::Named(n) if s.vars.iter().any(|(b, _)| b == n));
                for (a, c) in under {
                    if bound(&a) {
                        inner = inner.union(&c);
                    } else {
                        bounded.push((a, c));
                    }
                }
                inner.atoms.retain(|v| !bound(v));
                inner.vars.retain(|v| !bound(v));
                *out = out.union(&inner);
            }
            Type::Captured(t, c) => {
                go(t, out);
                if self.hidden(t, &mut Vec::new()) {
                    *out = out.union(c);
                }
            }
            Type::Bounded(t, c) => match self.resolve(t) {
                Type::Var(v @ Var::Named(_)) if !self.is_region(&v) => bounded.push((v, c.clone())),
                _ if self.holds_nothing(t) => {}
                _ => {
                    let mut inner = Caps::default();
                    go(t, &mut inner);
                    inner.atoms.retain(|v| self.is_region(v));
                    inner.vars.clear();
                    *out = out.union(&inner).union(c);
                }
            },
            // `Errors {E ῡ} ≡ E ῡ` (`core.md` §1.5) reaches what its errors
            // do, and a variable of the row what the errors it stands for do
            // (`docs/formal/regions.md#errors-reaches-its-payloads`)
            Type::Errors(r) => {
                for args in r.elems.values() {
                    args.iter().for_each(|a| go(a, out));
                }
                out.atoms.extend(r.vars.iter().cloned());
            }
            // a row is no capture; `Layer`, implementations, tags and
            // dictionaries hold nothing (§2.1)
            Type::Con(_)
            | Type::EffOf(..)
            | Type::Layer(..)
            | Type::Impl(_)
            | Type::Tag(_)
            | Type::Dict(..)
            | Type::Env(_) => {}
        }
    }

    pub(super) fn is_region(&self, v: &Var) -> bool {
        self.regions.kinds.contains(v)
    }

    fn scalar(&self, t: &Type) -> bool {
        matches!(self.resolve(t), Type::Con(p) if p.module.as_ref() == "Prelude" && SCALARS.contains(&p.name.as_ref()))
    }

    /// A qualifier on a value that can hold nothing adds nothing (`docs/formal/regions.md#a-value-bound-survives-an-unknown-type-position`).
    fn holds_nothing(&self, t: &Type) -> bool {
        self.scalar(t)
            || (!matches!(self.resolve(t), Type::Var(_))
                && self.reach(t).is_empty()
                && !self.hidden(t, &mut Vec::new()))
    }

    /// Whether a value of `t` stores what its type does not show: a
    /// function, a computation, a cell, a type without constructors, or
    /// data with such a field (§1.2).
    pub(super) fn hidden(&self, t: &Type, seen: &mut Vec<Path>) -> bool {
        match self.resolve(t) {
            Type::Var(_) => false,
            Type::Tuple(ts) => ts.iter().any(|t| self.hidden(t, seen)),
            Type::Record(f) => f.fields.values().any(|t| self.hidden(t, seen)),
            Type::Forall(s) => self.hidden(&s.body, seen),
            t @ (Type::Con(_) | Type::App(..)) => {
                let mut args = Vec::new();
                let mut head = t;
                while let Type::App(h, a) = head {
                    args.push((*a).clone());
                    head = self.resolve(&h);
                }
                args.reverse();
                let Type::Con(p) = head else { return true };
                if p.module.as_ref() == "Prelude" && SCALARS.contains(&p.name.as_ref()) {
                    return false;
                }
                if p == Path::new("Prelude", "List") {
                    return args.iter().any(|t| self.hidden(t, seen));
                }
                if seen.contains(&p) {
                    return false;
                }
                let Some(fields) = self.fields_of(&p, &args) else {
                    return true;
                };
                seen.push(p);
                fields.iter().any(|t| self.hidden(t, seen))
            }
            _ => true,
        }
    }

    /// Data with hidden storage a signature writes without a qualifier
    /// (`regions.md` §1.4): a declared ADT whose values hide captures,
    /// through the data it holds, or a type without constructors its
    /// interface says hides them (an opaque ADT, a foreign `@stores` type).
    /// Another type without constructors gets no row: what a foreign
    /// binding can hide is G4's business, not elision's.
    pub(in crate::check) fn bare_hidden(&self, t: &Type) -> bool {
        let mut head = t;
        while let Type::App(h, _) = head {
            head = h;
        }
        let Type::Con(p) = head else { return false };
        self.decls.hidden.contains(p)
            || (self.decls.variants.contains_key(p)
                && !self.decls.errors.contains(p)
                && self.hidden(t, &mut Vec::new()))
    }

    /// The fields of every constructor of the declared type `p` at `args`;
    /// `None` for a type without constructors.
    fn fields_of(&self, p: &Path, args: &[Type]) -> Option<Vec<Type>> {
        let variants = self.decls.variants.get(p)?;
        if self.decls.errors.contains(p) {
            return None;
        }
        let mut out = Vec::new();
        for (name, _) in variants {
            let s = self.decls.constructors.get(&Path {
                module: p.module.clone(),
                name: name.clone(),
            })?;
            let mut params = Vec::new();
            let mut t = &s.body;
            while let Type::Fun(x, _, r) = t {
                params.push((**x).clone());
                t = r;
            }
            let mut result = Vec::new();
            while let Type::App(h, a) = t {
                result.push((**a).clone());
                t = h;
            }
            result.reverse();
            let map = result
                .iter()
                .zip(args)
                .filter_map(|(v, a)| match v {
                    Type::Var(Var::Named(n)) => Some((n.clone(), super::Inst::Type(a.clone()))),
                    _ => None,
                })
                .collect();
            out.extend(params.iter().map(|x| super::substitute(x, &map)));
        }
        Some(out)
    }
}
