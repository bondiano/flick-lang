//! The steps of handlers of effects (`crate::handler`). A handler's frame
//! has an `id` while its body runs. An operation it handles takes the
//! frames from the handler's up, `K`: a `tail` arm runs below them and
//! gives its value back into `K`; an `abort` unwinds `K` to the handler,
//! closing its regions as `raise` does, and runs its arm there; a `control`
//! arm runs below `K` with `resume`, which puts `K` back once, and an arm
//! that ends without it lets `K` go: it unwinds as an `abort` does.

use std::cell::RefCell;
use std::rc::Rc;

use super::{Control, Frame, Machine, Outcome, Step, stuck};
use crate::handler::{Cont, Handled, Handler, OpClass, Signal};
use crate::syntax::{Const, Head, Path, Term};

impl Machine<'_> {
    /// `lookup(E, S)`: the innermost `with` that has `S`, or handler of
    /// `S`, else `Σ₀`. One of a `provide` that ended is a use after its
    /// release: work a region kept uses it (T4.3, `regions.md` §5.3).
    pub(super) fn lookup(&self, s: &Path) -> Result<Option<Term>, Outcome> {
        for f in self.frames.iter().rev() {
            match f {
                Frame::With(env) | Frame::Kept(env) => {
                    let Some(v) = env.get(s) else { continue };
                    if self.lambda.ended.iter().any(|e| Rc::ptr_eq(e, env)) {
                        return Err(stuck(format!("use after release of service `{s}`")));
                    }
                    return Ok(Some(v.clone()));
                }
                // a handler gives its operations, which perform
                Frame::Handler(id, h) | Frame::KeptHandler(id, h) => {
                    if let Some(handled) = h.effects.iter().find(|e| e.effect == *s) {
                        return Ok(Some(operations(*id, handled)));
                    }
                }
                _ => {}
            }
        }
        Ok(self.roots.get(s).cloned())
    }

    /// `try … with` arms for operations around its body.
    pub(super) fn install(&mut self, h: &Rc<Handler>) -> Control {
        self.handlers += 1;
        let id = self.handlers;
        self.frames.push(Frame::Handler(id, h.clone()));
        Control::Eval(h.body.clone())
    }

    /// The operation `op` of `effect` the handler `id` handles, given its
    /// dictionaries and arguments.
    pub(super) fn perform(
        &mut self,
        id: usize,
        effect: &Path,
        op: &str,
        class: OpClass,
        args: &[Term],
    ) -> Step {
        let at = self.frames.iter().rposition(
            |f| matches!(f, Frame::Handler(h, _) | Frame::KeptHandler(h, _) if *h == id),
        );
        let Some(at) = at else {
            return Err(stuck(format!("`{effect}.{op}` of a handler that ended")));
        };
        if self.lambda.ended_handlers.contains(&id) {
            return Err(stuck(format!("`{effect}.{op}` of a handler that ended")));
        }
        let (Frame::Handler(_, h) | Frame::KeptHandler(_, h)) = &self.frames[at] else {
            unreachable!("a handler's frame, found above")
        };
        let Some(function) = h.op(effect, op).map(|o| o.function.clone()) else {
            return Err(stuck(format!("the handler has no arm for `{effect}.{op}`")));
        };
        let arm = Term::apps(function, args.iter().cloned());
        Ok(match class {
            OpClass::Tail => {
                let k = self.frames.split_off(at);
                self.frames.push(Frame::Resume(k));
                Control::Eval(arm)
            }
            OpClass::Abort => Control::Raise(Term::Signal(Rc::new(Signal::Abort { id, arm }))),
            OpClass::Control => {
                let k = self.frames.split_off(at);
                let cont = Cont(Rc::new(RefCell::new(Some(k))));
                self.frames.push(Frame::Discontinue(id, cont.clone()));
                Control::Eval(Term::app(arm, Term::Cont(cont)))
            }
        })
    }

    /// `resume v`: the rest of the body again, once.
    pub(super) fn resume(&mut self, cont: &Cont, v: Term) -> Step {
        match cont.0.borrow_mut().take() {
            Some(k) => {
                self.frames.extend(k);
                Ok(Control::Ret(v))
            }
            None => Err(Outcome::Die(
                "`resume` ran twice: the rest of a computation resumes once".to_owned(),
            )),
        }
    }

    /// The end of a `control` arm, which ended with `v` or raises it: a body
    /// it did not resume is let go first.
    pub(super) fn let_go(&mut self, id: usize, cont: &Cont, then: Result<Term, Term>) -> Control {
        match cont.0.borrow_mut().take() {
            Some(k) => {
                self.frames.extend(k);
                Control::Raise(Term::Signal(Rc::new(Signal::Discontinue { id, then })))
            }
            None => match then {
                Ok(v) => Control::Ret(v),
                Err(v) => Control::Raise(v),
            },
        }
    }

    /// `v` returns to a frame of a handler.
    pub(super) fn ret_handler(&mut self, f: Frame, v: Term) -> Control {
        match f {
            Frame::Handler(id, h) => {
                self.lambda.ended_handlers.insert(id);
                match &h.ret {
                    Some(ret) => Control::Eval(Term::app(ret.clone(), v)),
                    None => Control::Ret(v),
                }
            }
            Frame::Resume(k) => {
                self.frames.extend(k);
                Control::Ret(v)
            }
            Frame::Discontinue(id, cont) => self.let_go(id, &cont, Ok(v)),
            _ => Control::Ret(v),
        }
    }

    /// `v` unwinds through a frame of a handler: a handler's own signals
    /// end there, and its error arms take the errors they match; the rest
    /// of a body whose arm never ends is let go. `None`: it goes on.
    pub(super) fn unwind_handler(&mut self, f: Frame, v: &Term) -> Option<Step> {
        match f {
            Frame::Handler(id, h) => {
                self.lambda.ended_handlers.insert(id);
                match v {
                    Term::Signal(s) if s.id() == id => Some(Ok(match &**s {
                        Signal::Abort { arm, .. } => Control::Eval(arm.clone()),
                        Signal::Discontinue { then: Ok(v), .. } => Control::Ret(v.clone()),
                        Signal::Discontinue { then: Err(v), .. } => Control::Raise(v.clone()),
                    })),
                    Term::Signal(_) => None,
                    _ if h.errors.is_empty() => None,
                    _ => Some(self.handle(v.clone(), &h.errors, 0)),
                }
            }
            // the arm's value never comes
            Frame::Resume(k) => {
                let Some(Frame::Handler(id, _)) = k.first() else {
                    return Some(Err(stuck("the rest of a body without its handler")));
                };
                let id = *id;
                self.frames.extend(k);
                let then = Err(v.clone());
                Some(Ok(Control::Raise(Term::Signal(Rc::new(
                    Signal::Discontinue { id, then },
                )))))
            }
            Frame::Discontinue(id, cont) => Some(Ok(self.let_go(id, &cont, Err(v.clone())))),
            _ => None,
        }
    }
}

/// What `ask_S` gives of the handler `id` of `S`: each operation, which
/// performs once it has its dictionaries and arguments.
pub(super) fn operations(id: usize, handled: &Handled) -> Term {
    Term::Record(
        handled
            .ops
            .iter()
            .map(|o| {
                let perform = Const::Perform {
                    id,
                    effect: handled.effect.clone(),
                    op: o.name.clone(),
                    class: o.class,
                    arity: o.dicts + o.arity,
                };
                (
                    o.name.clone(),
                    Term::Apply(Head::Const(perform), Rc::from([])),
                )
            })
            .collect(),
    )
}
