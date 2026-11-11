//! Non-tail handlers carry lexical completion through their body and resume.
//! Private cells retain state even if an operation discards its continuation.
use std::rc::Rc;

use super::{
    Tr,
    expr::stuck,
    loops::{effectful, result, rpat},
};
use crate::{
    handler::{Handled, Handler, Op},
    syntax::{Arm, Pat, Path, Term},
};
use flick_core::{
    ir::{self, ExprKind},
    types::OpClass,
};

impl Tr {
    pub(super) fn protected_resumption(&mut self, e: &ir::Expr, id: &str, xs: &[String]) -> Term {
        let completion = self.loop_result(e, id);
        let name = self.fresh("completion");
        let body = Term::Let(
            name.clone(),
            None,
            Rc::new(completion),
            Rc::new(self.pack(result(true, Term::Var(name)), xs)),
        );
        self.catch_failure(body, xs, true)
    }
    pub(super) fn iteration_effectful(&self, e: &ir::Expr) -> bool {
        effectful(e) || (!self.cells.is_empty() && self.cell_step(e))
    }
    fn cell_step(&self, e: &ir::Expr) -> bool {
        if matches!(
            e.kind,
            ExprKind::Lambda { .. } | ExprKind::Abstract(..) | ExprKind::Task(_)
        ) {
            return false;
        }
        if matches!(&e.kind, ExprKind::Var(x) if self.cell(x).is_some()) {
            return true;
        }
        if let ExprKind::Block(stmts, _) = &e.kind
            && stmts
                .iter()
                .any(|s| matches!(s, ir::Stmt::Assign(x, _) if self.cell(x).is_some()))
        {
            return true;
        }
        let mut found = false;
        e.each_child(&mut |c| found |= self.cell_step(c));
        found
    }
    /// With state in cells, a handler transports only its completion value.
    fn loop_result(&mut self, e: &ir::Expr, id: &str) -> Term {
        let value = self.loop_value(e, id, &[]);
        self.only_value(value)
    }
    fn only_value(&mut self, value: Term) -> Term {
        let v = self.fresh("completion");
        Term::Match(
            Rc::new(value),
            Rc::from([Arm {
                pat: Pat::Tuple(Rc::from([Pat::Var(v.clone())])),
                guard: None,
                body: Term::Var(v),
            }]),
        )
    }
    pub(super) fn loop_cell_handler(
        &mut self,
        h: &ir::Handler,
        id: &str,
        xs: &[String],
        protected: bool,
    ) -> Term {
        self.with_cells(|tr| {
            let cast = (!h.errors.is_empty() && tr.error_action.is_none())
                .then(|| tr.fresh("error_action"));
            if let Some(cast) = &cast {
                tr.error_action = Some(cast.clone());
            }
            let effects = h
                .effects
                .iter()
                .map(|e| Handled {
                    effect: Path::parse(&e.effect),
                    ops: e
                        .operations
                        .iter()
                        .map(|op| Op {
                            name: op.name.as_str().into(),
                            class: op.class,
                            dicts: op.dicts,
                            arity: op.arity,
                            covers: op.covers,
                            function: if op.class == OpClass::Tail {
                                tr.expr(&op.function)
                            } else {
                                tr.loop_operation(&op.function, op.class, id)
                            },
                        })
                        .collect(),
                })
                .collect();
            let body = tr.loop_result(&h.body, id);
            let ret = h.ret.as_ref().map(|r| tr.completion_return(r, id));
            let errors = tr.completion_errors(&h.errors, id);
            let value = Term::Handler(Rc::new(Handler {
                effects,
                body,
                ret,
                errors,
            }));
            let v = tr.fresh("completion");
            let completion = if protected {
                result(true, Term::Var(v.clone()))
            } else {
                Term::Var(v.clone())
            };
            let body = Term::Let(v, None, Rc::new(value), Rc::new(tr.pack(completion, xs)));
            let body = if protected {
                tr.catch_failure(body, xs, true)
            } else {
                body
            };
            if let Some(cast) = cast {
                tr.error_action = None;
                tr.error_actions(cast, body)
            } else {
                body
            }
        })
    }
    /// An operation arm has no source-level access to the outer variables.
    /// The generated completion protocol reads their private cells only.
    fn loop_operation(&mut self, e: &ir::Expr, class: OpClass, id: &str) -> Term {
        match &e.kind {
            ExprKind::Abstract(keys, body) => {
                let body = self.loop_operation(body, class, id);
                super::lams(keys.iter().map(|x| super::given(x)), body)
            }
            ExprKind::Lambda { params, body } => {
                let mark = self.resumptions.len();
                if class == OpClass::Control
                    && let Some(p) = params.last()
                    && let ir::Pat::Bind(name) = &p.pat
                {
                    self.resumptions.push((name.clone(), id.into()));
                }
                let normalized = self.normalize_resumptions(body, id);
                let mut body = self.loop_result(&normalized, id);
                self.resumptions.truncate(mark);
                for p in params.iter().rev() {
                    body = self.param(&p.pat, body);
                }
                body
            }
            _ => stuck("a non-lambda control operation"),
        }
    }
    fn completion_return(&mut self, e: &ir::Expr, id: &str) -> Term {
        let ExprKind::Lambda { params, body } = &e.kind else {
            return stuck("a non-lambda handler return callback");
        };
        let [p] = &params[..] else {
            return stuck("a handler return callback with multiple parameters");
        };
        let arg = self.fresh("completion");
        let v = self.fresh("value");
        let exit = self.fresh("exit");
        let body = self.loop_result(body, id);
        let callback = self.param(&p.pat, body);
        Term::lam(
            &arg,
            Term::Match(
                Rc::new(Term::Var(arg.clone())),
                Rc::from([
                    Arm {
                        pat: rpat(true, Pat::Var(v.clone())),
                        guard: None,
                        body: Term::app(callback, Term::Var(v)),
                    },
                    Arm {
                        pat: rpat(false, Pat::Var(exit.clone())),
                        guard: None,
                        body: result(false, Term::Var(exit)),
                    },
                ]),
            ),
        )
    }
    fn completion_errors(&mut self, arms: &[ir::Arm], id: &str) -> Rc<[Arm]> {
        if arms.is_empty() {
            return Rc::from([]);
        }
        let saved = self.control.replace(id.into());
        let error = self.fresh("error");
        let action = Term::Task(Rc::new(Term::Raise(Rc::new(Term::Var(error.clone())))));
        let action = self.cast_error_action(action);
        let value = self.handler_state(action, arms, &[], false);
        let body = self.only_value(value);
        self.control = saved;
        Rc::from([Arm {
            pat: Pat::Var(error),
            guard: None,
            body,
        }])
    }
}
