//! The seam of Flick's own runtime, `@flick-lang/runtime/rt`. A runtime stage
//! is a generator the kernel drives: what it yields is what it calls, what it
//! returns may be a call, which replaces its frame. A call is emitted without
//! knowing what the callee is: a generator object, an instruction, or the plain
//! value of a function of another stage are all fine to yield or return, which
//! is how a function of one stage runs where another is expected. A Sync stage
//! is a plain function: it gives what such a call gave to `$rt.runIn`.

use super::function::{balanced_without_yield, lone_yield};
use super::{Gen, indent, names};
use crate::codegen::tail::Calls;
use crate::ir::{Arm, Expr, Foreign, ForeignMode, Handler, Stage};
use crate::types::{Lift, OpClass};

/// Which calls jump: with the kernel, those of any stage.
pub(super) fn calls(kernel: bool) -> Calls {
    if kernel { Calls::Direct } else { Calls::Pure }
}

/// `js` without its `(yield …)`, if it is that alone and yields nothing else:
/// in tail position its callee replaces the frame.
fn unyielded(js: &str) -> Option<String> {
    let probe = format!("return {};", js.replacen("(yield ", "(yield* ", 1));
    lone_yield(&probe)
}

/// Whether `stmts` yield outside a nested generator.
fn yields(stmts: &[String]) -> bool {
    stmts
        .iter()
        .any(|s| s.contains("yield") && !balanced_without_yield(s))
}

impl Gen<'_> {
    pub(super) fn calls(&self) -> Calls {
        calls(self.kernel)
    }

    /// The value of a tail position, `v`: a lone runtime call, unyielded.
    pub(super) fn tail_call(&self, v: String) -> String {
        if self.kernel && self.generator {
            return unyielded(&v).unwrap_or(v);
        }
        v
    }

    /// A function of `heads` running `stmts`: `uncurried`, one taking them all,
    /// otherwise an arrow per head but the last. A generator when it yields; a
    /// body that only calls and returns needs no frame of its own.
    pub(super) fn stage_function(
        &self,
        heads: &[String],
        stmts: &[String],
        uncurried: bool,
    ) -> String {
        let (outer, own) = if uncurried {
            (&[][..], heads.join(", "))
        } else {
            let at = heads.len().saturating_sub(1);
            (&heads[..at], heads[at..].join(", "))
        };
        let mut text = if yields(stmts) {
            self.generator_function(&own, stmts)
        } else {
            let body = match stmts {
                [only] if only.starts_with("return ") && !only.contains('\n') => {
                    let v = &only["return ".len()..only.len() - 1];
                    if super::expr::unmarked(v).starts_with('{') {
                        format!("({v})")
                    } else {
                        v.to_owned()
                    }
                }
                _ => format!("{{\n{}}}", indent(stmts)),
            };
            format!("{} {body}", self.arrow(&own))
        };
        for head in outer.iter().rev() {
            text = format!("{} {text}", self.arrow(head));
        }
        text
    }

    /// Runs the call `js`, in `owner` when a `Within` gave one.
    pub(super) fn kernel_run(&mut self, owner: Option<String>, js: &str) -> String {
        let js = match owner {
            Some(owner) => format!("{}({owner}, () => {js})", self.rt("within")),
            None => js.to_owned(),
        };
        if self.generator {
            format!("(yield {js})")
        } else {
            format!("{}({js})", self.rt("runIn"))
        }
    }

    /// `await v`: a task is a function of nothing that runs when called.
    pub(super) fn await_task(&mut self, owner: Option<String>, v: String) -> String {
        let v = if owner.is_some() && self.generator && v.contains("yield") {
            self.spill(v)
        } else {
            v
        };
        let called = if super::expr::atomic(&v) {
            format!("{v}()")
        } else {
            format!("({v})()")
        };
        self.run(owner, &called)
    }

    /// `try` in a stage that runs at once: the body runs on the stack.
    pub(super) fn try_at_once(&mut self, body: &Expr, arms: &[Arm]) -> String {
        let tail = self.tail.take();
        let (stmts, ()) = self.block(|g| g.tail_expr(body));
        self.tail = tail;
        let thunk = self.stage_function(&[], &stmts, true);
        let error = self.temp();
        let (handlers, ()) = self.block(|g| g.handlers(&error, arms));
        format!(
            "{}({thunk}, {} {{\n{}}})",
            self.rt("trySync"),
            self.arrow(&error),
            indent(&handlers)
        )
    }

    /// A foreign `sync` or `effect` call at its runtime stage: what it gives,
    /// a value or a call, is what the stage gives.
    pub(super) fn kernel_foreign(
        &mut self,
        f: &Foreign,
        copy: bool,
        call: &str,
        catch: Option<String>,
    ) -> String {
        if f.mode == ForeignMode::Sync {
            let value = self.foreign_result(f, copy, call);
            return match catch {
                Some(c) => format!("{}(() => {value}, {c})", self.rt("foreignSync")),
                None => value,
            };
        }
        let called = match catch {
            Some(c) => format!("{}(() => {call}, {c})", self.rt("foreignEffect")),
            None => call.to_owned(),
        };
        let info = &f.adapter;
        if info.result.nullable || info.result.unit || copy {
            let value = self.foreign_result(f, copy, "$v");
            format!("{}({called}, ($v) => {value})", self.rt("mapped"))
        } else {
            called
        }
    }

    /// `$rt.handle` of the operations of `h`.
    pub(super) fn kernel_handle(&mut self, h: &Handler) -> String {
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
                operations.push(format!(
                    "[{}, {function}, \"{class}\", {}, {}]",
                    names::string(&op.name),
                    op.dicts,
                    op.arity
                ));
            }
            installs.push(format!("[{tag}, [{}]]", operations.join(", ")));
        }
        let body = self.gen_block(&h.body);
        let ret = match &h.ret {
            Some(ret) => {
                let ret = self.expr(ret);
                self.spill(ret)
            }
            None => "undefined".to_owned(),
        };
        let errors = if h.errors.is_empty() {
            "undefined".to_owned()
        } else {
            let error = self.temp();
            let (stmts, ()) = self.block(|g| g.handlers(&error, &h.errors));
            format!("{} {{\n{}}}", self.arrow(&error), indent(&stmts))
        };
        let handle = self.rt("handle");
        let installs = installs.join(", ");
        self.kernel_run(
            None,
            &format!("{handle}([{installs}], {body}, {ret}, {errors})"),
        )
    }

    /// A JS function that turns a function into the one of the stage `lift`
    /// expects. A call adapts itself; only a result that changes takes work.
    pub(super) fn kernel_lift(&mut self, lift: &Lift) -> String {
        match lift {
            Lift::Stage { from, to, result } => {
                let inner = result.as_deref().map(|r| self.kernel_lift(r));
                let call = "$f($x)";
                let body = match (from, to, inner) {
                    (Stage::Runtime, Stage::Pure | Stage::Sync, inner) => {
                        let ran = format!("{}({call})", self.rt("runIn"));
                        inner.map_or(ran.clone(), |r| format!("({r})({ran})"))
                    }
                    (Stage::Runtime | Stage::Sync, Stage::Runtime, Some(r)) => {
                        format!("{}({call}, {r})", self.rt("mapped"))
                    }
                    (_, _, Some(r)) => format!("({r})({call})"),
                    (_, _, None) => call.to_owned(),
                };
                format!("{} {} {body}", self.arrow("$f"), self.arrow("$x"))
            }
            Lift::Eff(inner) => {
                let inner = self.kernel_lift(inner);
                let mapped = self.rt("mapped");
                format!(
                    "{} {} {mapped}($e(), {inner})",
                    self.arrow("$e"),
                    self.arrow("")
                )
            }
        }
    }
}
