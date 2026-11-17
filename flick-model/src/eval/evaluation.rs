impl<'p> Machine<'p> {
    pub fn new(program: &'p Program, fuel: u64) -> Self {
        Machine {
            program,
            mem: Vec::new(),
            lambda: Lambda::default(),
            frames: Vec::new(),
            roots: Env::new(),
            stdout: String::new(),
            fuel,
            handlers: 0,
        }
    }

    /// `Σ₀ := build(v)` for the value `v` of `layer`: the services the
    /// runner provides, which answer an operation no handler of the run does.
    pub fn provide_roots(&mut self, layer: Term) -> Result<(), Outcome> {
        let v = match self.run(layer) {
            Outcome::Value(v) => v,
            other => return Err(other),
        };
        match self.run(build(&v)?) {
            Outcome::Value(Term::Env(env)) => {
                self.roots = (*env).clone();
                Ok(())
            }
            Outcome::Value(other) => Err(stuck(format!("the root services are {other:?}"))),
            other => Err(other),
        }
    }

    /// Runs `e` to its outcome.
    pub fn run(&mut self, e: Term) -> Outcome {
        self.drive(e, None)
    }

    /// Runs `e`, showing `watch` each state the machine passes through as
    /// the configuration `⟨μ, E[e]⟩` of §3.1: what Preservation is checked on.
    pub fn run_watched(&mut self, e: Term, watch: &mut Watch<'_>) -> Outcome {
        self.drive(e, Some(watch))
    }

    fn drive(&mut self, e: Term, watch: Option<&mut Watch<'_>>) -> Outcome {
        let outcome = self.drive_loop(e, watch);
        // Terminal outcomes do not retain an execution context for a later
        // run on the same machine. In particular defects/fuel exhaustion
        // cannot leave C or a raw D activation installed.
        self.frames.clear();
        outcome
    }

    fn drive_loop(&mut self, e: Term, mut watch: Option<&mut Watch<'_>>) -> Outcome {
        let mut control = Control::Eval(e);
        loop {
            if let Some(w) = watch.as_mut() {
                w(&self.mem, &self.lambda, &self.configuration(&control));
            }
            if self.fuel == 0 {
                return Outcome::OutOfFuel;
            }
            self.fuel -= 1;
            let next = match control {
                Control::Eval(e) => self.eval(e),
                Control::Ret(v) => match self.frames.pop() {
                    None => return Outcome::Value(v),
                    Some(f) => self.ret(f, v),
                },
                Control::Raise(v) => self.unwind(v),
            };
            control = match next {
                Ok(c) => c,
                Err(outcome) => return outcome,
            };
        }
    }

    /// `E[e]`: the frames around what the machine looks at.
    fn configuration(&self, control: &Control) -> Term {
        let e = match control {
            Control::Eval(e) | Control::Ret(e) => e.clone(),
            Control::Raise(v) => Term::Raise(Rc::new(v.clone())),
        };
        self.frames.iter().rev().fold(e, |e, f| f.plug(e))
    }

    fn push(&mut self, f: Frame) {
        self.frames.push(f);
    }

    /// One step of `e` in the current context.
    fn eval(&mut self, e: Term) -> Step {
        if e.is_value() {
            return Ok(Control::Ret(e));
        }
        Ok(match e {
            Term::Var(x) => return Err(stuck(format!("free variable `{x}`"))),
            Term::Global(p) => match self.program.globals.get(&p) {
                Some(def) => Control::Eval(def.clone()),
                None => return Err(stuck(format!("no definition of `{p}`"))),
            },
            Term::App(f, a) => {
                self.push(Frame::AppL(a));
                Control::Eval((*f).clone())
            }
            Term::Let(x, sig, e1, e2) => {
                self.push(Frame::Let(x, sig, e2));
                Control::Eval((*e1).clone())
            }
            Term::LetRec(f, sig, lam, e2) => {
                let fix = Term::Fix(f.clone(), sig, lam);
                Control::Eval(subst(&e2, &[(f, fix)]))
            }
            Term::Match(e, arms) => {
                self.push(Frame::Match(arms));
                Control::Eval((*e).clone())
            }
            Term::Tuple(xs) => self.items(true, Vec::new(), &xs),
            Term::List(xs) => self.items(false, Vec::new(), &xs),
            Term::Record(fs) => self.fields(Vec::new(), &fs),
            Term::Field(e, l) => {
                self.push(Frame::Field(l));
                Control::Eval((*e).clone())
            }
            Term::Update(e, l, v) => {
                self.push(Frame::UpdateRecord(l, v));
                Control::Eval((*e).clone())
            }
            Term::Await(e) => {
                self.push(Frame::Await);
                Control::Eval((*e).clone())
            }
            Term::Try(e, arms) => {
                self.push(Frame::Try(arms));
                Control::Eval((*e).clone())
            }
            Term::Provide(l, e) => {
                self.push(Frame::ProvideLayer(e));
                Control::Eval((*l).clone())
            }
            Term::Apply(Head::Const(c), args) => return self.delta(&c, &args),
            Term::Bind(s, e) => {
                self.push(Frame::Bind(s));
                Control::Eval((*e).clone())
            }
            Term::Join(a, b) => {
                self.push(Frame::JoinLeft(b));
                Control::Eval((*a).clone())
            }
            Term::With(env, e) => {
                self.push(Frame::With(env));
                Control::Eval((*e).clone())
            }
            Term::Handle(v, arms) => return self.handle((*v).clone(), &arms, 0),
            Term::Raise(v) => Control::Raise((*v).clone()),
            Term::Structural(class, fields) => Control::Eval(structural_dict(&class, &fields)),
            Term::Die(why) => return Err(Outcome::Die(why.to_string())),
            Term::Stuck(why) => return Err(stuck(why.to_string())),
            Term::ReadOwnerEvidence => Control::Ret(Term::OwnerCandidate(self.evidence())),
            Term::DefaultOwner => Control::Ret(self.owner().map_or_else(Term::unit, Term::Own)),
            Term::WithOwnerEvidence(owner, body) => {
                self.push(Frame::OwnerOperand(false, body));
                Control::Eval((*owner).clone())
            }
            Term::ActivateOwner(owner, body) => {
                self.push(Frame::OwnerOperand(true, body));
                Control::Eval((*owner).clone())
            }
            Term::Scope(s, body) => self.open_scope(s, &body),
            Term::Handler(h) => self.install(&h),
            other => return Err(stuck(format!("no rule for {other:?}"))),
        })
    }

    /// The next item of a tuple or list, after the values `done`.
    fn items(&mut self, tuple: bool, done: Vec<Term>, rest: &[Term]) -> Control {
        let mut done = done;
        let mut i = 0;
        while i < rest.len() && rest[i].is_value() {
            done.push(rest[i].clone());
            i += 1;
        }
        match rest.get(i) {
            None if tuple => Control::Ret(Term::Tuple(done.into())),
            None => Control::Ret(Term::List(done.into())),
            Some(next) => {
                let next = next.clone();
                self.push(Frame::Items(tuple, done, rest[i + 1..].into()));
                Control::Eval(next)
            }
        }
    }

    /// The next field of a record, `{ℓ₁ = v₁, …, ℓᵢ = E, …}`.
    fn fields(&mut self, done: Vec<(Rc<str>, Term)>, rest: &[(Rc<str>, Term)]) -> Control {
        let mut done = done;
        let mut i = 0;
        while i < rest.len() && rest[i].1.is_value() {
            done.push(rest[i].clone());
            i += 1;
        }
        match rest.get(i) {
            None => Control::Ret(Term::Record(done.into())),
            Some((label, next)) => {
                let next = next.clone();
                self.push(Frame::Record(done, label.clone(), rest[i + 1..].into()));
                Control::Eval(next)
            }
        }
    }
}
