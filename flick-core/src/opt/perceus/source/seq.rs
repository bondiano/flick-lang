//! Audited `Seq.*` lazy-sequence and effectful-iteration adapters.

use flick_ir::{Arith, Constructor, Foreign, ForeignMode, Num, Rep};

use super::lazy_scope::has_standard_shape;
use super::{
    Arm, BinOp, Code, Env, Expr, ExprKind, Intrinsic, Lit, Local, Lower, LowerError, Operation,
    Param, Pat, Span, SpannedBlock, SpannedFunction, Stage, Stmt, ValueRep,
};

impl Lower<'_> {
    pub(super) fn seq_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.binding.from() != Some("flick:rt")
            || value.dicts != 0
            || !has_standard_shape(value)
        {
            return None;
        }
        let name = value.binding.export_name().unwrap_or("");
        match (name, value.mode, value.arity) {
            ("seqOfList", ForeignMode::Pure, 1) => {
                Some(self.intrinsic(Intrinsic::SeqOfList, false, vec![], block, span))
            }
            ("seqToList", ForeignMode::Pure, 1) => {
                Some(self.intrinsic(Intrinsic::SeqToList, false, vec![], block, span))
            }
            ("seqZip", ForeignMode::Pure, 2) => Some(self.seq_zip(false, block, span)),
            ("seqZipWith", ForeignMode::Pure, 3) => Some(self.seq_zip(true, block, span)),
            _ => {
                let expr = seq_expression(name, value.mode, value.arity, span)?;
                Some(self.expr(&expr, &Env::new(), block))
            }
        }
    }

    /// Direct memory-IR lowering for `Seq.zip` / `Seq.zip_with` opening two iterators in lockstep.
    fn seq_zip(
        &mut self,
        with_fn: bool,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let loop_id = self.fresh_loop_id();
        let loop_fn = Self::seq_zip_loop(&loop_id, with_fn, span)?;
        let loop_code = self.reserve(span)?;
        self.finish(loop_code, &loop_fn)?;

        // Generator body: params [0: self_seq, 1: unit]; captures = if with_fn { [f, a, b] } else { [a, b] }.
        let gen_code = self.reserve(span)?;
        let mut gen_fn = Code::new(vec![0, 1]);
        let loop_args = if with_fn {
            let f = gen_fn.emit(
                Operation::Project {
                    object: 0,
                    field: 0,
                    rep: ValueRep::Cell,
                },
                span,
            )?;
            let a = gen_fn.emit(
                Operation::Project {
                    object: 0,
                    field: 1,
                    rep: ValueRep::Cell,
                },
                span,
            )?;
            let b = gen_fn.emit(
                Operation::Project {
                    object: 0,
                    field: 2,
                    rep: ValueRep::Cell,
                },
                span,
            )?;
            let ia = gen_fn.emit(Operation::SeqOpen(a), span)?;
            let ib = gen_fn.emit(Operation::SeqOpen(b), span)?;
            vec![ia, ib, f]
        } else {
            let a = gen_fn.emit(
                Operation::Project {
                    object: 0,
                    field: 0,
                    rep: ValueRep::Cell,
                },
                span,
            )?;
            let b = gen_fn.emit(
                Operation::Project {
                    object: 0,
                    field: 1,
                    rep: ValueRep::Cell,
                },
                span,
            )?;
            let ia = gen_fn.emit(Operation::SeqOpen(a), span)?;
            let ib = gen_fn.emit(Operation::SeqOpen(b), span)?;
            vec![ia, ib]
        };
        let res = gen_fn.emit(
            Operation::Loop {
                id: loop_id,
                code: loop_code,
                args: loop_args,
            },
            span,
        )?;
        self.finish(gen_code, &gen_fn.finish(res))?;

        let arity = if with_fn { 3 } else { 2 };
        self.curried_generate_lazy(gen_code, arity, vec![], block, span)
    }

    fn seq_zip_next(
        code: &mut Code,
        iter: Local,
        bind: &str,
        span: Span,
    ) -> Result<(Local, Local), LowerError> {
        let next = code.emit(Operation::SeqNext(iter), span)?;
        let pattern = Pat::Con(
            Constructor {
                module: "Prelude".into(),
                ty: "Option".into(),
                name: "Some".into(),
                arity: 1,
                dicts: 0,
                rep: Rep::Tagged,
                fields: Vec::new(),
                type_args: Vec::new(),
            },
            vec![Pat::Bind(bind.into())],
        );
        let matched = code.emit(
            Operation::Match {
                object: next,
                pattern: Box::new(pattern),
                rep: ValueRep::Cell,
            },
            span,
        )?;
        let has_val = code.emit(
            Operation::Project {
                object: matched,
                field: 0,
                rep: ValueRep::Cell,
            },
            span,
        )?;
        Ok((matched, has_val))
    }

    fn seq_zip_loop(
        loop_id: &str,
        with_fn: bool,
        span: Span,
    ) -> Result<SpannedFunction, LowerError> {
        let exit_loop = |mut branch: Code| -> Result<SpannedBlock, LowerError> {
            let unit = branch.emit(Operation::Literal(Lit::Unit), span)?;
            let stop = branch.emit(
                Operation::LoopExit {
                    id: loop_id.to_owned(),
                    value: Some(unit),
                },
                span,
            )?;
            Ok(branch.finish(stop).body)
        };

        let loop_params = if with_fn { vec![0, 1, 2] } else { vec![0, 1] };
        let mut step = Code::new(loop_params.clone());
        let (ma, has_a) = Self::seq_zip_next(&mut step, 0, "$zip.a", span)?;

        let mut branch_a = step.successor();
        let val_a = branch_a.emit(
            Operation::Project {
                object: ma,
                field: 1,
                rep: ValueRep::Cell,
            },
            span,
        )?;
        let (mb, has_b) = Self::seq_zip_next(&mut branch_a, 1, "$zip.b", span)?;

        let mut on_both = branch_a.successor();
        let val_b = on_both.emit(
            Operation::Project {
                object: mb,
                field: 1,
                rep: ValueRep::Cell,
            },
            span,
        )?;
        let item = if with_fn {
            let f_a = on_both.emit(
                Operation::Apply {
                    function: 2,
                    argument: val_a,
                },
                span,
            )?;
            on_both.emit(
                Operation::Apply {
                    function: f_a,
                    argument: val_b,
                },
                span,
            )?
        } else {
            on_both.emit(
                Operation::Construct {
                    tag: "$tuple".into(),
                    fields: vec![val_a, val_b],
                    rep: ValueRep::Cell,
                    layout: None,
                },
                span,
            )?
        };
        let yielded = on_both.emit(Operation::Yield(item), span)?;
        let exit_b = exit_loop(branch_a.successor())?;
        let branch_a_block = branch_a.branch(has_b, on_both.finish(yielded).body, exit_b);
        let exit_a = exit_loop(step.successor())?;

        Ok(SpannedFunction {
            params: loop_params,
            body: step.branch(has_a, branch_a_block, exit_a),
        })
    }

    fn curried_generate_lazy(
        &mut self,
        gen_code: super::CodeId,
        arity: usize,
        collected: Vec<Local>,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        if collected.len() == arity {
            return block.emit(
                Operation::GenerateLazy {
                    code: gen_code,
                    captures: collected,
                    limit: None,
                    sync: true,
                },
                span,
            );
        }
        let code = self.reserve(span)?;
        let mut function = Code::new(vec![0, 1]);
        let mut inside = Vec::with_capacity(collected.len() + 1);
        for field in 0..collected.len() {
            inside.push(function.emit(
                Operation::Project {
                    object: 0,
                    field,
                    rep: ValueRep::Cell,
                },
                span,
            )?);
        }
        inside.push(1);
        let result = self.curried_generate_lazy(gen_code, arity, inside, &mut function, span)?;
        self.finish(code, &function.finish(result))?;
        block.emit(
            Operation::Closure {
                code,
                captures: collected,
            },
            span,
        )
    }
}

fn seq_expression(name: &str, mode: ForeignMode, arity: usize, span: Span) -> Option<Expr> {
    let b = SeqBuilder(span);
    match (name, mode, arity) {
        ("seqFrom", ForeignMode::Pure, 1) => Some(b.seq_from()),
        ("seqRange", ForeignMode::Pure, 2) => Some(b.seq_range(false)),
        ("seqRangeTo", ForeignMode::Pure, 2) => Some(b.seq_range(true)),
        ("seqRangeBy", ForeignMode::Pure, 3) => Some(b.seq_range_by()),
        ("seqIterate", ForeignMode::Pure, 2) => Some(b.seq_iterate()),
        ("seqRepeat", ForeignMode::Pure, 1) => Some(b.seq_repeat()),
        ("seqCycle", ForeignMode::Pure, 1) => Some(b.seq_cycle()),
        ("seqUnfold", ForeignMode::Pure, 2) => Some(b.seq_unfold()),
        ("seqAppend", ForeignMode::Pure, 2) => Some(b.seq_append()),
        ("seqMap", ForeignMode::Pure, 2) => Some(b.seq_map()),
        ("seqMap2", ForeignMode::Pure, 3) => Some(b.seq_map2()),
        ("seqFilter", ForeignMode::Pure, 2) => Some(b.seq_filter()),
        ("seqFilterMap", ForeignMode::Pure, 2) => Some(b.seq_filter_map()),
        ("seqFlatMap", ForeignMode::Pure, 2) => Some(b.seq_flat_map()),
        ("seqTake", ForeignMode::Pure, 2) => Some(b.seq_take()),
        ("seqTakeWhile", ForeignMode::Pure, 2) => Some(b.seq_take_while()),
        ("seqDrop", ForeignMode::Pure, 2) => Some(b.seq_drop()),
        ("seqDropWhile", ForeignMode::Pure, 2) => Some(b.seq_drop_while()),
        ("seqEnumerate", ForeignMode::Pure, 1) => Some(b.seq_enumerate()),
        ("seqScan", ForeignMode::Pure, 3) => Some(b.seq_scan()),
        ("seqChunks", ForeignMode::Pure, 2) => Some(b.seq_chunks()),
        ("seqWindows", ForeignMode::Pure, 2) => Some(b.seq_windows()),
        ("seqFold", ForeignMode::Pure, 3) => Some(b.seq_fold(false)),
        ("seqAny", ForeignMode::Pure, 2) => Some(b.seq_quantifier(true)),
        ("seqAll", ForeignMode::Pure, 2) => Some(b.seq_quantifier(false)),
        ("seqFind", ForeignMode::Pure, 2) => Some(b.seq_find()),
        ("seqFirst", ForeignMode::Pure, 1) => Some(b.seq_first()),
        ("seqEachEffect" | "listEachEffect", ForeignMode::Effect, 2) => Some(b.seq_each_effect()),
        ("seqFoldEffect" | "listFoldEffect", ForeignMode::Effect, 3) => Some(b.seq_fold(true)),
        ("listMapEffect", ForeignMode::Effect, 2) => Some(b.list_map_effect()),
        _ => None,
    }
}

struct SeqBuilder(Span);

impl SeqBuilder {
    fn at(&self, kind: ExprKind) -> Expr {
        Expr::new(kind, self.0)
    }
    fn unit(&self) -> Expr {
        self.at(ExprKind::Lit(Lit::Unit))
    }
    fn int(&self, n: i32) -> Expr {
        self.at(ExprKind::Lit(Lit::Int(n)))
    }
    fn bool(&self, v: bool) -> Expr {
        self.at(ExprKind::Lit(Lit::Bool(v)))
    }
    fn loc(&self, name: &str) -> Expr {
        self.at(ExprKind::Local(format!("$seq.{name}")))
    }
    fn var(&self, name: &str) -> Expr {
        self.at(ExprKind::Var(format!("$seq.{name}")))
    }
    fn vname(name: &str) -> String {
        format!("$seq.{name}")
    }
    fn lam(&self, names: &[&str], body: Expr) -> Expr {
        self.at(ExprKind::Lambda {
            params: names
                .iter()
                .map(|n| Param {
                    pat: Pat::Bind(Self::vname(n)),
                    stage: Stage::Pure,
                    ty: None,
                    binder_types: Vec::new(),
                })
                .collect(),
            body: Box::new(body),
        })
    }
    fn app(&self, f: Expr, a: Expr) -> Expr {
        self.at(ExprKind::App {
            func: Box::new(f),
            arg: Box::new(a),
            stage: Stage::Pure,
            arg_first: false,
        })
    }
    fn app2(&self, f: Expr, a: Expr, b: Expr) -> Expr {
        let fa = self.app(f, a);
        self.app(fa, b)
    }
    fn bin(&self, op: BinOp, l: Expr, r: Expr) -> Expr {
        self.at(ExprKind::Binary(op, Box::new(l), Box::new(r)))
    }
    fn add(&self, l: Expr, r: Expr) -> Expr {
        self.bin(BinOp::Arith(Arith::Add, Num::Int), l, r)
    }
    fn sub(&self, l: Expr, r: Expr) -> Expr {
        self.bin(BinOp::Arith(Arith::Sub, Num::Int), l, r)
    }
    fn if_(&self, c: Expr, t: Expr, e: Expr) -> Expr {
        self.at(ExprKind::If(Box::new(c), Box::new(t), Box::new(e)))
    }
    fn block(&self, stmts: Vec<Stmt>, tail: Expr) -> Expr {
        self.at(ExprKind::Block(stmts, Box::new(tail)))
    }
    fn yield_(&self, v: Expr) -> Expr {
        self.at(ExprKind::Yield(Box::new(v)))
    }
    fn gen_lazy(&self, body: Expr) -> Expr {
        self.at(ExprKind::Generate {
            body: Box::new(body),
            strict: false,
            sync: true,
            limit: None,
        })
    }
    fn each(&self, item: &str, src: Expr, body: Expr) -> Expr {
        self.at(ExprKind::Each {
            stop: None,
            pat: Pat::Bind(Self::vname(item)),
            binder_types: Vec::new(),
            source: Box::new(src),
            body: Box::new(body),
        })
    }
    fn for_ctrl(&self, id: &str, item: &str, src: Expr, body: Expr) -> Expr {
        self.at(ExprKind::ControlLoop {
            id: id.into(),
            condition: None,
            body: Box::new(self.each(item, src, body)),
            iterable: true,
        })
    }
    fn break_(&self, id: &str, val: Expr) -> Expr {
        self.at(ExprKind::LoopExit {
            id: id.into(),
            value: Some(Box::new(val)),
        })
    }
    fn some_con() -> Constructor {
        Constructor {
            module: "Prelude".into(),
            ty: "Option".into(),
            name: "Some".into(),
            arity: 1,
            dicts: 0,
            rep: Rep::Tagged,
            fields: Vec::new(),
            type_args: Vec::new(),
        }
    }
    fn none_con() -> Constructor {
        Constructor {
            module: "Prelude".into(),
            ty: "Option".into(),
            name: "None".into(),
            arity: 0,
            dicts: 0,
            rep: Rep::Tagged,
            fields: Vec::new(),
            type_args: Vec::new(),
        }
    }
    fn some(&self, v: Expr) -> Expr {
        self.app(self.at(ExprKind::Con(Self::some_con())), v)
    }
    fn none(&self) -> Expr {
        self.at(ExprKind::Con(Self::none_con()))
    }
    fn some_pat(inner: Pat) -> Pat {
        Pat::Con(Self::some_con(), vec![inner])
    }
}

include!("seq_sources.rs");
include!("seq_ops.rs");
