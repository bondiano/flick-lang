//! A handler of effects: `flickrt.Handle` installs, around the body, the
//! functions of each effect's operations as the implementation of its
//! service, and handles the body's value and errors.

use flick_pretty::docvec;

use super::Gen;
use super::layout::{Code, call, list};
use crate::ir::Handler;
use crate::types::OpClass;

impl<'a> Gen<'a> {
    pub(super) fn handle(&mut self, h: &Handler) -> Code<'a> {
        let a = self.a;
        let mut installs = Vec::new();
        for handled in &h.effects {
            let mut fields = Vec::new();
            let mut aborts = Vec::new();
            let mut controls = Vec::new();
            for op in &handled.operations {
                let function = self.expr(&op.function);
                let function = self.spill(function);
                fields.push(self.string(&op.name));
                fields.push(function.doc);
                match op.class {
                    OpClass::Abort => aborts.push(self.string(&op.name)),
                    OpClass::Control => {
                        let name = self.string(&op.name);
                        let arity = op.arity.to_string();
                        controls.push(docvec![a, "{Name: ", name, ", Arity: ", arity, "}"]);
                    }
                    OpClass::Tail => {}
                }
            }
            let arms = call(a, "flickrt.Rec", fields);
            let aborts = list(a, "[]string{", aborts, "}");
            let controls = list(a, "[]flickrt.Control{", controls, "}");
            let key = self.string(&handled.effect);
            installs.push(docvec![
                a,
                "{Key: ",
                key,
                ", Arms: ",
                arms,
                ", Aborts: ",
                aborts,
                ", Controls: ",
                controls,
                "}"
            ]);
        }
        let ret = match &h.ret {
            Some(ret) => {
                let ret = self.expr(ret);
                self.spill(ret).doc
            }
            None => a.text("nil"),
        };
        let body = self.task(&h.body);
        let errors = if h.errors.is_empty() {
            a.text("nil")
        } else {
            self.error_handler(&h.errors)
        };
        let fb = self.fiber.expect("a handler runs in a fiber");
        let installs = list(a, "[]flickrt.Install{", installs, "}");
        call(
            a,
            "flickrt.Handle",
            [a.text(fb), installs, body.doc, ret, errors],
        )
        .into()
    }
}
