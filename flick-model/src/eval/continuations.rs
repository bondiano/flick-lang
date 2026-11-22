impl Machine<'_> {
    /// The value `v` returns to the frame `f`.
    fn ret(&mut self, f: Frame, v: Term) -> Step {
        Ok(match f {
            Frame::AppL(a) => {
                self.push(Frame::AppR(v));
                Control::Eval((*a).clone())
            }
            Frame::OwnerOperand(raw, body) => return self.enter_owner_context(raw, &body, v),
            Frame::AppR(Term::Cont(cont)) => return self.resume(&cont, v),
            Frame::AppR(func) => return Self::apply(func, v),
            // a λ keeps its signature as `fix x. λ`, `x` not free in it: a
            // state types it against σ, as the source did (`docs/formal/core.md#stepping-a-signed-let-loses-its-signature`)
            Frame::Let(x, Some(sig), e2) if matches!(v, Term::Lam(..)) => {
                let fix = Term::Fix(x.clone(), Some(sig), Rc::new(v));
                Control::Eval(subst(&e2, &[(x, fix)]))
            }
            Frame::Let(x, _, e2) => Control::Eval(subst(&e2, &[(x, v)])),
            Frame::Match(arms) => return self.select(v, &arms, 0),
            Frame::Guard {
                scrutinee,
                arms,
                at,
                body,
                handler,
            } => match v {
                Term::Lit(Lit::Bool(true)) => Control::Eval(body),
                Term::Lit(Lit::Bool(false)) if handler => {
                    return self.handle(scrutinee, &arms, at + 1);
                }
                Term::Lit(Lit::Bool(false)) => return self.select(scrutinee, &arms, at + 1),
                other => return Err(stuck(format!("a guard gave {other:?}"))),
            },
            Frame::Items(tuple, mut done, rest) => {
                done.push(v);
                self.items(tuple, done, &rest)
            }
            Frame::Record(mut done, label, rest) => {
                done.push((label, v));
                self.fields(done, &rest)
            }
            Frame::Field(l) => match &v {
                Term::Record(fs) => return read_field(fs, &l),
                _ => return Err(stuck(format!("`.{l}` of {v:?}"))),
            },
            Frame::UpdateRecord(l, e) => {
                self.push(Frame::UpdateValue(v, l));
                Control::Eval((*e).clone())
            }
            Frame::UpdateValue(record, l) => match record {
                Term::Record(fs) => {
                    let mut out: Vec<_> = fs
                        .iter()
                        .map(|(m, x)| (m.clone(), if *m == l { v.clone() } else { x.clone() }))
                        .collect();
                    out.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
                    Control::Ret(Term::Record(out.into()))
                }
                other => return Err(stuck(format!("`with` on {other:?}"))),
            },
            // `await` runs the body again each time: a task is not a cache
            Frame::Await => match v {
                Term::Task(e) => Control::Eval((*e).clone()),
                other => return Err(stuck(format!("`await` of {other:?}"))),
            },
            // try v with ā → v; a `provide` ends with its `with` (`regions.md` §2.7)
            Frame::Try(_)
            | Frame::Kept(_)
            | Frame::KeptHandler(..)
            | Frame::Evidence(_)
            | Frame::Activation(_) => Control::Ret(v),
            Frame::With(env) => {
                self.lambda.ended.push(env);
                Control::Ret(v)
            }
            Frame::ProvideLayer(e) => {
                self.push(Frame::ProvideEnv(e));
                Control::Eval(build(&v)?)
            }
            Frame::ProvideEnv(e) => match v {
                Term::Env(env) => {
                    self.push(Frame::With(env));
                    Control::Eval((*e).clone())
                }
                other => return Err(stuck(format!("`provide` of {other:?}"))),
            },
            Frame::Bind(s) => Control::Ret(Term::Env(Rc::new(Env::from([(s, v)])))),
            Frame::JoinLeft(b) => {
                self.push(Frame::JoinRight(v));
                Control::Eval((*b).clone())
            }
            Frame::JoinRight(left) => match (left, v) {
                (Term::Env(a), Term::Env(b)) => {
                    let mut env = (*a).clone();
                    env.extend(b.iter().map(|(k, x)| (k.clone(), x.clone())));
                    Control::Ret(Term::Env(Rc::new(env)))
                }
                (a, b) => return Err(stuck(format!("join of {a:?} and {b:?}"))),
            },
            Frame::Inside(pi, s) => self.close(pi, s, Ok(v)),
            Frame::Settle(l) => self.settle(l, Ok(v)),
            f @ (Frame::Handler(..) | Frame::Resume(_) | Frame::Discontinue(..)) => {
                self.ret_handler(f, v)
            }
            // what a child or a finalizer gives is dropped
            Frame::Close(pi, s, ended) => self.close(pi, s, ended),
        })
    }

    /// The initializer of the lazy cell `l` ended with `ended`, which goes
    /// on: a value or an error settles the cell, a handler's signal makes it
    /// pending again.
    fn settle(&mut self, l: usize, ended: Result<Term, Term>) -> Control {
        if let Some(state) = settled(&self.mem[l], ended.as_ref()) {
            self.mem[l] = state;
        }
        match ended {
            Ok(v) => Control::Ret(v),
            Err(v) => Control::Raise(v),
        }
    }

    /// `f v`.
    fn apply(f: Term, v: Term) -> Step {
        match f {
            Term::Lam(x, body) => Ok(Control::Eval(subst(&body, &[(x, v)]))),
            Term::Fix(name, sig, lam) => match &*lam {
                Term::Lam(x, body) => {
                    let fix = Term::Fix(name.clone(), sig, lam.clone());
                    Ok(Control::Eval(subst(body, &[(name, fix), (x.clone(), v)])))
                }
                other => Err(stuck(format!("`fix` of {other:?}"))),
            },
            Term::Apply(head, args) if args.len() < head.arity() => {
                let mut args = args.to_vec();
                args.push(v);
                Ok(Control::Eval(Term::Apply(head, args.into())))
            }
            other => Err(stuck(format!("applied {other:?}, which is not a function"))),
        }
    }

    /// `match v with ā`, from arm `at`.
    fn select(&mut self, v: Term, arms: &Rc<[Arm]>, at: usize) -> Step {
        self.arms(v, arms, at, false)
    }

    /// `handle v with ā` (§3.5): like `match`, but no arm rethrows.
    fn handle(&mut self, v: Term, arms: &Rc<[Arm]>, at: usize) -> Step {
        self.arms(v, arms, at, true)
    }

    fn arms(&mut self, v: Term, arms: &Rc<[Arm]>, at: usize, handler: bool) -> Step {
        for (i, arm) in arms.iter().enumerate().skip(at) {
            let mut delta = Vec::new();
            if !matches(&arm.pat, &v, &mut delta) {
                continue;
            }
            let body = subst(&arm.body, &delta);
            return Ok(match &arm.guard {
                None => Control::Eval(body),
                Some(g) => {
                    self.push(Frame::Guard {
                        scrutinee: v,
                        arms: arms.clone(),
                        at: i,
                        body,
                        handler,
                    });
                    Control::Eval(subst(g, &delta))
                }
            });
        }
        if handler {
            Ok(Control::Raise(v))
        } else {
            Err(stuck(format!("no arm matches {v:?}")))
        }
    }

    /// `E[F[raise v]] → E[raise v]` up to the nearest `try` (§3.5): a
    /// `with` frame goes with the others, which restores the services. A
    /// region closes on the way (`regions.md` §3.3).
    fn unwind(&mut self, v: Term) -> Step {
        while let Some(f) = self.frames.pop() {
            match f {
                // a handler's signal is no error
                Frame::Try(arms) if !matches!(v, Term::Signal(_)) => {
                    return self.handle(v, &arms, 0);
                }
                f @ (Frame::Handler(..) | Frame::Resume(_) | Frame::Discontinue(..)) => {
                    if let Some(step) = self.unwind_handler(f, &v) {
                        return step;
                    }
                }
                Frame::Inside(pi, s) => return Ok(self.close(pi, s, Err(v))),
                Frame::Settle(l) => return Ok(self.settle(l, Err(v))),
                // shortcut: a child or a finalizer that fails stops the machine, its supervisor and the order of failures are λRT's (`runtime.md`); run the supervisor once a generated child can fail
                Frame::Close(pi, ..) => {
                    return Err(stuck(format!("a child or finalizer of π{pi} raised {v:?}")));
                }
                Frame::With(env) => {
                    self.lambda.ended.push(env);
                }
                _ => {}
            }
        }
        Err(Outcome::Raise(v))
    }

    /// The steps of the constants (§3.4), and of the natives that stand
    /// for foreign bindings under hypothesis G1.
    fn delta(&mut self, c: &Const, args: &[Term]) -> Step {
        Ok(match (c, args) {
            (Const::RefMake, [v]) => {
                self.mem.push(v.clone());
                Control::Ret(Term::Loc(self.mem.len() - 1))
            }
            (Const::RefGet, [Term::Loc(l)]) => Control::Ret(self.mem[*l].clone()),
            (Const::RefSet, [Term::Loc(l), v]) => {
                self.mem[*l] = v.clone();
                Control::Ret(Term::unit())
            }
            (Const::Raise, [v]) => Control::Raise(v.clone()),
            (Const::SeqIter, [v @ Term::Apply(Head::Const(Const::SeqThunk), _)]) => {
                Control::Ret(v.clone())
            }
            (Const::SeqIter, [v @ Term::List(_)]) => {
                Control::Eval(seq::from_list_term(v.clone()))
            }
            (Const::SeqIter, [other]) => {
                return Err(stuck(format!(
                    "unsupported `for` collection source: {other:?}"
                )));
            }
            (Const::SeqNext, [Term::Apply(Head::Const(Const::SeqThunk), args)]) => {
                Control::Eval(Term::app(args[0].clone(), Term::unit()))
            }
            (Const::SeqSync, [thunk]) => Control::Eval(seq::sync_thunk(thunk.clone())),
            (Const::SeqFlatMapSync, [f, xs]) => {
                Control::Eval(seq::flat_map_term(f.clone(), xs.clone()))
            }
            (Const::Ask(s), [_]) => {
                // Source operations are `(ask_S ()).op`, looked up only
                // at their first runtime stage (translate/expr.rs). Select
                // the nearest arm for that operation, not just its effect.
                // A bare internal `ask_S` retains its whole-service lookup.
                let operation = match self.frames.last() {
                    Some(Frame::Field(name)) => Some(&**name),
                    _ => None,
                };
                match self.lookup(s, operation)? {
                    Some(v) => Control::Ret(v),
                    None => return Err(stuck(format!("no implementation of service `{s}`"))),
                }
            }
            (Const::Prim(p), args) => match prim::run(*p, args) {
                Ok(v) => Control::Eval(v),
                Err(why) => return Err(stuck(why)),
            },
            (Const::Region(p, op), args) => return self.region_step(p, *op, args),
            (
                Const::Perform {
                    id,
                    effect,
                    op,
                    class,
                    ..
                },
                args,
            ) => return self.perform(*id, effect, op, *class, args),
            (Const::Native(n), args) => match (n.run)(args, &mut self.stdout) {
                Ok(t) => Control::Eval(t),
                Err(why) => return Err(stuck(format!("{}: {why}", n.export))),
            },
            (c, args) => return Err(stuck(format!("{c:?} of {args:?}"))),
        })
    }
}

/// The field `l` of a record value: a default's getter runs its body there,
/// as a lazy field of a dictionary does on both targets.
fn read_field(fs: &[(Name, Term)], l: &Name) -> Step {
    match fs.iter().find(|(m, _)| m == l) {
        Some((_, Term::Lam(get, body))) if &**get == GETTER => Ok(Control::Eval((**body).clone())),
        Some((_, x)) => Ok(Control::Ret(x.clone())),
        None => Err(stuck(format!("no field `{l}`"))),
    }
}
