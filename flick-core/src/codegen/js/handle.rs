//! A handler of effects: `$rt.handle` installs, around the body, the
//! functions of each effect's operations as the implementation of its
//! service, and handles the body's value and errors.

use super::{Gen, indent, names};
use crate::ir::Handler;
use crate::types::OpClass;

impl Gen<'_> {
    pub(super) fn handle(&mut self, h: &Handler) -> String {
        if self.kernel {
            return self.kernel_handle(h);
        }
        let mut installs = Vec::new();
        for handled in &h.effects {
            let tag = self.service_ref(&handled.effect);
            let mut fields = Vec::new();
            let mut aborts = Vec::new();
            for op in &handled.operations {
                let function = self.expr(&op.function);
                fields.push(format!("{}: {function}", names::string(&op.name)));
                if op.class == OpClass::Abort {
                    aborts.push(names::string(&op.name));
                }
            }
            installs.push(format!(
                "[{tag}, {{ {} }}, [{}]]",
                fields.join(", "),
                aborts.join(", ")
            ));
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
        self.run(
            None,
            &format!("{handle}([{installs}], {body}, {ret}, {errors})"),
        )
    }
}
