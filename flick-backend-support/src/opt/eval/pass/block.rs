//! Block traversal, local environment propagation, and literal-data detection.

use std::rc::Rc;

use flick_ir::{Expr, ExprKind, Pat, Stmt};

use super::{EvalAttempt, Hint, PassCtx};
use crate::opt::eval::value::{Closure, DictEnv, Env, RecFun, RecGroup, Value};

impl<'a> PassCtx<'a, '_> {
    /// Walks a `Block(stmts, result)`, propagating known pure `let` and `Fun`
    /// bindings into subsequent statements via disjoint `split_first_mut` borrows.
    #[allow(
        clippy::too_many_lines,
        reason = "block statement traversal with lexical env propagation"
    )]
    pub(super) fn visit_block<'e>(
        &mut self,
        e: &'e mut Expr,
        outer_env: &Env<'e>,
        depth: u32,
    ) -> Hint
    where
        'a: 'e,
    {
        let mut all_concrete = true;
        {
            let ExprKind::Block(stmts, result) = &mut e.kind else {
                return Hint::NonConcrete;
            };
            let mut env = outer_env.clone();
            let mut rest: &mut [Stmt] = stmts.as_mut_slice();
            while !rest.is_empty() {
                if matches!(rest[0], Stmt::Fun { rec: true, .. }) {
                    let group_len = rest
                        .iter()
                        .take_while(|s| matches!(s, Stmt::Fun { rec: true, .. }))
                        .count();
                    let (group, tail) = rest.split_at_mut(group_len);
                    // Shadow every mutually recursive sibling before visiting any body.
                    let mut fn_env = env.clone();
                    for stmt in group.iter() {
                        if let Stmt::Fun { name, .. } = stmt {
                            fn_env = fn_env.shadow(Rc::from(name.as_str()));
                        }
                    }
                    for stmt in group.iter_mut() {
                        if let Stmt::Fun { value, .. } = stmt {
                            self.visit_expr(value, &fn_env, false, depth + 1);
                        }
                    }
                    let group_ref: &[Stmt] = &*group;
                    if let Some(next_env) = bind_rec_group(group_ref, &env) {
                        env = next_env;
                    } else {
                        for stmt in group_ref {
                            if let Stmt::Fun { name, .. } = stmt {
                                env = env.shadow(Rc::from(name.as_str()));
                            }
                        }
                        all_concrete = false;
                    }
                    rest = tail;
                    continue;
                }

                let Some((first, tail)) = rest.split_first_mut() else {
                    break;
                };
                let stmt_hint = match first {
                    Stmt::Let(_, rhs) | Stmt::Expr(rhs) => {
                        self.visit_expr(rhs, &env, false, depth + 1)
                    }
                    Stmt::Fun {
                        rec: false,
                        dicts,
                        value,
                        ..
                    } => {
                        let h = self.visit_expr(value, &env, false, depth + 1);
                        if dicts.is_empty() { h } else { Hint::Concrete }
                    }
                    Stmt::Var(_, rhs) | Stmt::Assign(_, rhs) => {
                        self.visit_expr(rhs, &env, false, depth + 1);
                        Hint::NonConcrete
                    }
                    Stmt::Fun { rec: true, .. } => unreachable!("handled above"),
                };
                if stmt_hint != Hint::Concrete {
                    all_concrete = false;
                }
                let first_ref: &Stmt = &*first;
                match first_ref {
                    Stmt::Let(pat, rhs) => {
                        if stmt_hint == Hint::Concrete {
                            if let Some(next_env) = self.bind_let_in_env(pat, rhs, &env) {
                                env = next_env;
                            } else {
                                shadow_pat(pat, &mut env);
                                all_concrete = false;
                            }
                        } else {
                            shadow_pat(pat, &mut env);
                        }
                    }
                    Stmt::Fun {
                        name,
                        rec: false,
                        dicts,
                        value,
                    } => {
                        if !dicts.is_empty() {
                            let closure = Value::Closure(Rc::new(Closure {
                                env: env.clone(),
                                dict_env: DictEnv::default(),
                                dict_params: dicts.as_slice(),
                                params: &[],
                                body: value,
                                rec_group: None,
                            }));
                            env = env.with(Rc::from(name.as_str()), closure);
                        } else if stmt_hint == Hint::Concrete
                            && let Some(val) = self.eval_for_env(value, &env)
                        {
                            env = env.with(Rc::from(name.as_str()), val);
                        } else {
                            env = env.shadow(Rc::from(name.as_str()));
                            all_concrete = false;
                        }
                    }
                    Stmt::Var(name, _) => {
                        env = env.shadow(Rc::from(name.as_str()));
                    }
                    Stmt::Assign(..) | Stmt::Expr(_) => {}
                    Stmt::Fun { rec: true, .. } => unreachable!("handled above"),
                }
                rest = tail;
            }
            if self.visit_expr(result, &env, false, depth + 1) != Hint::Concrete {
                all_concrete = false;
            }
        }
        if all_concrete {
            return match self.try_eval_and_replace(e, outer_env) {
                EvalAttempt::Replaced | EvalAttempt::KnownKept => Hint::Concrete,
                EvalAttempt::Stopped => Hint::NonConcrete,
            };
        }
        Hint::NonConcrete
    }
}

/// Iteratively checks whether `expr` is already a pure materialized data tree
/// (literals, constructors, tuples, lists, records, and constructor applications
/// with `arg_first: false`), so we never waste work re-evaluating already-folded data.
pub(super) fn is_literal_data(expr: &Expr) -> bool {
    let mut stack = vec![expr];
    let mut visited = 0_usize;
    while let Some(e) = stack.pop() {
        visited += 1;
        if visited > 128 {
            return false;
        }
        match &e.kind {
            ExprKind::Lit(_) | ExprKind::Con(_) => {}
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                for item in items {
                    stack.push(item);
                }
            }
            ExprKind::Record(fields) => {
                for (_, val) in fields {
                    stack.push(val);
                }
            }
            ExprKind::App { .. } => {
                let (head, args) = e.pure_spine();
                if !matches!(head.kind, ExprKind::Con(_)) || args.is_empty() {
                    return false;
                }
                for arg in args {
                    stack.push(arg);
                }
            }
            _ => return false,
        }
    }
    true
}

pub(super) fn shadow_pat(pat: &Pat, env: &mut Env<'_>) {
    let mut binders = Vec::new();
    pat.binders(&mut binders);
    for name in binders {
        *env = env.shadow(Rc::from(name));
    }
}

fn bind_rec_group<'e>(group: &'e [Stmt], env: &Env<'e>) -> Option<Env<'e>> {
    let mut defs = Vec::with_capacity(group.len());
    for stmt in group {
        let Stmt::Fun {
            name,
            rec: true,
            dicts,
            value,
        } = stmt
        else {
            return None;
        };
        let ExprKind::Lambda { params, body } = &value.kind else {
            return None;
        };
        defs.push(RecFun {
            name: Rc::from(name.as_str()),
            dict_params: dicts.as_slice(),
            params: params.as_slice(),
            body: body.as_ref(),
        });
    }
    let rec_group = Rc::new(RecGroup {
        env: env.clone(),
        dict_env: DictEnv::default(),
        defs: Rc::from(defs),
    });
    Some(env.with_rec_group(&rec_group))
}
