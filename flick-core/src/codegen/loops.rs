//! Backend-only completion flags. The shared IR retains typed lexical exits.
use crate::ir::{Arm, BinOp, Expr, ExprKind, Handler, Lit, Param, Pat, Stage, Stmt};
use crate::source::Span;
fn unit(span: Span) -> Expr {
    Expr::new(ExprKind::Lit(Lit::Unit), span)
}
fn boolean(b: bool, span: Span) -> Expr {
    Expr::new(ExprKind::Lit(Lit::Bool(b)), span)
}
fn read(name: &str, span: Span) -> Expr {
    Expr::new(ExprKind::Var(name.into()), span)
}
fn test(flag: &str, b: bool, span: Span) -> Expr {
    Expr::new(
        ExprKind::Binary(
            BinOp::Same,
            Box::new(read(flag, span)),
            Box::new(boolean(b, span)),
        ),
        span,
    )
}

struct Control {
    id: String,
    next: usize,
}
impl Control {
    fn hidden(&mut self, label: &str) -> String {
        self.next += 1;
        format!("{}${label}${}", self.id, self.next)
    }
    fn exits(&mut self, e: &mut Expr, flag: &str, result: &str) {
        if let ExprKind::LoopExit { id, value } = &e.kind {
            if id == flag {
                let span = e.span;
                let mut stmts = Vec::new();
                if let Some(value) = value {
                    let mut value = (**value).clone();
                    self.exits(&mut value, flag, result);
                    if Self::has_exit(&value, flag) {
                        let name = self.hidden("exitvalue");
                        let value = self.guarded_parts(value, flag);
                        let finish = Expr::new(
                            ExprKind::Block(
                                vec![
                                    Stmt::Assign(
                                        result.into(),
                                        Expr::new(ExprKind::Local(name.clone()), span),
                                    ),
                                    Stmt::Assign(flag.into(), boolean(false, span)),
                                    Stmt::Assign(format!("{flag}$step"), boolean(false, span)),
                                ],
                                Box::new(unit(span)),
                            ),
                            span,
                        );
                        *e = Expr::new(
                            ExprKind::Block(
                                vec![Stmt::Let(Pat::Bind(name), value)],
                                Box::new(Self::guard(finish, flag)),
                            ),
                            span,
                        );
                        return;
                    }
                    stmts.push(Stmt::Assign(result.into(), value));
                    stmts.push(Stmt::Assign(flag.into(), boolean(false, span)));
                }
                stmts.push(Stmt::Assign(format!("{flag}$step"), boolean(false, span)));
                *e = Expr::new(ExprKind::Block(stmts, Box::new(unit(span))), span);
            }
            return;
        }
        if matches!(e.kind, ExprKind::ControlLoop { .. }) {
            return;
        }
        e.each_child_mut(&mut |c| self.exits(c, flag, result));
    }
    fn guard(e: Expr, flag: &str) -> Expr {
        let span = e.span;
        let condition = Expr::new(
            ExprKind::Binary(
                BinOp::And,
                Box::new(test(flag, true, span)),
                Box::new(test(&format!("{flag}$step"), true, span)),
            ),
            span,
        );
        Expr::new(
            ExprKind::If(Box::new(condition), Box::new(e), Box::new(unit(span))),
            span,
        )
    }
    fn has_exit(e: &Expr, flag: &str) -> bool {
        if let ExprKind::Block(stmts, _) = &e.kind
            && stmts
                .iter()
                .any(|s| matches!(s, Stmt::Assign(n, _) if n == &format!("{flag}$step")))
        {
            return true;
        }
        let mut found = false;
        e.each_child(&mut |c| found |= Self::has_exit(c, flag));
        found
    }
    fn guarded_block(&mut self, stmts: &[Stmt], tail: &Expr, flag: &str, span: Span) -> Expr {
        let mut rest = self.guarded_parts(tail.clone(), flag);
        for mut stmt in stmts.iter().cloned().rev() {
            let v = match &mut stmt {
                Stmt::Let(_, v) | Stmt::Var(_, v) | Stmt::Assign(_, v) | Stmt::Expr(v) => Some(v),
                Stmt::Fun { .. } => None,
            };
            if let Some(v) = v {
                let name = self.hidden("$loop$operand");
                let value = self.guarded_parts(v.clone(), flag);
                *v = Expr::new(ExprKind::Local(name.clone()), span);
                let suffix = Expr::new(ExprKind::Block(vec![stmt], Box::new(rest)), span);
                rest = Expr::new(
                    ExprKind::Block(
                        vec![Stmt::Let(Pat::Bind(name), value)],
                        Box::new(Self::guard(suffix, flag)),
                    ),
                    span,
                );
            } else {
                rest = Expr::new(ExprKind::Block(vec![stmt], Box::new(rest)), span);
            }
        }
        rest
    }
    fn guarded_match(&mut self, mut e: Expr, flag: &str) -> Expr {
        let span = e.span;
        let ExprKind::Match(scrutinee, arms) = &mut e.kind else {
            unreachable!("match")
        };
        let name = self.hidden("$loop$scrutinee");
        let value = self.guarded_parts((**scrutinee).clone(), flag);
        **scrutinee = Expr::new(ExprKind::Local(name.clone()), span);
        self.guarded_arms(arms, flag, span);
        Expr::new(
            ExprKind::Block(
                vec![Stmt::Let(Pat::Bind(name), value)],
                Box::new(Self::guard(e, flag)),
            ),
            span,
        )
    }

    fn guarded_arms(&mut self, arms: &mut [Arm], flag: &str, span: Span) {
        for arm in arms {
            if let Some(g) = &mut arm.guard {
                let name = self.hidden("guard");
                let value = self.guarded_parts(g.clone(), flag);
                let active = Self::guard(Expr::new(ExprKind::Local(name.clone()), span), flag);
                let ExprKind::If(c, yes, _) = active.kind else {
                    unreachable!("guard")
                };
                let selected = Expr::new(ExprKind::If(c, yes, Box::new(boolean(true, span))), span);
                *g = Expr::new(
                    ExprKind::Block(vec![Stmt::Let(Pat::Bind(name), value)], Box::new(selected)),
                    span,
                );
            }
            let body = self.guarded_parts(arm.body.clone(), flag);
            arm.body = Self::guard(body, flag);
        }
    }

    fn guarded_handler(&mut self, h: &mut Handler, flag: &str, span: Span) {
        h.body = self.guarded_parts(h.body.clone(), flag);
        self.guarded_arms(&mut h.errors, flag, span);
        if let Some(ret) = &mut h.ret {
            if let ExprKind::Lambda { body, .. } = &mut ret.kind {
                **body = Self::guard(self.guarded_parts((**body).clone(), flag), flag);
            } else {
                let name = self.hidden("return");
                let call = Expr::new(
                    ExprKind::App {
                        func: Box::new(ret.clone()),
                        arg: Box::new(Expr::new(ExprKind::Local(name.clone()), span)),
                        stage: Stage::Runtime,
                        arg_first: false,
                    },
                    span,
                );
                *ret = Expr::new(
                    ExprKind::Lambda {
                        params: vec![Param {
                            pat: Pat::Bind(name),
                            stage: Stage::Runtime,
                            ty: None,
                        }],
                        body: Box::new(Self::guard(call, flag)),
                    },
                    span,
                );
            }
        }
    }

    fn guarded_parts(&mut self, mut e: Expr, flag: &str) -> Expr {
        if !Self::has_exit(&e, flag) {
            return e;
        }
        let span = e.span;
        match &mut e.kind {
            ExprKind::Block(stmts, tail) => {
                // An exit commits its result and both flags atomically.
                if stmts
                    .iter()
                    .any(|s| matches!(s, Stmt::Assign(n, _) if n == &format!("{flag}$step")))
                {
                    return e;
                }
                self.guarded_block(stmts, tail, flag, span)
            }
            ExprKind::Lambda { body, .. } | ExprKind::Task(body) => {
                **body = self.guarded_parts((**body).clone(), flag);
                e
            }
            ExprKind::App { func, arg, .. } if matches!(&func.kind, ExprKind::Global { module, name } if module == "Scope" && name == "run") =>
            {
                **arg = self.guarded_parts((**arg).clone(), flag);
                Self::guard(e, flag)
            }
            ExprKind::Handle(h) => {
                self.guarded_handler(h, flag, span);
                e
            }
            ExprKind::If(c, t, f) => {
                let name = self.hidden("$loop$condition");
                let c = self.guarded_parts((**c).clone(), flag);
                let t = self.guarded_parts((**t).clone(), flag);
                let f = self.guarded_parts((**f).clone(), flag);
                let branch = Expr::new(
                    ExprKind::If(
                        Box::new(Expr::new(ExprKind::Local(name.clone()), span)),
                        Box::new(t),
                        Box::new(f),
                    ),
                    span,
                );
                Expr::new(
                    ExprKind::Block(
                        vec![Stmt::Let(Pat::Bind(name), c)],
                        Box::new(Self::guard(branch, flag)),
                    ),
                    span,
                )
            }
            ExprKind::Match(..) => self.guarded_match(e, flag),
            ExprKind::Binary(op @ (BinOp::And | BinOp::Or), c, r) => {
                let and = *op == BinOp::And;
                let (yes, no) = if and {
                    ((**r).clone(), boolean(false, span))
                } else {
                    (boolean(true, span), (**r).clone())
                };
                self.guarded_parts(
                    Expr::new(ExprKind::If(c.clone(), Box::new(yes), Box::new(no)), span),
                    flag,
                )
            }
            ExprKind::Loop { .. } | ExprKind::Each { .. } | ExprKind::Generate { .. } => e,
            _ => {
                let mut values = Vec::new();
                e.each_child_mut(&mut |c| {
                    let name = self.hidden("$loop$operand");
                    values.push((name.clone(), self.guarded_parts(c.clone(), flag)));
                    *c = Expr::new(ExprKind::Local(name), c.span);
                });
                let mut rest = e;
                for (name, value) in values.into_iter().rev() {
                    rest = Expr::new(
                        ExprKind::Block(
                            vec![Stmt::Let(Pat::Bind(name), value)],
                            Box::new(Self::guard(rest, flag)),
                        ),
                        span,
                    );
                }
                rest
            }
        }
    }
    pub(super) fn control_each(&mut self, e: &mut Expr, flag: &str) {
        let span = e.span;
        if let ExprKind::Each {
            source, stop, body, ..
        } = &mut e.kind
        {
            let name = self.hidden("source");
            let value = self.guarded_parts((**source).clone(), flag);
            **source = Expr::new(ExprKind::Local(name.clone()), span);
            *stop = Some(Box::new(test(flag, false, span)));
            self.control_each(body, flag);
            let inner = (**body).clone();
            **body = Expr::new(
                ExprKind::Block(
                    vec![Stmt::Assign(format!("{flag}$step"), boolean(true, span))],
                    Box::new(inner),
                ),
                span,
            );
            // A source can itself break or continue. Do not enumerate its
            // absent value; an outer clause may still advance after continue.
            let guarded = Self::guard(e.clone(), flag);
            *e = Expr::new(
                ExprKind::Block(vec![Stmt::Let(Pat::Bind(name), value)], Box::new(guarded)),
                span,
            );
        } else {
            fn has_each(e: &Expr) -> bool {
                if matches!(e.kind, ExprKind::Each { .. }) {
                    return true;
                }
                if matches!(
                    e.kind,
                    ExprKind::Lambda { .. }
                        | ExprKind::Generate { .. }
                        | ExprKind::ControlLoop { .. }
                        | ExprKind::Loop { .. }
                ) {
                    return false;
                }
                let mut found = false;
                e.each_child(&mut |c| found |= has_each(c));
                found
            }
            if has_each(e) {
                e.each_child_mut(&mut |c| self.control_each(c, flag));
                *e = self.guarded_parts(e.clone(), flag);
            } else {
                *e = self.guarded_parts(e.clone(), flag);
            }
        }
    }
}

pub(super) fn lower(
    id: &str,
    condition: Option<&Expr>,
    body: &Expr,
    iterable: bool,
    span: Span,
) -> Expr {
    let mut c = Control {
        id: id.into(),
        next: 0,
    };
    let flag = id;
    let result = format!("{id}$result");
    let mut body = body.clone();
    c.exits(&mut body, flag, &result);
    let body = if iterable {
        c.control_each(&mut body, flag);
        body
    } else {
        let mut condition = condition.cloned().unwrap_or_else(|| boolean(true, span));
        c.exits(&mut condition, flag, &result);
        let condition = c.guarded_parts(condition, flag);
        let body = c.guarded_parts(body, flag);
        let name = c.hidden("test");
        let stop = Expr::new(
            ExprKind::Block(
                vec![Stmt::Assign(flag.into(), boolean(false, span))],
                Box::new(unit(span)),
            ),
            span,
        );
        let branch = Expr::new(
            ExprKind::If(
                Box::new(Expr::new(ExprKind::Local(name.clone()), span)),
                Box::new(body),
                Box::new(stop),
            ),
            span,
        );
        let branch = Control::guard(branch, flag);
        let body = Expr::new(
            ExprKind::Block(
                vec![
                    Stmt::Assign(format!("{flag}$step"), boolean(true, span)),
                    Stmt::Let(Pat::Bind(name), condition),
                ],
                Box::new(branch),
            ),
            span,
        );
        Expr::new(
            ExprKind::Loop {
                condition: Box::new(read(flag, span)),
                body: Box::new(body),
            },
            span,
        )
    };
    Expr::new(
        ExprKind::Block(
            vec![
                Stmt::Var(flag.into(), boolean(true, span)),
                Stmt::Var(result.clone(), unit(span)),
                Stmt::Var(format!("{flag}$step"), boolean(true, span)),
                Stmt::Expr(body),
            ],
            Box::new(read(&result, span)),
        ),
        span,
    )
}
