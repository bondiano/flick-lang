//! Block traversal and local environment propagation for the bounded evaluator pass.

use flick_ir::{Expr, ExprKind, Pat, Stmt};

use super::{EvalAttempt, Hint, PassCtx};
use crate::opt::eval::machine::{bind_poly_fun, bind_rec_group};
use crate::opt::eval::value::{Env, rc_str, walk_pat};

impl<'a> PassCtx<'a, '_> {
    /// Walks a `Block(stmts, result)`, propagating known pure `let` and `Fun`
    /// bindings into subsequent statements via disjoint `split_first_mut` borrows.
    #[inline(never)]
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
                    shadow_group(group, &mut fn_env);
                    for stmt in group.iter_mut() {
                        if let Stmt::Fun { value, .. } = stmt {
                            self.visit_expr(value, &fn_env, false, depth + 1);
                        }
                    }
                    let group_ref: &[Stmt] = &*group;
                    if let Some(next_env) = self.bind_rec_group_in_env(group_ref, &env) {
                        env = next_env;
                    } else {
                        shadow_group(group_ref, &mut env);
                        all_concrete = false;
                    }
                    rest = tail;
                    continue;
                }

                let Some((first, tail)) = rest.split_first_mut() else {
                    break;
                };
                let stmt_hint = match first {
                    Stmt::Let { value: rhs, .. } | Stmt::Expr(rhs) => {
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
                    Stmt::Var { value: rhs, .. } | Stmt::Assign(_, rhs) => {
                        self.visit_expr(rhs, &env, false, depth + 1);
                        Hint::NonConcrete
                    }
                    Stmt::Fun { rec: true, .. } => Hint::NonConcrete,
                };
                if stmt_hint != Hint::Concrete {
                    all_concrete = false;
                }
                let first_ref: &Stmt = &*first;
                match first_ref {
                    Stmt::Let {
                        pat, value: rhs, ..
                    } => {
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
                        ..
                    } => {
                        let next = if !dicts.is_empty() {
                            self.bind_poly_fun_in_env(name, dicts, value, &env)
                        } else if stmt_hint == Hint::Concrete {
                            self.eval_for_env(value, &env)
                                .map(|val| env.with(rc_str(name), val))
                        } else {
                            None
                        };
                        if let Some(next_env) = next {
                            env = next_env;
                        } else {
                            env = env.shadow(rc_str(name));
                            all_concrete = false;
                        }
                    }
                    Stmt::Var { name, .. } => {
                        env = env.shadow(rc_str(name));
                    }
                    Stmt::Assign(..) | Stmt::Expr(_) | Stmt::Fun { rec: true, .. } => {}
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

    fn bind_poly_fun_in_env<'e>(
        &mut self,
        name: &str,
        dicts: &'e [String],
        value: &'e Expr,
        env: &Env<'e>,
    ) -> Option<Env<'e>>
    where
        'a: 'e,
    {
        let mut budget = self.new_trial_budget()?;
        let res = bind_poly_fun(name, dicts, value, env, &mut budget);
        *self.module_steps_remaining = budget.module_steps_remaining;
        res.ok()
    }

    fn bind_rec_group_in_env<'e>(&mut self, group: &'e [Stmt], env: &Env<'e>) -> Option<Env<'e>>
    where
        'a: 'e,
    {
        let mut budget = self.new_trial_budget()?;
        let res = bind_rec_group(group, env, &mut budget);
        *self.module_steps_remaining = budget.module_steps_remaining;
        res.ok()
    }
}

fn shadow_group(group: &[Stmt], env: &mut Env<'_>) {
    for stmt in group {
        if let Stmt::Fun { name, .. } = stmt {
            *env = env.shadow(rc_str(name));
        }
    }
}

pub(super) fn shadow_pat(pat: &Pat, env: &mut Env<'_>) {
    walk_pat(pat, &mut |p| {
        if let Pat::Bind(name) = p {
            *env = env.shadow(rc_str(name));
        }
    });
}
