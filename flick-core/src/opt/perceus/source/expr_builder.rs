//! Compact closed-template AST builder for audited pure stdlib adapters.

use flick_ir::{Arith, BinOp, Constructor, Num, Rep};

use super::{Arm, Expr, ExprKind, Lit, Param, Pat, Span, Stage, Stmt};

pub(super) fn arm(pat: Pat, body: Expr) -> Arm {
    Arm {
        pat,
        binder_types: Vec::new(),
        guard: None,
        body,
    }
}

pub(super) struct ExprBuilder {
    span: Span,
    prefix: &'static str,
}

impl ExprBuilder {
    pub(super) fn new(span: Span, prefix: &'static str) -> Self {
        Self { span, prefix }
    }

    pub(super) fn at(&self, kind: ExprKind) -> Expr {
        Expr::new(kind, self.span)
    }

    pub(super) fn name(&self, s: &str) -> String {
        format!("{}.{s}", self.prefix)
    }

    pub(super) fn unit(&self) -> Expr {
        self.at(ExprKind::Lit(Lit::Unit))
    }

    pub(super) fn int(&self, n: i32) -> Expr {
        self.at(ExprKind::Lit(Lit::Int(n)))
    }

    pub(super) fn float(&self, f: f64) -> Expr {
        self.at(ExprKind::Lit(Lit::Float(f)))
    }

    pub(super) fn bigint(&self, s: &str) -> Expr {
        self.at(ExprKind::Lit(Lit::BigInt(s.into())))
    }

    pub(super) fn bool_(&self, v: bool) -> Expr {
        self.at(ExprKind::Lit(Lit::Bool(v)))
    }

    pub(super) fn loc(&self, s: &str) -> Expr {
        self.at(ExprKind::Local(self.name(s)))
    }

    pub(super) fn var(&self, s: &str) -> Expr {
        self.at(ExprKind::Var(self.name(s)))
    }

    pub(super) fn global(&self, module: &str, name: &str) -> Expr {
        self.at(ExprKind::Global {
            module: module.into(),
            name: name.into(),
            type_args: Vec::new(),
        })
    }

    pub(super) fn lam(&self, names: &[&str], body: Expr) -> Expr {
        self.at(ExprKind::Lambda {
            params: names
                .iter()
                .map(|n| Param {
                    pat: Pat::Bind(self.name(n)),
                    stage: Stage::Pure,
                    ty: None,
                    binder_types: Vec::new(),
                })
                .collect(),
            body: Box::new(body),
        })
    }

    pub(super) fn app(&self, f: Expr, a: Expr) -> Expr {
        self.at(ExprKind::App {
            func: Box::new(f),
            arg: Box::new(a),
            stage: Stage::Pure,
            arg_first: false,
        })
    }

    pub(super) fn app2(&self, f: Expr, a: Expr, b: Expr) -> Expr {
        let fa = self.app(f, a);
        self.app(fa, b)
    }

    pub(super) fn app3(&self, f: Expr, a: Expr, b: Expr, c: Expr) -> Expr {
        let fab = self.app2(f, a, b);
        self.app(fab, c)
    }

    pub(super) fn app4(&self, func: Expr, a1: Expr, a2: Expr, a3: Expr, a4: Expr) -> Expr {
        let fabc = self.app3(func, a1, a2, a3);
        self.app(fabc, a4)
    }

    pub(super) fn app5(
        &self,
        func: Expr,
        a1: Expr,
        a2: Expr,
        a3: Expr,
        a4: Expr,
        a5: Expr,
    ) -> Expr {
        let fabcd = self.app4(func, a1, a2, a3, a4);
        self.app(fabcd, a5)
    }

    pub(super) fn app6(&self, func: Expr, args: [Expr; 6]) -> Expr {
        args.into_iter().fold(func, |acc, arg| self.app(acc, arg))
    }

    pub(super) fn str_(&self, s: &str) -> Expr {
        self.at(ExprKind::Lit(Lit::String(s.into())))
    }

    pub(super) fn task(&self, body: Expr) -> Expr {
        self.at(ExprKind::Task(Box::new(body)))
    }

    pub(super) fn await_(&self, t: Expr) -> Expr {
        self.at(ExprKind::Await(Box::new(t)))
    }

    pub(super) fn tup_pat(&self, names: &[&str]) -> Pat {
        Pat::Tuple(
            names
                .iter()
                .map(|n| {
                    if *n == "_" {
                        Pat::Wild
                    } else {
                        Pat::Bind(self.name(n))
                    }
                })
                .collect(),
        )
    }

    pub(super) fn loop_ctrl(&self, id: &str, body: Expr) -> Expr {
        self.at(ExprKind::ControlLoop {
            id: id.into(),
            condition: None,
            body: Box::new(body),
            iterable: false,
        })
    }

    pub(super) fn rmake(&self, v: Expr) -> Expr {
        self.app(self.global("Ref", "make"), v)
    }

    pub(super) fn rget(&self, r: Expr) -> Expr {
        self.app(self.global("Ref", "get"), r)
    }

    pub(super) fn rset(&self, r: Expr, v: Expr) -> Stmt {
        Stmt::Expr(self.app2(self.global("Ref", "set"), r, v))
    }

    pub(super) fn dmake(&self) -> Expr {
        self.app(self.global("Deferred", "make"), self.unit())
    }

    pub(super) fn dsucceed(&self, v: Expr, d: Expr) -> Stmt {
        self.let_("_dw", self.app2(self.global("Deferred", "succeed"), v, d))
    }

    pub(super) fn dawait(&self, d: Expr) -> Expr {
        self.app(self.global("Deferred", "await_"), d)
    }

    pub(super) fn no_preempt(&self, body: Expr) -> Expr {
        let pref = self.app2(
            self.global("Context", "reference"),
            self.str_("flick/Scheduler/PreventYield"),
            self.lam(&["_u"], self.bool_(false)),
        );
        self.app3(
            self.global("Context", "with_reference"),
            pref,
            self.bool_(true),
            self.task(body),
        )
    }

    pub(super) fn bin(&self, op: BinOp, l: Expr, r: Expr) -> Expr {
        self.at(ExprKind::Binary(op, Box::new(l), Box::new(r)))
    }

    pub(super) fn not(&self, e: Expr) -> Expr {
        self.at(ExprKind::Not(Box::new(e)))
    }

    pub(super) fn add_i(&self, l: Expr, r: Expr) -> Expr {
        self.bin(BinOp::Arith(Arith::Add, Num::Int), l, r)
    }

    pub(super) fn sub_i(&self, l: Expr, r: Expr) -> Expr {
        self.bin(BinOp::Arith(Arith::Sub, Num::Int), l, r)
    }

    pub(super) fn field(&self, obj: Expr, name: &str) -> Expr {
        self.at(ExprKind::Field(Box::new(obj), name.into()))
    }

    pub(super) fn if_(&self, c: Expr, t: Expr, e: Expr) -> Expr {
        self.at(ExprKind::If(Box::new(c), Box::new(t), Box::new(e)))
    }

    pub(super) fn block(&self, stmts: impl Into<Vec<Stmt>>, tail: Expr) -> Expr {
        self.at(ExprKind::Block(stmts.into(), Box::new(tail)))
    }

    pub(super) fn let_(&self, name: &str, val: Expr) -> Stmt {
        Stmt::let_bind(Pat::Bind(self.name(name)), val)
    }

    pub(super) fn var_decl(&self, name: &str, init: Expr) -> Stmt {
        Stmt::var(self.name(name), None, init)
    }

    pub(super) fn assign(&self, name: &str, val: Expr) -> Stmt {
        Stmt::Assign(self.name(name), val)
    }

    pub(super) fn assign_e(&self, name: &str, val: Expr) -> Expr {
        self.block(vec![self.assign(name, val)], self.unit())
    }

    pub(super) fn tuple(&self, items: impl Into<Vec<Expr>>) -> Expr {
        self.at(ExprKind::Tuple(items.into()))
    }

    pub(super) fn list(&self, items: impl Into<Vec<Expr>>) -> Expr {
        self.at(ExprKind::List(items.into()))
    }

    pub(super) fn yield_(&self, v: Expr) -> Expr {
        self.at(ExprKind::Yield(Box::new(v)))
    }

    pub(super) fn gen_strict(&self, body: Expr) -> Expr {
        self.at(ExprKind::Generate {
            body: Box::new(body),
            strict: true,
            sync: true,
            limit: None,
        })
    }

    pub(super) fn gen_lazy(&self, body: Expr) -> Expr {
        self.at(ExprKind::Generate {
            body: Box::new(body),
            strict: false,
            sync: true,
            limit: None,
        })
    }

    pub(super) fn each(&self, item: &str, src: Expr, body: Expr) -> Expr {
        self.at(ExprKind::Each {
            stop: None,
            pat: Pat::Bind(self.name(item)),
            binder_types: Vec::new(),
            source: Box::new(src),
            body: Box::new(body),
        })
    }

    pub(super) fn for_ctrl(&self, id: &str, item: &str, src: Expr, body: Expr) -> Expr {
        self.at(ExprKind::ControlLoop {
            id: id.into(),
            condition: None,
            body: Box::new(self.each(item, src, body)),
            iterable: true,
        })
    }

    pub(super) fn while_(&self, cond: Expr, body: Expr) -> Expr {
        self.at(ExprKind::Loop {
            condition: Box::new(cond),
            body: Box::new(body),
        })
    }

    pub(super) fn break_(&self, id: &str, val: Expr) -> Expr {
        self.at(ExprKind::LoopExit {
            id: id.into(),
            value: Some(Box::new(val)),
        })
    }

    pub(super) fn match_(&self, scr: Expr, arms: impl Into<Vec<Arm>>) -> Expr {
        self.at(ExprKind::Match(Box::new(scr), arms.into()))
    }

    pub(super) fn con(module: &str, ty: &str, name: &str, arity: usize) -> Constructor {
        Constructor {
            module: module.into(),
            ty: ty.into(),
            name: name.into(),
            arity,
            dicts: 0,
            rep: Rep::Tagged,
            fields: Vec::new(),
            type_args: Vec::new(),
        }
    }

    pub(super) fn some(&self, v: Expr) -> Expr {
        self.app(
            self.at(ExprKind::Con(Self::con("Prelude", "Option", "Some", 1))),
            v,
        )
    }

    pub(super) fn none(&self) -> Expr {
        self.at(ExprKind::Con(Self::con("Prelude", "Option", "None", 0)))
    }

    pub(super) fn ok(&self, v: Expr) -> Expr {
        self.app(
            self.at(ExprKind::Con(Self::con("Prelude", "Result", "Ok", 1))),
            v,
        )
    }

    pub(super) fn err(&self, v: Expr) -> Expr {
        self.app(
            self.at(ExprKind::Con(Self::con("Prelude", "Result", "Err", 1))),
            v,
        )
    }

    pub(super) fn ord(&self, name: &str) -> Expr {
        self.at(ExprKind::Con(Self::con("Prelude", "Ordering", name, 0)))
    }

    pub(super) fn some_pat(&self, name: &str) -> Pat {
        Pat::Con(
            Self::con("Prelude", "Option", "Some", 1),
            vec![Pat::Bind(self.name(name))],
        )
    }

    pub(super) fn ok_pat(&self, name: &str) -> Pat {
        Pat::Con(
            Self::con("Prelude", "Result", "Ok", 1),
            vec![Pat::Bind(self.name(name))],
        )
    }

    pub(super) fn err_pat(&self, name: &str) -> Pat {
        Pat::Con(
            Self::con("Prelude", "Result", "Err", 1),
            vec![Pat::Bind(self.name(name))],
        )
    }
}
