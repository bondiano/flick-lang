//! Native runtime stages yield calls to the kernel; a tail call replaces its frame.
use super::Gen;
use super::doc::{
    self, After, Before, Head, Js, MULTILINE, Stmt, YIELD_TEXT, YIELDS, array, block_js, call,
};
use crate::codegen::tail::Calls;
use crate::ir::{Arm, Expr, Foreign, ForeignMode, Handler, Stage};
use flick_ir::{Lift, OpClass};
use flick_pretty::docvec;

pub(super) fn calls(kernel: bool) -> Calls {
    if kernel { Calls::Direct } else { Calls::Pure }
}
impl<'d> Gen<'_, 'd> {
    pub(super) fn calls(&self) -> Calls {
        calls(self.kernel)
    }
    pub(super) fn tail_call(&self, v: Js<'d>) -> Js<'d> {
        if self.kernel
            && self.generator
            && let Some((doc, flags)) = v.effect
        {
            return Js::code(doc, Head::Ident, &[])
                .with(flags)
                .marked_by(self.a, v.marks);
        }
        v
    }
    pub(super) fn stage_function(
        &self,
        heads: &[String],
        stmts: &[Stmt<'d>],
        uncurried: bool,
    ) -> Js<'d> {
        let (outer, own) = if uncurried {
            (&[][..], heads.join(", "))
        } else {
            let at = heads.len().saturating_sub(1);
            (&heads[..at], heads[at..].join(", "))
        };
        let a = self.a;
        let mut text = if doc::stmt_flags(stmts) & YIELDS != 0 {
            self.generator_function(&own, stmts)
        } else {
            let body = match stmts {
                [only] if !only.multiline() && only.ret.is_some() => {
                    let (marks, value) = only.ret.expect("return");
                    let value = value.marked_by(a, marks);
                    if value.lead() == Head::Brace {
                        Js::code(
                            docvec![
                                a,
                                "(",
                                value.at(a, Before::Delimiter, After::Delimiter),
                                ")"
                            ],
                            value.after_paren(),
                            &[value],
                        )
                    } else {
                        value
                    }
                }
                _ => block_js(a, stmts),
            };
            self.lambda_of(&own, body)
        };
        for head in outer.iter().rev() {
            text = self.lambda_of(head, text);
        }
        text
    }
    pub(super) fn kernel_run(&mut self, owner: Option<String>, js: Js<'d>) -> Js<'d> {
        let a = self.a;
        let js = match owner {
            Some(owner) => {
                let within = self.rt("within");
                call(a, within, &[self.leaf(&owner), self.thunk(js)])
            }
            None => js,
        };
        if self.generator {
            let alone = js.flags & (YIELDS | MULTILINE) == 0;
            Js {
                effect: alone.then(|| (js.at(a, Before::Arrow, After::Delimiter), js.flags)),
                ..Js::code(
                    docvec![a, "yield ", js.at(a, Before::Other, After::Delimiter)],
                    Head::Ident,
                    &[js],
                )
                .with(YIELDS | YIELD_TEXT)
                .in_parens()
            }
        } else {
            let run = self.rt("runIn");
            call(a, run, &[js])
        }
    }
    pub(super) fn await_task(&mut self, owner: Option<String>, v: Js<'d>) -> Js<'d> {
        let v = if owner.is_some() && self.generator && v.flags & YIELD_TEXT != 0 {
            self.spill(v)
        } else {
            v
        };
        let callable = if v.atom().is_some() { v } else { v.in_parens() };
        let called = call(self.a, callable, &[]);
        self.run(owner, called)
    }
    pub(super) fn try_at_once(&mut self, body: &Expr, arms: &[Arm]) -> Js<'d> {
        let tail = self.tail.take();
        let (stmts, ()) = self.block(|g| g.tail_expr(body));
        self.tail = tail;
        let thunk = self.stage_function(&[], &stmts, true);
        let error = self.temp();
        let (handlers, ()) = self.block(|g| g.handlers(&error, arms));
        let handler = self.lambda_of(&error, block_js(self.a, &handlers));
        let tr = self.rt("trySync");
        call(self.a, tr, &[thunk, handler])
    }
    pub(super) fn kernel_foreign(
        &mut self,
        f: &Foreign,
        copy: bool,
        called: Js<'d>,
        catch: Option<Js<'d>>,
    ) -> Js<'d> {
        let a = self.a;
        if f.mode == ForeignMode::Sync {
            let value = self.foreign_result(f, copy, called);
            return match catch {
                Some(c) => {
                    let foreign = self.rt("foreignSync");
                    call(a, foreign, &[self.thunk(value), c])
                }
                None => value,
            };
        }
        let called = match catch {
            Some(c) => {
                let foreign = self.rt("foreignEffect");
                call(a, foreign, &[self.thunk(called), c])
            }
            None => called,
        };
        let info = &f.adapter;
        if info.result.nullable || info.result.unit || copy {
            let value = self.foreign_result(f, copy, self.leaf("$v"));
            let mapped = self.rt("mapped");
            call(a, mapped, &[called, self.lambda_of("$v", value)])
        } else {
            called
        }
    }
    pub(super) fn kernel_handle(&mut self, h: &Handler) -> Js<'d> {
        let mut installs = Vec::new();
        for handled in &h.effects {
            let tag = self.service_ref(&handled.effect);
            let mut operations = Vec::new();
            for op in &handled.operations {
                let function = self.expr(&op.function);
                let class = match op.class {
                    OpClass::Tail => "tail",
                    OpClass::Abort => "abort",
                    OpClass::Control => "control",
                };
                operations.push(array(
                    self.a,
                    &[
                        self.string(&op.name),
                        function,
                        self.string(class),
                        self.leaf(&op.dicts.to_string()),
                        self.leaf(&op.arity.to_string()),
                    ],
                ));
            }
            installs.push(array(self.a, &[tag, array(self.a, &operations)]));
        }
        let body = self.gen_block(&h.body);
        let ret = match &h.ret {
            Some(ret) => {
                let ret = self.expr(ret);
                self.spill(ret)
            }
            None => self.leaf("undefined"),
        };
        let errors = if h.errors.is_empty() {
            self.leaf("undefined")
        } else {
            let error = self.temp();
            let (stmts, ()) = self.block(|g| g.handlers(&error, &h.errors));
            self.lambda_of(&error, block_js(self.a, &stmts))
        };
        let handle = self.rt("handle");
        let called = call(
            self.a,
            handle,
            &[array(self.a, &installs), body, ret, errors],
        );
        self.kernel_run(None, called)
    }
    pub(super) fn kernel_lift(&mut self, lift: &Lift) -> Js<'d> {
        let a = self.a;
        match lift {
            Lift::Stage { from, to, result } => {
                let inner = result.as_deref().map(|r| self.kernel_lift(r));
                let called = call(a, self.leaf("$f"), &[self.leaf("$x")]);
                let body = match (from, to, inner) {
                    (Stage::Runtime, Stage::Pure | Stage::Sync, inner) => {
                        let run = self.rt("runIn");
                        let ran = call(a, run, &[called]);
                        inner.map_or(ran, |r| call(a, r.in_parens(), &[ran]))
                    }
                    (Stage::Runtime | Stage::Sync, Stage::Runtime, Some(r)) => {
                        let mapped = self.rt("mapped");
                        call(a, mapped, &[called, r])
                    }
                    (_, _, Some(r)) => call(a, r.in_parens(), &[called]),
                    (_, _, None) => called,
                };
                self.lambda_of("$f", self.lambda_of("$x", body))
            }
            Lift::Eff(inner) => {
                let inner = self.kernel_lift(inner);
                let mapped = self.rt("mapped");
                let called = call(a, self.leaf("$e"), &[]);
                self.lambda_of("$e", self.lambda_of("", call(a, mapped, &[called, inner])))
            }
        }
    }
}
