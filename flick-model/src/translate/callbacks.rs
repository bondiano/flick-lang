//! Compiler-generated scope/provider callbacks preserve operand order and completion.
use super::Tr;
use crate::syntax::{Name, Term};
use flick_core::{
    ir::{self, ExprKind},
    source::Span,
};
use std::rc::Rc;

impl Tr {
    /// `Scope.run arg`, which is `scope s { body }`: the core's form of it.
    pub(super) fn scope(&mut self, arg: &ir::Expr) -> Term {
        self.scope_with(arg, &|tr, body| tr.expr(body))
    }

    /// [`Self::scope`] with the body as `body` translates it.
    pub(super) fn scope_with(&mut self, arg: &ir::Expr, body: Body) -> Term {
        let translate = body;
        if matches!(arg.kind, ExprKind::LoopExit { .. }) {
            return translate(self, arg);
        }
        let run = |arg: &ir::Expr| app(global("Scope", arg.span), arg.clone(), arg.span);
        let context = match &arg.kind {
            ExprKind::If(c, a, b) => {
                Some(ExprKind::If(c.clone(), Box::new(run(a)), Box::new(run(b))))
            }
            ExprKind::Block(stmts, tail) => {
                Some(ExprKind::Block(stmts.clone(), Box::new(run(tail))))
            }
            ExprKind::Match(value, arms) => Some(ExprKind::Match(
                value.clone(),
                arms.iter()
                    .map(|a| ir::Arm {
                        body: run(&a.body),
                        ..a.clone()
                    })
                    .collect(),
            )),
            _ => None,
        };
        if let Some(kind) = context {
            return translate(self, &ir::Expr::new(kind, arg.span));
        }
        if let ExprKind::Lambda { params, body } = &arg.kind
            && let [ir::Param { pat, .. }] = &params[..]
        {
            match pat {
                ir::Pat::Bind(s) => {
                    return Term::Scope(Name::from(s.as_str()), Rc::new(translate(self, body)));
                }
                ir::Pat::Wild => {
                    let s = self.fresh("s");
                    return Term::Scope(s, Rc::new(translate(self, body)));
                }
                _ => {}
            }
        }
        let name = self.fresh("callback");
        let s = self.fresh("s");
        let call = runtime_call(
            &name,
            ir::Expr::new(ExprKind::Local(s.to_string()), arg.span),
            arg.span,
        );
        let callback = lambda(ir::Pat::Bind(s.to_string()), call, arg.span);
        let run = app(global("Scope", arg.span), callback, arg.span);
        let block = binding(&name, arg, run);
        translate(self, &block)
    }

    /// `Layer.run layer arg`, which is `provide layer { body }`: the core's
    /// form of it.
    pub(super) fn provide(&mut self, func: &ir::Expr, arg: &ir::Expr) -> Term {
        let layer = self.expr(layer_run(func).expect("a call of `Layer.run`"));
        self.provide_value(layer, arg, &|tr, body| tr.expr(body))
    }

    pub(super) fn provide_value(&mut self, layer: Term, arg: &ir::Expr, body: Body) -> Term {
        let name = self.fresh("layer");
        let body = self.provided_value(&name, arg, body);
        Term::Let(name, None, Rc::new(layer), Rc::new(body))
    }

    fn provided_value(&mut self, layer: &Name, arg: &ir::Expr, body: Body) -> Term {
        let translate = body;
        let body = match &arg.kind {
            ExprKind::Lambda { params, body } if params.len() == 1 => {
                let body = translate(self, body);
                Term::app(self.param(&params[0].pat, body), Term::unit())
            }
            _ => {
                let name = self.fresh("callback");
                let call = runtime_call(
                    &name,
                    ir::Expr::new(ExprKind::Lit(ir::Lit::Unit), arg.span),
                    arg.span,
                );
                let callback = lambda(ir::Pat::Wild, call, arg.span);
                let layer = ir::Expr::new(ExprKind::Local(layer.to_string()), arg.span);
                let partial = app(global("Layer", arg.span), layer, arg.span);
                let run = app(partial, callback, arg.span);
                let block = binding(&name, arg, run);
                return translate(self, &block);
            }
        };
        Term::Provide(Rc::new(Term::Var(layer.clone())), Rc::new(body))
    }
}

/// How the body of a `scope` or a `provide` is translated.
pub(super) type Body<'b> = &'b dyn Fn(&mut Tr, &ir::Expr) -> Term;

fn binding(name: &Name, value: &ir::Expr, body: ir::Expr) -> ir::Expr {
    ir::Expr::new(
        ExprKind::Block(
            vec![ir::Stmt::Let(
                ir::Pat::Bind(name.to_string()),
                value.clone(),
            )],
            Box::new(body),
        ),
        value.span,
    )
}
fn lambda(pat: ir::Pat, body: ir::Expr, span: Span) -> ir::Expr {
    ir::Expr::new(
        ExprKind::Lambda {
            params: vec![ir::Param {
                pat,
                stage: ir::Stage::Runtime,
                ty: None,
            }],
            body: Box::new(body),
        },
        span,
    )
}
fn global(module: &str, span: Span) -> ir::Expr {
    ir::Expr::new(
        ExprKind::Global {
            module: module.into(),
            name: "run".into(),
        },
        span,
    )
}
fn app(func: ir::Expr, arg: ir::Expr, span: Span) -> ir::Expr {
    ir::Expr::new(
        ExprKind::App {
            func: Box::new(func),
            arg: Box::new(arg),
            stage: ir::Stage::Runtime,
            arg_first: false,
        },
        span,
    )
}

/// The layer of `Layer.run layer`.
pub(super) fn layer_run(func: &ir::Expr) -> Option<&ir::Expr> {
    let ExprKind::App { func, arg, .. } = &func.kind else {
        return None;
    };
    match &func.kind {
        ExprKind::Global { module, name } if module == "Layer" && name == "run" => Some(arg),
        _ => None,
    }
}

/// An already evaluated callback, called under the state/completion protocol.
fn runtime_call(name: &Name, arg: ir::Expr, span: Span) -> ir::Expr {
    app(
        ir::Expr::new(ExprKind::Local(name.to_string()), span),
        arg,
        span,
    )
}
