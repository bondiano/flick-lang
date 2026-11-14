//! A handler of effects: `$rt.handle` installs, around the body, the
//! functions of each effect's operations as the implementation of its
//! service, and handles the body's value and errors.

use super::doc::{After, Before, Js, array, call, object};
use super::{Gen, expr::key};
use crate::ir::Handler;
use flick_ir::OpClass;
use flick_pretty::docvec;

impl<'d> Gen<'_, 'd> {
    pub(super) fn handle(&mut self, h: &Handler) -> Js<'d> {
        if self.kernel {
            return self.kernel_handle(h);
        }
        let mut installs = Vec::new();
        for handled in &h.effects {
            let tag = self.service_ref(&handled.effect);
            let mut fields = Vec::new();
            let mut parts = Vec::new();
            let mut aborts = Vec::new();
            for op in &handled.operations {
                let function = self.expr(&op.function);
                let name = self.leaf(&key(&op.name));
                fields.push(docvec![
                    self.a,
                    name.doc,
                    ": ",
                    function.at(self.a, Before::Delimiter, After::Delimiter)
                ]);
                parts.push(function);
                if op.class == OpClass::Abort {
                    aborts.push(self.string(&op.name));
                }
            }
            installs.push(array(
                self.a,
                &[tag, object(self.a, &fields, &parts), array(self.a, &aborts)],
            ));
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
            self.handlers(&error, &h.errors)
        };
        let handle = self.rt("handle");
        let called = call(
            self.a,
            handle,
            &[array(self.a, &installs), body, ret, errors],
        );
        self.run(None, called)
    }
}
