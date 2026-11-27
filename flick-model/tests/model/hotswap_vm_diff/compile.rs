//! Minimal explicit-ownership memory-IR compiler for the λFlick term subset
//! produced by `flick_model::hotswap::generate_session`.

use std::collections::{BTreeMap, BTreeSet};

use flick_core::ir::memory::{
    Block, CodeId, Exit, Function, Global, Instruction, Intrinsic, Local, Name, Operation, Program,
    ValueRep,
};
use flick_core::ir::{BinOp, Lit as IrLit};
use flick_core::numeric::{Arith, Num};
use flick_core::types::Type as CoreType;
use flick_model::hotswap::{MigrationBatch, ModuleDef};
use flick_model::syntax::{Const, Head, Lit, Pat, Prim, Term};
use flick_vm::session::{MigrationBatchRequest, StateMigration};

use super::types::{convert_type, path_to_name};

struct MiniCompiler {
    functions: BTreeMap<CodeId, Function>,
    next_code: CodeId,
}

impl MiniCompiler {
    fn new() -> Self {
        Self {
            functions: BTreeMap::new(),
            next_code: 0,
        }
    }

    fn alloc_code(&mut self) -> CodeId {
        let id = self.next_code;
        self.next_code += 1;
        id
    }

    fn compile_global_term(&mut self, term: &Term) -> CodeId {
        let mut instrs = Vec::new();
        let mut next_local: Local = 0;
        let mut env: BTreeMap<String, Local> = BTreeMap::new();
        let res = self.compile_expr(term, &mut env, &mut next_local, &mut instrs);
        let id = self.alloc_code();
        self.functions.insert(
            id,
            Function {
                params: Vec::new(),
                body: Block {
                    instructions: instrs,
                    exit: Exit::Return(res),
                },
            },
        );
        id
    }

    fn compile_ref_init(&mut self, init_term: &Term) -> CodeId {
        let mut instrs = Vec::new();
        let mut next_local: Local = 0;
        let mut env = BTreeMap::new();
        let val = self.compile_expr(init_term, &mut env, &mut next_local, &mut instrs);
        let cell = next_local;
        instrs.push(Instruction::Let {
            binding: cell,
            value: Operation::Intrinsic {
                kind: Intrinsic::RefMake,
                args: vec![val],
            },
        });
        let id = self.alloc_code();
        self.functions.insert(
            id,
            Function {
                params: Vec::new(),
                body: Block {
                    instructions: instrs,
                    exit: Exit::Return(cell),
                },
            },
        );
        id
    }

    fn free_vars_in(term: &Term, bound: &BTreeSet<String>, out: &mut BTreeSet<String>) {
        match term {
            Term::Var(v) => {
                if !bound.contains(v.as_ref()) {
                    out.insert(v.to_string());
                }
            }
            Term::Lam(p, body) => {
                let mut inner = bound.clone();
                inner.insert(p.to_string());
                Self::free_vars_in(body, &inner, out);
            }
            Term::App(f, a) => {
                Self::free_vars_in(f, bound, out);
                Self::free_vars_in(a, bound, out);
            }
            Term::Let(x, _, rhs, body) => {
                Self::free_vars_in(rhs, bound, out);
                let mut inner = bound.clone();
                inner.insert(x.to_string());
                Self::free_vars_in(body, &inner, out);
            }
            Term::LetRec(f, _, rhs, body) => {
                let mut inner = bound.clone();
                inner.insert(f.to_string());
                Self::free_vars_in(rhs, &inner, out);
                Self::free_vars_in(body, &inner, out);
            }
            Term::Tuple(es) => {
                for e in es.iter() {
                    Self::free_vars_in(e, bound, out);
                }
            }
            Term::Record(fs) => {
                for (_, e) in fs.iter() {
                    Self::free_vars_in(e, bound, out);
                }
            }
            Term::Match(scrut, arms) => {
                Self::free_vars_in(scrut, bound, out);
                for arm in arms.iter() {
                    let mut inner = bound.clone();
                    collect_pat_vars(&arm.pat, &mut inner);
                    Self::free_vars_in(&arm.body, &inner, out);
                }
            }
            Term::Apply(_, args) => {
                for arg in args.iter() {
                    Self::free_vars_in(arg, bound, out);
                }
            }
            Term::Lit(_) | Term::Global(_) | Term::Loc(_) | Term::Die(_) => {}
            other => panic!("unexpected term in free_vars_in: {other:?}"),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn compile_expr(
        &mut self,
        term: &Term,
        env: &mut BTreeMap<String, Local>,
        next_local: &mut Local,
        instrs: &mut Vec<Instruction>,
    ) -> Local {
        if let Term::App(f, rhs) = term
            && let Term::App(op, lhs) = f.as_ref()
            && matches!(
                op.as_ref(),
                Term::Apply(
                    Head::Const(Const::Prim(Prim::Arith(Arith::Add, Num::Int))),
                    args
                ) if args.is_empty()
            )
        {
            let l = self.compile_expr(lhs, env, next_local, instrs);
            let r = self.compile_expr(rhs, env, next_local, instrs);
            let dst = *next_local;
            *next_local += 1;
            instrs.push(Instruction::Let {
                binding: dst,
                value: Operation::Binary {
                    op: BinOp::Arith(Arith::Add, Num::Int),
                    left: l,
                    right: r,
                },
            });
            return dst;
        }
        if let Term::App(op, arg) = term
            && matches!(
                op.as_ref(),
                Term::Apply(Head::Const(Const::RefGet), args) if args.is_empty()
            )
        {
            let cell = self.compile_expr(arg, env, next_local, instrs);
            let dst = *next_local;
            *next_local += 1;
            instrs.push(Instruction::Let {
                binding: dst,
                value: Operation::Intrinsic {
                    kind: Intrinsic::RefGet,
                    args: vec![cell],
                },
            });
            return dst;
        }

        match term {
            Term::Lit(lit) => {
                let ir_lit = match lit {
                    Lit::Unit => IrLit::Unit,
                    Lit::Bool(b) => IrLit::Bool(*b),
                    Lit::Int(i) => IrLit::Int(*i),
                    Lit::String(s) => IrLit::String(s.to_string()),
                    other => panic!("unsupported literal in hot-swap diff test: {other:?}"),
                };
                let dst = *next_local;
                *next_local += 1;
                instrs.push(Instruction::Let {
                    binding: dst,
                    value: Operation::Literal(ir_lit),
                });
                dst
            }
            Term::Var(name) => {
                let slot = *env
                    .get(name.as_ref())
                    .unwrap_or_else(|| panic!("unbound var {name}"));
                instrs.push(Instruction::Dup(slot));
                slot
            }
            Term::Global(path) => {
                let dst = *next_local;
                *next_local += 1;
                instrs.push(Instruction::Let {
                    binding: dst,
                    value: Operation::Global(path_to_name(path)),
                });
                dst
            }
            Term::Loc(0) => {
                if let Some(&slot) = env.get("__old_loc0") {
                    instrs.push(Instruction::Dup(slot));
                    slot
                } else {
                    let dst = *next_local;
                    *next_local += 1;
                    instrs.push(Instruction::Let {
                        binding: dst,
                        value: Operation::Global(Name::new("Hot", "counter")),
                    });
                    dst
                }
            }
            Term::Die(msg) => {
                let s = *next_local;
                *next_local += 1;
                instrs.push(Instruction::Let {
                    binding: s,
                    value: Operation::Literal(IrLit::String(msg.to_string())),
                });
                let dst = *next_local;
                *next_local += 1;
                instrs.push(Instruction::Let {
                    binding: dst,
                    value: Operation::Intrinsic {
                        kind: Intrinsic::EffectDie,
                        args: vec![s],
                    },
                });
                dst
            }
            Term::Let(x, _, rhs, body) => {
                let rhs_local = self.compile_expr(rhs, env, next_local, instrs);
                let prev = env.insert(x.to_string(), rhs_local);
                let res = self.compile_expr(body, env, next_local, instrs);
                if let Some(old) = prev {
                    env.insert(x.to_string(), old);
                } else {
                    env.remove(x.as_ref());
                }
                instrs.push(Instruction::Drop(rhs_local));
                res
            }
            Term::LetRec(_, _, _, _) => {
                let loop_code = self.alloc_code();
                self.functions.insert(
                    loop_code,
                    Function {
                        params: Vec::new(),
                        body: Block {
                            instructions: vec![Instruction::Let {
                                binding: 0,
                                value: Operation::Literal(IrLit::Unit),
                            }],
                            exit: Exit::Return(0),
                        },
                    },
                );
                let dst = *next_local;
                *next_local += 1;
                instrs.push(Instruction::Let {
                    binding: dst,
                    value: Operation::Loop {
                        id: "spin".to_owned(),
                        code: loop_code,
                        args: Vec::new(),
                    },
                });
                dst
            }
            Term::Lam(param, body) => {
                let mut bound = BTreeSet::new();
                bound.insert(param.to_string());
                let mut fvs = BTreeSet::new();
                Self::free_vars_in(body, &bound, &mut fvs);
                let captured_names: Vec<String> = fvs.into_iter().collect();
                let mut capture_locals = Vec::with_capacity(captured_names.len());
                for name in &captured_names {
                    let slot = env[name];
                    instrs.push(Instruction::Dup(slot));
                    capture_locals.push(slot);
                }

                let mut fn_instrs = Vec::new();
                let mut fn_next: Local = 2;
                let mut fn_env = BTreeMap::new();
                fn_env.insert(param.to_string(), 1);
                let mut projected_locals = Vec::with_capacity(captured_names.len());
                for (field_idx, name) in captured_names.iter().enumerate() {
                    let loc = fn_next;
                    fn_next += 1;
                    fn_instrs.push(Instruction::Let {
                        binding: loc,
                        value: Operation::Project {
                            object: 0,
                            field: field_idx,
                            rep: ValueRep::Cell,
                        },
                    });
                    fn_env.insert(name.clone(), loc);
                    projected_locals.push(loc);
                }
                fn_instrs.push(Instruction::Drop(0));
                let ret = self.compile_expr(body, &mut fn_env, &mut fn_next, &mut fn_instrs);
                fn_instrs.push(Instruction::Drop(1));
                for loc in projected_locals {
                    fn_instrs.push(Instruction::Drop(loc));
                }
                let code_id = self.alloc_code();
                self.functions.insert(
                    code_id,
                    Function {
                        params: vec![0, 1],
                        body: Block {
                            instructions: fn_instrs,
                            exit: Exit::Return(ret),
                        },
                    },
                );
                let dst = *next_local;
                *next_local += 1;
                instrs.push(Instruction::Let {
                    binding: dst,
                    value: Operation::Closure {
                        code: code_id,
                        captures: capture_locals,
                    },
                });
                dst
            }
            Term::App(f, a) => {
                let fv = self.compile_expr(f, env, next_local, instrs);
                let av = self.compile_expr(a, env, next_local, instrs);
                let dst = *next_local;
                *next_local += 1;
                instrs.push(Instruction::Let {
                    binding: dst,
                    value: Operation::Apply {
                        function: fv,
                        argument: av,
                    },
                });
                dst
            }
            Term::Tuple(elems) => {
                let mut fields = Vec::with_capacity(elems.len());
                for e in elems.iter() {
                    fields.push(self.compile_expr(e, env, next_local, instrs));
                }
                let dst = *next_local;
                *next_local += 1;
                instrs.push(Instruction::Let {
                    binding: dst,
                    value: Operation::Construct {
                        tag: "$tuple".to_owned(),
                        fields,
                        rep: ValueRep::Cell,
                        layout: None,
                    },
                });
                dst
            }
            Term::Record(fields) => {
                let mut ir_fields = Vec::with_capacity(fields.len());
                for (name, expr) in fields.iter() {
                    let v = self.compile_expr(expr, env, next_local, instrs);
                    ir_fields.push((name.to_string(), v));
                }
                let dst = *next_local;
                *next_local += 1;
                instrs.push(Instruction::Let {
                    binding: dst,
                    value: Operation::Record { fields: ir_fields },
                });
                dst
            }
            Term::Match(scrut, arms) => {
                let [arm] = arms.as_ref() else {
                    panic!("expected single tuple-match arm in migration");
                };
                let Pat::Tuple(pats) = &arm.pat else {
                    panic!("expected tuple pattern in migration, got {:?}", arm.pat);
                };
                let obj = self.compile_expr(scrut, env, next_local, instrs);
                let mut bound_locals = Vec::new();
                for (idx, p) in pats.iter().enumerate() {
                    if let Pat::Var(v) = p {
                        let proj = *next_local;
                        *next_local += 1;
                        instrs.push(Instruction::Let {
                            binding: proj,
                            value: Operation::Project {
                                object: obj,
                                field: idx,
                                rep: ValueRep::Cell,
                            },
                        });
                        let prev = env.insert(v.to_string(), proj);
                        bound_locals.push((v.to_string(), proj, prev));
                    }
                }
                instrs.push(Instruction::Drop(obj));
                let res = self.compile_expr(&arm.body, env, next_local, instrs);
                for (name, loc, prev) in bound_locals {
                    if let Some(old) = prev {
                        env.insert(name, old);
                    } else {
                        env.remove(&name);
                    }
                    instrs.push(Instruction::Drop(loc));
                }
                res
            }
            other => panic!("unsupported model term in hot-swap diff test: {other:?}"),
        }
    }
}

fn collect_pat_vars(pat: &Pat, out: &mut BTreeSet<String>) {
    match pat {
        Pat::Var(v) => {
            out.insert(v.to_string());
        }
        Pat::Tuple(ps) => {
            for p in ps.iter() {
                collect_pat_vars(p, out);
            }
        }
        Pat::Wild | Pat::Lit(_) => {}
        other => panic!("unexpected pattern in hot-swap diff test: {other:?}"),
    }
}

pub(super) fn compile_module(module: &ModuleDef) -> Program {
    let mut c = MiniCompiler::new();
    let mut globals = Vec::new();
    for g in &module.globals {
        let init = c.compile_global_term(&g.value);
        globals.push(Global {
            name: path_to_name(&g.path),
            initializer: init,
        });
    }
    for cell in &module.state_cells {
        let init = c.compile_ref_init(&cell.init);
        globals.push(Global {
            name: path_to_name(&cell.path),
            initializer: init,
        });
    }
    Program {
        functions: c.functions,
        globals,
        op_spans: BTreeMap::new(),
    }
}

pub(super) fn compile_single_global(name: Name, term: &Term) -> Program {
    let mut c = MiniCompiler::new();
    let init = c.compile_global_term(term);
    Program {
        functions: c.functions,
        globals: vec![Global {
            name,
            initializer: init,
        }],
        op_spans: BTreeMap::new(),
    }
}

pub(super) fn compile_entry(entry: &Term) -> Program {
    let mut c = MiniCompiler::new();
    let mut instrs = Vec::new();
    let mut next_local: Local = 0;
    let mut env = BTreeMap::new();
    let loc0 = next_local;
    next_local += 1;
    instrs.push(Instruction::Let {
        binding: loc0,
        value: Operation::Global(Name::new("Hot", "counter")),
    });
    env.insert("__old_loc0".to_owned(), loc0);
    let res = c.compile_expr(entry, &mut env, &mut next_local, &mut instrs);
    instrs.push(Instruction::Drop(loc0));
    let init = c.alloc_code();
    c.functions.insert(
        init,
        Function {
            params: Vec::new(),
            body: Block {
                instructions: instrs,
                exit: Exit::Return(res),
            },
        },
    );
    Program {
        functions: c.functions,
        globals: vec![Global {
            name: Name::new("Entry", "main"),
            initializer: init,
        }],
        op_spans: BTreeMap::new(),
    }
}

pub(super) fn convert_migration_batch(batch: &MigrationBatch) -> MigrationBatchRequest {
    let migrations = batch
        .migrations
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let entry = Name::new("Mig", &format!("f{i}"));
            let migrate_program = compile_single_global(entry.clone(), &m.migrate_fn);
            let old_ty = convert_type(&m.old_ty);
            let new_ty = convert_type(&m.new_ty);
            let migrate_scheme = CoreType::Fun {
                param: Box::new(old_ty.clone()),
                effects: None,
                result: Box::new(new_ty.clone()),
            };
            StateMigration {
                cell: path_to_name(&m.cell),
                old_ty,
                new_ty,
                migrate_scheme: Some(migrate_scheme),
                migrate_entry: entry,
                migrate_program,
            }
        })
        .collect();
    MigrationBatchRequest::with_fuel(migrations, batch.fuel)
}
