//! Binder-aware captures, including local references inside dictionaries.

use std::collections::BTreeSet;

use flick_ir::Dict;

use crate::ir::{Arm, Expr, ExprKind, Pat, Stmt};

pub(super) fn given(name: &str) -> String {
    format!("?{name}")
}

pub(super) fn free(expr: &Expr) -> BTreeSet<String> {
    let mut output = BTreeSet::new();
    collect(expr, &BTreeSet::new(), &mut output);
    output
}

fn local(name: &str, bound: &BTreeSet<String>, output: &mut BTreeSet<String>) {
    if !bound.contains(name) {
        output.insert(name.to_owned());
    }
}

fn bind(pattern: &Pat, bound: &mut BTreeSet<String>) {
    let mut names = Vec::new();
    pattern.binders(&mut names);
    bound.extend(names.into_iter().map(str::to_owned));
}

fn dict(value: &Dict, bound: &BTreeSet<String>, output: &mut BTreeSet<String>) {
    match value {
        Dict::Given(name) => local(&given(name), bound, output),
        Dict::Field(name) => local(name, bound, output),
        Dict::Opened { binder, .. } => local(binder, bound, output),
        Dict::Instance { args, .. } | Dict::Structural { fields: args, .. } => {
            for argument in args {
                dict(argument, bound, output);
            }
        }
        Dict::Super { dict: inner, .. } => dict(inner, bound, output),
        Dict::Value { .. } | Dict::Primitive { .. } | Dict::Site { .. } | Dict::Error => {}
    }
}

fn arm(value: &Arm, bound: &BTreeSet<String>, output: &mut BTreeSet<String>) {
    let mut inside = bound.clone();
    bind(&value.pat, &mut inside);
    if let Some(guard) = &value.guard {
        collect(guard, &inside, output);
    }
    collect(&value.body, &inside, output);
}

fn collect(expr: &Expr, bound: &BTreeSet<String>, output: &mut BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Local(name) | ExprKind::Var(name) => local(name, bound, output),
        ExprKind::Method { dict: value, .. } => dict(value, bound, output),
        ExprKind::Within(owner, call) => {
            local(owner, bound, output);
            collect(call, bound, output);
        }
        ExprKind::Dicts(inner, values) => {
            collect(inner, bound, output);
            for value in values {
                dict(value, bound, output);
            }
        }
        ExprKind::Abstract(names, body) => {
            let mut bound = bound.clone();
            bound.extend(names.iter().map(|name| given(name)));
            collect(body, &bound, output);
        }
        ExprKind::Lambda { params, body } => {
            let mut bound = bound.clone();
            for parameter in params {
                bind(&parameter.pat, &mut bound);
            }
            collect(body, &bound, output);
        }
        ExprKind::Block(statements, result) => {
            let mut bound = bound.clone();
            for statement in statements {
                match statement {
                    Stmt::Let(pattern, value) => {
                        collect(value, &bound, output);
                        bind(pattern, &mut bound);
                    }
                    Stmt::Fun {
                        name,
                        rec,
                        dicts,
                        value,
                    } => {
                        let mut inside = bound.clone();
                        if *rec {
                            inside.insert(name.clone());
                        }
                        inside.extend(dicts.iter().map(|name| given(name)));
                        collect(value, &inside, output);
                        bound.insert(name.clone());
                    }
                    Stmt::Var(name, value) => {
                        collect(value, &bound, output);
                        bound.insert(name.clone());
                    }
                    Stmt::Assign(name, value) => {
                        local(name, &bound, output);
                        collect(value, &bound, output);
                    }
                    Stmt::Expr(value) => collect(value, &bound, output),
                }
            }
            collect(result, &bound, output);
        }
        ExprKind::Match(value, arms) => {
            collect(value, bound, output);
            for value in arms {
                arm(value, bound, output);
            }
        }
        ExprKind::Handle(handler) => {
            for handled in &handler.effects {
                for operation in &handled.operations {
                    collect(&operation.function, bound, output);
                }
            }
            collect(&handler.body, bound, output);
            if let Some(returned) = &handler.ret {
                collect(returned, bound, output);
            }
            for value in &handler.errors {
                arm(value, bound, output);
            }
        }
        ExprKind::Each {
            stop,
            pat,
            source,
            body,
        } => {
            if let Some(stop) = stop {
                collect(stop, bound, output);
            }
            collect(source, bound, output);
            let mut inside = bound.clone();
            bind(pat, &mut inside);
            collect(body, &inside, output);
        }
        // Forms not lowered yet still receive a real binding-aware walk;
        // lowering rejects their semantics explicitly instead of guessing.
        _ => expr.each_child(&mut |child| collect(child, bound, output)),
    }
}

#[cfg(test)]
mod tests {
    use super::free;
    use crate::ir::{Arm, Expr, ExprKind, Handler, Lit, Pat};
    use crate::source::{FileId, Span};

    fn expr(kind: ExprKind) -> Expr {
        Expr::new(kind, Span::new(FileId::new(0), 0, 0))
    }

    fn local(name: &str) -> Expr {
        expr(ExprKind::Local(name.into()))
    }

    #[test]
    fn handler_error_binders_are_local_to_their_guard_and_body() {
        let value = expr(ExprKind::Handle(Box::new(Handler {
            effects: vec![],
            body: local("body"),
            ret: Some(local("returned")),
            errors: vec![
                Arm {
                    pat: Pat::Bind("error".into()),
                    guard: Some(local("error")),
                    body: expr(ExprKind::Tuple(vec![local("error"), local("captured")])),
                },
                Arm {
                    pat: Pat::Wild,
                    guard: Some(local("error")),
                    body: expr(ExprKind::Lit(Lit::Unit)),
                },
            ],
        })));
        assert_eq!(
            free(&value).into_iter().collect::<Vec<_>>(),
            ["body", "captured", "error", "returned"]
        );
        let ExprKind::Handle(mut handler) = value.kind else {
            unreachable!()
        };
        handler.errors.pop();
        assert_eq!(
            free(&expr(ExprKind::Handle(handler)))
                .into_iter()
                .collect::<Vec<_>>(),
            ["body", "captured", "returned"]
        );
    }

    #[test]
    fn each_stop_is_outside_its_element_pattern_scope() {
        let value = expr(ExprKind::Each {
            stop: Some(Box::new(local("element"))),
            pat: Pat::Bind("element".into()),
            source: Box::new(local("source")),
            body: Box::new(local("element")),
        });
        assert_eq!(
            free(&value).into_iter().collect::<Vec<_>>(),
            ["element", "source"]
        );
    }
}
