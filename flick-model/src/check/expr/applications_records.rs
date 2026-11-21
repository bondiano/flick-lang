impl Checker<'_> {
    /// (Var).
    fn var(&mut self, x: &str) -> Check<Type> {
        match self.locals.iter().rev().find(|(n, _)| &**n == x) {
            Some((_, s)) => {
                let s = s.clone();
                Ok(self.instance_of(s))
            }
            None => error("Var", format!("`{x}` is not bound")),
        }
    }

    /// (Var) on the top-level values of §2.6 and Γ₀.
    pub(super) fn global(&mut self, p: &Path) -> Check<Type> {
        if let Some((own, s)) = &self.own
            && own == p
        {
            let s = s.clone();
            return Ok(self.instance_of(s));
        }
        self.declared_global(p)
    }

    /// Instantiates a declared value without treating it as a recursive
    /// source reference to the binding currently being checked. Primitive
    /// constants implement their declarations, even at that binding's path.
    pub(super) fn declared_global(&mut self, p: &Path) -> Check<Type> {
        let s = self.decls.value(p).or_else(|| self.decls.instances.get(p));
        match s {
            Some(s) => {
                let s = self.live_scheme(s);
                Ok(self.instance_of(s))
            }
            None => error("Var", format!("no type for `{p}`")),
        }
    }

    /// The instance just taken, on the node of `e`.
    pub(super) fn note_instance(&mut self, e: &Term) {
        if let Some(i) = self.instanced.take() {
            self.note(e, |n| n.instance = Some(i));
        }
    }

    /// An instance of `s`, which the trace keeps with the scheme.
    pub(super) fn instance_of(&mut self, s: Scheme) -> Type {
        let (t, insts) = self.instantiate_with(&s);
        if self.trace.is_some() {
            self.instanced = Some((s, insts));
        }
        t
    }

    pub(super) fn constructor(&mut self, module: &str, name: &str) -> Check<Type> {
        match self.decls.constructors.get(&Path::new(module, name)) {
            Some(s) => {
                let s = s.clone();
                Ok(self.instance_of(s))
            }
            None => error("Var", format!("no constructor `{module}.{name}`")),
        }
    }

    /// (Lam): without an expected type the arrow has the least stage, that
    /// of the body.
    fn lam(&mut self, x: &Name, body: &Term) -> Check<Type> {
        let p = self.fresh();
        let outer = mem::replace(&mut self.stage, Stage::Pure);
        self.locals.push((x.clone(), mono(p.clone())));
        let r = self.declaring(None, |c| c.infer(body));
        self.locals.pop();
        let inner = mem::replace(&mut self.stage, outer);
        Ok(Type::fun(p, inner, r?))
    }

    /// (App): `φ₁ ⊔ φ₂ ⊔ φ`. A callee of unknown type is a pure arrow (`docs/formal/core.md#a-callee-of-unknown-type-is-a-pure-arrow`).
    /// A constructor with all its fields is (Con).
    pub(in crate::check) fn app(&mut self, f: &Term, a: &Term) -> Check<Type> {
        if let (true, Term::Lam(x, body)) = (self.config, f) {
            return self.redex(x, body, a, None);
        }
        let mut args = vec![a];
        let mut head = f;
        while let Term::App(g, b) = head {
            args.push(b);
            head = g;
        }
        args.reverse();
        if let Term::Apply(Head::Con(c), given) = head
            && given.len() + args.len() == c.arity
        {
            // the fields themselves, not copies: the trace is by address
            return self.construct(c, given.iter().chain(args), head);
        }
        // the spine is one call
        self.call(|c| {
            let (t, rest) = match head {
                Term::Lam(x, body) if c.config => (c.redex(x, body, args[0], None)?, &args[1..]),
                _ => (c.infer(head)?, &args[..]),
            };
            rest.iter().try_fold(t, |t, a| c.apply(&t, a))
        })
    }

    pub(super) fn apply(&mut self, f: &Type, a: &Term) -> Check<Type> {
        match self.resolve(f) {
            Type::Fun(p, s, r) => {
                if matches!(self.resolve(&p), Type::Eff(..)) {
                    self.task_argument(|c| c.check(a, &p))?;
                } else {
                    self.check(a, &p)?;
                }
                let s_live = live_stage(&s, &self.qualifier(f));
                self.perform(&s_live)?;
                let r = (*r).clone();
                if s == Stage::Pure && self.is_callable(&r) {
                    return Ok(self.rewrap_bounds(f, r));
                }
                Ok(r)
            }
            Type::Forall(s) => {
                let inst = self.instantiate(&s);
                let t = self.rewrap_qualifiers(f, inst);
                self.apply(&t, a)
            }
            t @ Type::Var(Var::Meta(_)) => {
                let (p, r) = (self.fresh(), self.fresh());
                self.unify(&t, &Type::fun(p.clone(), Stage::Pure, r.clone()))?;
                self.check(a, &p)?;
                Ok(r)
            }
            other => error(
                "App",
                format!(
                    "this is not a function: it has type `{}`",
                    self.zonk(&other)
                ),
            ),
        }
    }

    /// (Rec): distinct labels, an exact record.
    fn record(&mut self, fields: &[(Name, Term)]) -> Check<Type> {
        let mut out = BTreeMap::new();
        for (l, x) in fields {
            let t = self.infer(x)?;
            if out.insert(l.clone(), t).is_some() {
                return error("Rec", format!("field `{l}` is given more than once"));
            }
        }
        Ok(Type::Record(Fields {
            fields: out,
            tail: None,
        }))
    }

    /// (Field): of a record, an implementation or a dictionary (§1.6).
    pub(super) fn field(&mut self, t: &Type, l: &Name) -> Check<Type> {
        match self.resolve(t) {
            Type::Record(f) => {
                let f = self.zonk_fields(&f);
                if let Some(part) = f.fields.get(l) {
                    return Ok(self.project_part(t, part.clone()));
                }
                match f.tail {
                    Some(Var::Meta(_)) => {
                        let t = self.fresh();
                        let more = Fields {
                            fields: BTreeMap::from([(l.clone(), t.clone())]),
                            tail: Some(Var::Meta(self.meta())),
                        };
                        self.unify(&Type::Record(f), &Type::Record(more))?;
                        Ok(t)
                    }
                    _ => error("Field", format!("`{}` has no field `{l}`", Type::Record(f))),
                }
            }
            m @ Type::Var(Var::Meta(_)) => {
                let t = self.fresh();
                let open = Fields {
                    fields: BTreeMap::from([(l.clone(), t.clone())]),
                    tail: Some(Var::Meta(self.meta())),
                };
                self.unify(&m, &Type::Record(open))?;
                Ok(t)
            }
            Type::Impl(r) => {
                let s = self.service(&r)?;
                let args = self.zonk_row(&r).elems[&s].clone();
                let op = self.decls.operation(&s, l).map(|op| at_args(op, &args));
                match op {
                    Some(op) => Ok(self.instantiate(&op)),
                    None => error("Field", format!("`{}` has no operation `{l}`", s.name)),
                }
            }
            Type::Dict(c, args) => self.method(&c, &args, l).map(|s| self.instantiate(&s)),
            Type::Module(sig) => {
                let part = self.contract_field(&sig, None, l)?;
                Ok(self.project_part(t, part))
            }
            Type::OpenedModule(sig, w) => {
                let part = self.contract_field(&sig, Some(w), l)?;
                Ok(self.project_part(t, part))
            }
            Type::OpenedExists(sig, w, body) => match &**l {
                "$module" => Ok(self.project_part(t, Type::OpenedModule(sig, w))),
                "$value" => Ok(self.project_part(t, (*body).clone())),
                _ => error(
                    "Field",
                    format!("`exists _ : {}. {body}` has no field `{l}`", sig.name),
                ),
            },
            Type::Exists(k, sig, body) => match &**l {
                "$module" => Ok(self.project_part(t, Type::Module(sig))),
                "$value" => {
                    let contract = self.contract(&sig)?;
                    if contract.abstract_types.is_empty() {
                        let opened = self.open_exists_body(&k, &sig, &body, &[])?;
                        Ok(self.project_part(t, opened))
                    } else {
                        error(
                            "Field",
                            format!("abstract type of `{}` requires `unpack`", sig.name),
                        )
                    }
                }
                _ => error(
                    "Field",
                    format!("`exists {k} : {}. {body}` has no field `{l}`", sig.name),
                ),
            },
            other => error(
                "Field",
                format!("`{}` has no field `{l}`", self.zonk(&other)),
            ),
        }
    }

    /// (Rec) against a closed record type: each field against its type, so
    /// a field's λ takes the stage the record asks for.
    pub(super) fn record_against(&mut self, fields: &[(Name, Term)], expected: &Fields) -> Check<()> {
        // equal counts and every expected label given: the labels are distinct
        let same = fields.len() == expected.fields.len()
            && expected
                .fields
                .keys()
                .all(|l| fields.iter().any(|(m, _)| m == l));
        if !same {
            let t = self.record(fields)?;
            return self.subsume(&t, &Type::Record(expected.clone()));
        }
        fields
            .iter()
            .try_for_each(|(l, x)| self.check(x, &expected.fields[l]))
    }
}
