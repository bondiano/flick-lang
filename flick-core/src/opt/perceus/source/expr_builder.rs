//! Compact closed-template AST builder for audited pure stdlib adapters.

use flick_ir::{Arith, BinOp, Constructor, Num, Rep};

use super::{Arm, Expr, ExprKind, Lit, Param, Pat, Span, Stage, Stmt};

pub(super) fn arm(pat: Pat, body: Expr) -> Arm {
    Arm {
        pat,
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

    pub(super) fn block(&self, stmts: Vec<Stmt>, tail: Expr) -> Expr {
        self.at(ExprKind::Block(stmts, Box::new(tail)))
    }

    pub(super) fn let_(&self, name: &str, val: Expr) -> Stmt {
        Stmt::Let(Pat::Bind(self.name(name)), val)
    }

    pub(super) fn var_decl(&self, name: &str, init: Expr) -> Stmt {
        Stmt::Var(self.name(name), init)
    }

    pub(super) fn assign(&self, name: &str, val: Expr) -> Stmt {
        Stmt::Assign(self.name(name), val)
    }

    pub(super) fn tuple(&self, items: Vec<Expr>) -> Expr {
        self.at(ExprKind::Tuple(items))
    }

    pub(super) fn list(&self, items: Vec<Expr>) -> Expr {
        self.at(ExprKind::List(items))
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

    pub(super) fn each(&self, item: &str, src: Expr, body: Expr) -> Expr {
        self.at(ExprKind::Each {
            stop: None,
            pat: Pat::Bind(self.name(item)),
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

    pub(super) fn match_(&self, scr: Expr, arms: Vec<Arm>) -> Expr {
        self.at(ExprKind::Match(Box::new(scr), arms))
    }

    pub(super) fn con(module: &str, ty: &str, name: &str, arity: usize) -> Constructor {
        Constructor {
            module: module.into(),
            ty: ty.into(),
            name: name.into(),
            arity,
            dicts: 0,
            rep: Rep::Tagged,
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
