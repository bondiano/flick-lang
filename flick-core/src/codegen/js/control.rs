//! Branches, typed error dispatch and local loop control.
use super::{Gen, indent};
use crate::codegen::loops;
use crate::ir::{Arm, Expr, ExprKind};

impl Gen<'_> {
    pub(super) fn matched(&mut self, scrutinee: &Expr, arms: &[Arm]) -> String {
        let s = self.expr(scrutinee);
        let s = self.spill(s);
        let r = self.temp();
        self.buf.push(self.declare(&r));
        self.arms(&s, arms, Some(&r));
        r
    }
    pub(super) fn loop_expr(&mut self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::ControlLoop {
                id,
                condition,
                body,
                iterable,
            } => {
                let lowered = loops::lower(id, condition.as_deref(), body, *iterable, e.span);
                self.expr(&lowered)
            }
            ExprKind::LoopExit { .. } => unreachable!("loop exits are lowered with their loop"),
            ExprKind::Loop { condition, body } => {
                let (stmts, ()) = self.block(|g| {
                    let condition = g.expr(condition);
                    g.buf.push(format!("if (!({condition})) break;"));
                    g.discard(body);
                });
                self.buf
                    .push(format!("while (true) {{\n{}}}", indent(&stmts)));
                "undefined".to_owned()
            }
            _ => unreachable!("loop expression"),
        }
    }
    pub(super) fn if_expr(&mut self, c: &Expr, t: &Expr, f: &Expr) -> String {
        let c = self.expr(c);
        let (ts, tv) = self.block(|g| g.expr(t));
        let (fs, fv) = self.block(|g| g.expr(f));
        if ts.is_empty() && fs.is_empty() {
            return format!("({c} ? {tv} : {fv})");
        }
        let r = self.temp();
        let declare = self.declare(&r);
        self.buf.push(format!(
            "{declare}\nif ({c}) {{\n{}  {r} = {tv};\n}} else {{\n{}  {r} = {fv};\n}}",
            indent(&ts),
            indent(&fs)
        ));
        r
    }
    pub(super) fn try_expr(&mut self, body: &Expr, arms: &[Arm]) -> String {
        if !self.generator && !self.sync {
            return self.expr(body);
        }
        if self.kernel && !self.generator {
            return self.try_at_once(body, arms);
        }
        let body = self.gen_block(body);
        let error = self.temp();
        let (stmts, ()) = self.block(|g| g.handlers(&error, arms));
        let try_with = self.rt("tryWith");
        let tried = format!(
            "{try_with}({body}, {} {{\n{}}})",
            self.arrow(&error),
            indent(&stmts)
        );
        self.run(None, &tried)
    }
    pub(super) fn unwrap(&mut self, option: &Expr) -> String {
        let v = self.expr(option);
        let v = self.spill(v);
        self.buf
            .push(format!("if ({v}._tag === \"None\") return {v};"));
        format!("{v}.value")
    }
}
