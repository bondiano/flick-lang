impl Checker<'_> {
    /// Records `⋃ ⌈held⌉ ⊆ ⋃ ⌈within⌉ ∪ allowed`, decided at the end.
    pub(super) fn hold(
        &mut self,
        held: &[Type],
        within: &[Type],
        allowed: &Caps,
        rule: &'static str,
    ) {
        self.regions.holds.push(Hold {
            held: held.to_vec(),
            within: within.to_vec(),
            allowed: allowed.clone(),
            rule,
            bounds: self.regions.bounds.clone(),
        });
    }

    /// A capture row inference has yet to solve.
    pub(super) fn fresh_caps(&mut self) -> Caps {
        Caps::var(Var::Meta(self.meta()))
    }

    /// The qualifier of `t`, `∅` when bare.
    pub(super) fn qualifier(&self, t: &Type) -> Caps {
        match self.follow(t) {
            Type::Captured(_, c) | Type::Bounded(_, c) => self.zonk_caps(&c),
            _ => Caps::default(),
        }
    }

    /// Every qualifier of `t`, outermost first: a value of `(τ^{≤C})^{≤D}`,
    /// as `Ref.set` takes into a cell of `τ^{≤C}`, is within each.
    pub(super) fn qualifiers(&self, t: &Type) -> Vec<Caps> {
        let mut out = Vec::new();
        let mut t = self.follow(t);
        while let Type::Captured(inner, c) | Type::Bounded(inner, c) = &t {
            out.push(self.zonk_caps(c));
            t = self.follow(inner);
        }
        out
    }

    /// `t` once for each of its qualifiers, about the bare type: what is
    /// within `(τ^{≤C})^{≤D}` is within `τ^{≤C}` and within `τ^{≤D}`, not
    /// within what the two reach together.
    pub(super) fn layers(&self, t: &Type) -> Vec<Type> {
        let mut out = Vec::new();
        let bare = Rc::new(self.resolve(t));
        let mut cur = self.follow(t);
        loop {
            cur = match &cur {
                Type::Captured(inner, c) => {
                    out.push(Type::Captured(bare.clone(), c.clone()));
                    self.follow(inner)
                }
                Type::Bounded(inner, c) => {
                    out.push(Type::Bounded(bare.clone(), c.clone()));
                    self.follow(inner)
                }
                _ => break,
            };
        }
        if out.len() < 2 {
            return vec![self.follow(t)];
        }
        out
    }

    /// The premise of (Lam) and (Task) against `expected`, for each of its
    /// qualifiers: the body's stage `inner` within `stage` with its `Live`.
    /// What the body holds is no bound: a qualifier is what its use needs.
    pub(super) fn close_within(
        &mut self,
        ty: &Type,
        inner: &Stage,
        stage: &Stage,
        expected: Option<&Type>,
    ) -> Check<Type> {
        let mut each = expected.map_or_else(Vec::new, |t| self.qualifiers(t));
        if each.is_empty() {
            each.push(Caps::default());
        }
        for q in &each {
            self.stage_within(inner, &live_stage(stage, q))?;
        }
        Ok(ty.clone())
    }

    /// (Proj): a part of a value of `(T ῡ)^C` taken apart has `C` too.
    pub(super) fn project_part(&self, whole: &Type, part: Type) -> Type {
        match self.follow(whole) {
            Type::Captured(t, c) | Type::Bounded(t, c)
                if !c.is_empty() && self.hidden(&t, &mut Vec::new()) =>
            {
                Type::Captured(Rc::new(part), c)
            }
            _ => part,
        }
    }

    /// (Con): the fields of `K ē`; captures a field hides go into the
    /// qualifier of the data, which a type without hidden storage needs not.
    pub(super) fn construct<'e>(
        &mut self,
        c: &Con,
        args: impl IntoIterator<Item = &'e Term>,
        at: &Term,
    ) -> Check<Type> {
        let mut t = self.constructor(&c.ty.module, &c.name)?;
        self.note_instance(at);
        let mut fields = Vec::new();
        for a in args {
            let Type::Fun(p, _, r) = self.resolve(&t) else {
                return error("Con", format!("`{}` takes fewer fields", c.name));
            };
            let field = if self.hidden(&p, &mut Vec::new()) {
                let q = self.fresh_caps();
                Type::Captured(p, q)
            } else {
                (*p).clone()
            };
            self.check(a, &field)?;
            fields.push(field);
            t = (*r).clone();
        }
        if !self.hidden(&t, &mut Vec::new()) {
            return Ok(t);
        }
        let q = self.fresh_caps();
        self.hold(&fields, &[t.clone()], &q, "Con");
        Ok(Type::Captured(Rc::new(t), q))
    }

    /// (Inst): each `αᵢ^D` of the scheme bounds what `αᵢ` becomes, and `Θ`
    /// is needed at the instance.
    pub(super) fn instance_premises(&mut self, s: &Scheme, map: &HashMap<Name, super::Inst>) {
        let mut bounds = Vec::new();
        bounded_vars(&s.body, &s.vars, &mut bounds);
        for (a, d) in bounds {
            let t = super::substitute(&Type::Var(Var::Named(a)), map);
            let d = super::generalize::substitute_caps(&d, map);
            self.hold(&[t], &[], &d, "Inst");
        }
        for (c, r) in &s.outlives {
            let c = super::generalize::substitute_caps(c, map);
            let r = super::substitute(r, map);
            let facts = self.regions.facts.clone();
            self.regions.needs.push((c, r, facts));
        }
    }

    /// A signed body assumes its scheme's bound facts, just as it assumes
    /// Outlives and Provided. Uses of the scheme still prove (Inst).
    pub(super) fn give_bounds(&mut self, s: &Scheme, map: &HashMap<Name, super::Inst>) {
        let mut bounds = Vec::new();
        bounded_vars(&s.body, &s.vars, &mut bounds);
        for (name, bound) in bounds {
            let t = super::substitute(&Type::Var(Var::Named(name)), map);
            if let Type::Var(variable) = t {
                let bound = super::generalize::substitute_caps(&bound, map);
                self.regions.bounds.push((variable, bound));
            }
        }
    }

    /// Variables of pending region obligations remain monomorphic (Gen,
    /// core.md §2.5 item 4). A later argument/call can still fix their owner.
    pub(super) fn open_region_metas(&self) -> BTreeSet<u32> {
        let mut metas = BTreeSet::new();
        for hold in &self.regions.holds {
            for held in &hold.held {
                self.metas(held, &mut metas);
            }
        }
        for (captures, owner, _) in &self.regions.needs {
            if !self.zonk_caps(captures).is_empty() {
                self.metas(owner, &mut metas);
                self.metas(
                    &Type::Captured(Rc::new(Type::con("Prelude", "Unit")), captures.clone()),
                    &mut metas,
                );
            }
        }
        for need in &self.regions.pending_provided {
            self.metas(&Type::Errors(need.row.clone()), &mut metas);
            self.metas(&Type::Errors(need.within.clone()), &mut metas);
        }
        metas
    }
}
