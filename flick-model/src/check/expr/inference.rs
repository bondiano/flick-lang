impl Checker<'_> {
    /// `Γ ⊢ e : τ ! φ`, `φ` joined into `self.stage`.
    pub(super) fn infer(&mut self, e: &Term) -> Check<Type> {
        let t = self.infer_node(e)?;
        if self.trace.is_some() {
            let instance = self
                .instanced
                .take()
                .filter(|_| matches!(e, Term::Var(_) | Term::Global(_)));
            let inferred = t.clone();
            self.note(e, |n| {
                n.inferred = Some(inferred);
                n.instance = n.instance.take().or(instance);
            });
        }
        Ok(t)
    }

    fn infer_node(&mut self, e: &Term) -> Check<Type> {
        match e {
            Term::Var(x) => self.var(x),
            Term::Global(p) => self.global(p),
            Term::Lit(l) => Ok(lit(l)),
            Term::Lam(x, body) => self.lam(x, body),
            Term::App(f, a) => self.app(f, a),
            Term::Let(x, sig, e1, e2) => self.let_(e, x, sig.as_deref(), e1, e2, None),
            Term::LetRec(f, sig, lam, e2) => self.let_rec(f, sig.as_deref(), lam, e2, None),
            Term::Match(e, arms) => self.match_(e, arms, None),
            Term::Tuple(xs) => Ok(Type::Tuple(
                xs.iter().map(|x| self.infer(x)).collect::<Check<_>>()?,
            )),
            Term::List(xs) => {
                let t = self.fresh();
                for x in xs.iter() {
                    self.check(x, &t)?;
                }
                Ok(list(t))
            }
            Term::Record(fields) => self.record(fields),
            Term::Field(e, l) => {
                let t = self.infer(e)?;
                self.field(&t, l)
            }
            Term::Update(e, l, v) => {
                let t = self.infer(e)?;
                let f = self.field(&t, l)?;
                self.check(v, &f)?;
                Ok(t)
            }
            Term::Task(body) => self.task(body, None),
            Term::Await(e) => self.await_(e),
            Term::Try(body, arms) => self.try_(body, arms, None),
            Term::Provide(l, body) => self.provide(l, body, None),
            Term::Handler(h) => self.handler(h, None),
            Term::Apply(Head::Con(c), args) if args.len() == c.arity => {
                self.construct(c, args.iter(), e)
            }
            Term::Apply(Head::Const(Const::SeqIter), args) if args.len() == 1 => {
                self.seq_iter(&args[0])
            }
            Term::Apply(head, args) => {
                let t = match head {
                    Head::Con(c) => self.constructor(&c.ty.module, &c.name)?,
                    Head::Const(c) => self.constant(c)?,
                };
                self.note_instance(e);
                self.call(|c| args.iter().try_fold(t, |t, a| c.apply(&t, a)))
            }
            Term::ReadOwnerEvidence | Term::OwnerCandidate(_) => {
                Ok(Type::con("$internal", "OwnerEvidence"))
            }
            Term::DefaultOwner => {
                let t = self.global(&Path::new("Scope", "current"))?;
                self.apply(&t, &Term::unit())
            }
            Term::WithOwnerEvidence(owner, body) | Term::ActivateOwner(owner, body) => {
                self.owner_context(owner, matches!(e, Term::ActivateOwner(..)))?;
                self.infer(body)
            }
            Term::Scope(s, body) => self.scope(s, body, None),
            Term::Tag(s) => {
                let (vars, row) = self.service_row(s);
                Ok(self.instantiate(&Scheme::new(vars, Type::Tag(row))))
            }
            Term::Structural(class, fields) => {
                let t = self.fresh();
                let d = Type::Dict(Path::new("Prelude", class), Rc::from([t]));
                self.structural
                    .push((class.clone(), fields.clone(), d.clone()));
                Ok(d)
            }
            Term::Stuck(why) => error("Translation", why.to_string()),
            Term::Die(_) => Ok(self.fresh()),
            other => self.runtime_form(other, None),
        }
    }

    /// Compiler-only context operands cannot carry arbitrary source values.
    /// Activation dynamically validates an opaque candidate; literal absence
    /// is only admitted by the inert bracket.
    pub(in crate::check) fn owner_context(&mut self, owner: &Term, raw: bool) -> Check<()> {
        let t = self.infer(owner)?;
        let t = self.resolve(&t);
        let valid = matches!(&t, Type::App(head, _) if **head == Type::con("Fiber", "Owner"))
            || t == Type::con("$internal", "OwnerEvidence")
            || (!raw && t == prelude("Unit"));
        if valid {
            Ok(())
        } else {
            error("OwnerEvidence", "context needs owner evidence or absence")
        }
    }

    /// `Γ ⊢ e : τ'` with `τ ≤ τ'` (Sub): the expected type is pushed into
    /// λs, `let` bodies and arms, where it fixes stages and parameters.
    pub(super) fn check(&mut self, e: &Term, expected: &Type) -> Check<()> {
        if self.trace.is_some() {
            let expected = expected.clone();
            self.note(e, |n| n.expected = Some(expected));
        }
        let full = self.follow(expected);
        let expected = self.resolve(&full);
        match (e, &expected) {
            (Term::WithOwnerEvidence(owner, body) | Term::ActivateOwner(owner, body), _) => {
                self.owner_context(owner, matches!(e, Term::ActivateOwner(..)))?;
                self.check(body, &full)
            }
            (_, Type::Forall(s)) => {
                let qualifier = self
                    .hidden_qualifiers(&full)
                    .into_iter()
                    .fold(Caps::default(), |a, b| a.union(&b));
                self.check_qualified_forall(e, s, qualifier, None, false)
            }
            // (Lam) against `(τ₁ →φ τ₂)^C` takes `C`, against a bare arrow `∅`
            (Term::Lam(x, body), Type::Fun(p, s, r)) => {
                let outer = mem::replace(&mut self.stage, Stage::Pure);
                self.locals.push((x.clone(), mono((**p).clone())));
                let checked = self.declaring(Some(s), |c| c.check(body, r));
                self.locals.pop();
                let inner = mem::replace(&mut self.stage, outer);
                checked?;
                self.close_within(&expected, &inner, s, Some(&full))
                    .map(drop)
            }
            (Term::Scope(s, body), _) => self.scope(s, body, Some(&full)).map(drop),
            // (List) against a bare `List τ`: each element against `τ`, so
            // (Sub) may widen it, as `[text "x"] : List (Ui^c)`
            (Term::List(xs), Type::App(h, t))
                if matches!(full, Type::App(..)) && **h == prelude("List") =>
            {
                xs.iter().try_for_each(|x| self.check(x, t))
            }
            (Term::Apply(Head::Con(c), args), _) if args.len() == c.arity => {
                let t = self.construct(c, args.iter(), e)?;
                self.fit(e, &t, &full)
            }
            (Term::Let(x, sig, e1, e2), _) => self
                .let_(e, x, sig.as_deref(), e1, e2, Some(&full))
                .map(drop),
            (Term::LetRec(f, sig, lam, e2), _) => self
                .let_rec(f, sig.as_deref(), lam, e2, Some(&full))
                .map(drop),
            (Term::Match(e, arms), _) => self.match_(e, arms, Some(&full)).map(drop),
            (Term::Try(body, arms), _) => self.try_(body, arms, Some(&full)).map(drop),
            (Term::Provide(l, body), _) => self.provide(l, body, Some(&full)).map(drop),
            (Term::Handler(h), _) => self.handler(h, Some(&full)).map(drop),
            (Term::Task(body), Type::Eff(..)) => self.task(body, Some(&full)).map(drop),
            (Term::Record(fields), Type::Record(f)) if f.tail.is_none() => {
                self.record_against(fields, f)
            }
            (Term::Record(fields), Type::Impl(r)) => {
                let f = self.implementation(r)?;
                self.record_against(fields, &f)
            }
            (Term::Record(fields), Type::Dict(c, args)) => self.dictionary(fields, c, args),
            (Term::Record(fields), Type::Module(sig)) => self.check_pack_module(fields, sig),
            (Term::Record(fields), Type::Exists(k, sig, body)) => {
                self.check_pack_exists(fields, k, sig, body)
            }
            (Term::With(..) | Term::Handle(..) | Term::Inside(..), _) => {
                self.runtime_form(e, Some(&full)).map(drop)
            }
            (Term::App(f, a), _)
                if matches!(&**f, Term::Lam(x, body) if self.redex_of(x, body)) =>
            {
                let Term::Lam(x, body) = &**f else {
                    unreachable!("a λ, matched above")
                };
                self.redex(x, body, a, Some(&full)).map(drop)
            }
            // `bind_S (await v)` of a configuration: what the layer builds is
            // checked against `S.Impl`, as `Layer.effect` checked it; the
            // task may hold what it likes
            (Term::Await(x), _) if self.config => {
                let (r, x_rows) = (self.fresh_row(), self.fresh_row());
                let task = Type::Eff(r.clone(), x_rows.clone(), Rc::new(full));
                let held = self.fresh_caps();
                self.check(x, &Type::Captured(Rc::new(task), held))?;
                self.perform(&Stage::Runtime(r, x_rows))
            }
            _ => {
                let t = self.infer(e)?;
                self.fit(e, &t, &full)
            }
        }
    }

    /// (Let-Sig) with `σ` a `∀`: the value is checked with rigid variables,
    /// and a runtime step while it is built is `FLICK-T013` (`docs/formal/core.md#an-annotation-with-forall-bypasses-let-sig`).
    pub(super) fn check_forall(&mut self, e: &Term, s: &Scheme) -> Check<()> {
        self.check_qualified_forall(e, s, Caps::default(), None, true)
    }

    /// (Let-Sig) against `(∀ᾱ. τ)^C`: the value has `τ^C`. A signed `let
    /// rec` binds `f` in `e` at `own`, its regions those `e` is checked at.
    /// When `bind_tyvars` is true, the scheme's binders enter scope for local
    /// signatures inside `e`.
    pub(super) fn check_qualified_forall(
        &mut self,
        e: &Term,
        s: &Scheme,
        qualifier: Caps,
        rec: Option<(&Name, &Scheme)>,
        bind_tyvars: bool,
    ) -> Check<()> {
        let (facts, givens, tyvars) = (self.regions.facts.len(), self.givens(), self.tyvars.len());
        let (mut body, rigid) = self.skolemize(s, bind_tyvars);
        if let Some((f, own)) = rec {
            let own = own_regions(own, &s.vars, &rigid);
            self.locals.push((f.clone(), own));
        }
        self.note(e, |n| n.rigid = Some(rigid));
        if !qualifier.is_empty() {
            body = Self::qualify_hidden(body, &[qualifier]);
        }
        let outer = mem::replace(&mut self.stage, Stage::Pure);
        let checked = self.check(e, &body);
        if rec.is_some() {
            self.locals.pop();
        }
        let inner = mem::replace(&mut self.stage, outer);
        self.tyvars.truncate(tyvars);
        self.regions.facts.truncate(facts);
        self.forget_givens(givens);
        checked?;
        if !s.vars.is_empty() && inner != Stage::Pure {
            return error(
                "Let-Sig",
                format!(
                    "a runtime step `{}` builds a polymorphic value",
                    self.zonk_stage(&inner)
                ),
            );
        }
        self.perform(&inner)
    }
}
