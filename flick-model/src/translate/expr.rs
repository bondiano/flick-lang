//! Expressions and patterns of the IR to λFlick, by the reading of
//! `docs/formal/core.md` §1.4: blocks are nested `let`s, `if` is a
//! `match` on `Bool`, operators are constants, a lift is the identity.

use std::rc::Rc;

use flick_core::ir::{self, BinOp, ExprKind, Stmt};
use flick_core::types::Constructor;

use super::{Tr, dict, given, lams};
use crate::handler::{Handled, Handler, Op};
use crate::syntax::{Arm, Con, Const, Lit, Name, Pat, Path, Prim, Term};

/// The model's constructor of an IR one. A constructor of a GADT takes
/// the dictionaries it holds first, as its scheme's arrows do.
pub fn con(c: &Constructor) -> Rc<Con> {
    Rc::new(Con {
        ty: Path::new(&c.module, &c.ty),
        name: c.name.as_str().into(),
        arity: c.arity + c.dicts,
        dicts: c.dicts,
    })
}

fn stuck(what: &str) -> Term {
    Term::Stuck(format!("{what} has no λFlick form").into())
}

/// `match c with True → a | False → b`.
pub fn cond(c: Term, then: Term, otherwise: Term) -> Term {
    let arm = |b: bool, body: Term| Arm {
        pat: Pat::Lit(Lit::Bool(b)),
        guard: None,
        body,
    };
    Term::Match(
        Rc::new(c),
        Rc::from([arm(true, then), arm(false, otherwise)]),
    )
}

impl Tr {
    pub(super) fn expr(&mut self, e: &ir::Expr) -> Term {
        let rc = |t: Term| Rc::new(t);
        match &e.kind {
            ExprKind::Lit(l) => lit(l).map_or_else(|| stuck("a `BigInt` literal"), Term::Lit),
            ExprKind::Local(x) => Term::Var(x.as_str().into()),
            ExprKind::Global { module, name } => Term::Global(Path::new(module, name)),
            ExprKind::Method { dict: d, name } => Term::Field(rc(dict(d)), name.as_str().into()),
            ExprKind::Con(c) => Term::konst_con(con(c)),
            ExprKind::Perform {
                effect: service,
                op: name,
                stages,
            } => self.operation(service, name, stages, &[]),
            ExprKind::ServiceTag(s) => Term::Tag(Path::parse(s)),
            ExprKind::Dicts(inner, dicts) => {
                let dicts: Vec<Term> = dicts.iter().map(dict).collect();
                match &inner.kind {
                    // the dictionaries go to the implementation, after the lookup
                    ExprKind::Perform {
                        effect: service,
                        op: name,
                        stages,
                    } => self.operation(service, name, stages, &dicts),
                    _ => Term::apps(self.expr(inner), dicts),
                }
            }
            ExprKind::Abstract(names, body) => {
                lams(names.iter().map(|n| given(n)), self.expr(body))
            }
            ExprKind::App { func, arg, .. } if layer_run(func).is_some() => self.provide(func, arg),
            ExprKind::App { func, arg, .. } if is_global(func, "Scope", "run") => self.scope(arg),
            ExprKind::App { func, .. } if is_global(func, "Scope", "current") => Term::unit(),
            ExprKind::App {
                func,
                arg,
                arg_first,
                ..
            } => {
                let (f, a) = (self.expr(func), self.expr(arg));
                if *arg_first {
                    let x = self.fresh("arg");
                    Term::Let(x.clone(), None, rc(a), rc(Term::app(f, Term::Var(x))))
                } else {
                    Term::app(f, a)
                }
            }
            ExprKind::Lambda { params, body } => {
                let mut body = self.expr(body);
                for p in params.iter().rev() {
                    body = self.param(&p.pat, body);
                }
                body
            }
            ExprKind::Block(stmts, tail) => {
                let tail = self.expr(tail);
                stmts.iter().rev().fold(tail, |rest, s| self.stmt(s, rest))
            }
            ExprKind::If(c, a, b) => cond(self.expr(c), self.expr(a), self.expr(b)),
            ExprKind::Match(e, arms) => Term::Match(rc(self.expr(e)), self.arms(arms)),
            ExprKind::Handle(h) if h.is_try() => {
                Term::Try(rc(self.expr(&h.body)), self.arms(&h.errors))
            }
            ExprKind::Tuple(xs) if xs.is_empty() => Term::unit(),
            ExprKind::Tuple(xs) => Term::Tuple(xs.iter().map(|x| self.expr(x)).collect()),
            ExprKind::List(xs) => Term::List(xs.iter().map(|x| self.expr(x)).collect()),
            ExprKind::Record(fs) | ExprKind::Dictionary(fs) => Term::Record(
                fs.iter()
                    .map(|(l, x)| (Name::from(l.as_str()), self.expr(x)))
                    .collect(),
            ),
            ExprKind::Update(base, fs) => fs.iter().fold(self.expr(base), |r, (l, x)| {
                Term::Update(rc(r), l.as_str().into(), rc(self.expr(x)))
            }),
            // `x._tag` compares constructors of types without fields
            ExprKind::Field(x, l) if l == "_tag" => {
                Term::app(Term::prim(Prim::TagOf), self.expr(x))
            }
            ExprKind::Field(x, l) => Term::Field(rc(self.expr(x)), l.as_str().into()),
            ExprKind::Binary(op, a, b) => {
                let (a, b) = (self.expr(a), self.expr(b));
                binary(*op, a, b)
            }
            ExprKind::Not(x) => Term::app(Term::prim(Prim::Not), self.expr(x)),
            ExprKind::Neg(num, x) => Term::app(Term::prim(Prim::Neg(*num)), self.expr(x)),
            ExprKind::Ignore(x) => Term::Let("_".into(), None, rc(self.expr(x)), rc(Term::unit())),
            ExprKind::Await(x) => Term::Await(rc(self.expr(x))),
            ExprKind::Task(x) => Term::Task(rc(self.expr(x))),
            ExprKind::Handle(h) => Term::Handler(Rc::new(self.handler(h))),
            // a lift does nothing in the model (§3.4); its correctness is G2
            ExprKind::Lift(x, _) => self.expr(x),
            ExprKind::Label(_) => stuck("`Record.label`"),
            ExprKind::Generate { .. } | ExprKind::Each { .. } | ExprKind::Yield(_) => {
                stuck("a `for` or a `Seq`")
            }
            ExprKind::Unwrap(_) => stuck("`?` on an `Option`, a return from the function,"),
            // shortcut: the model acquires into the innermost `scope`, not the
            // owner a `Within` names (`regions.md` §1.4, G6); a program that
            // acquires into an outer region from a nested one is rejected
            ExprKind::Within(_, inner) => self.expr(inner),
        }
    }

    /// `S.op` with `op : τ₁ →R⟨r;ε⟩ τ₂` is `λx. (ask_S ()).op x` (§1.4):
    /// it looks the service up at its first runtime stage.
    fn operation(
        &mut self,
        service: &str,
        name: &str,
        stages: &[ir::Stage],
        dicts: &[Term],
    ) -> Term {
        let n = ir::operation_arity(stages);
        let xs: Vec<Name> = (0..n).map(|_| self.fresh("op")).collect();
        let ask = Term::app(Term::konst(Const::Ask(Path::parse(service))), Term::unit());
        let op = Term::Field(Rc::new(ask), name.into());
        let call = Term::apps(
            Term::apps(op, dicts.iter().cloned()),
            xs.iter().cloned().map(Term::Var),
        );
        lams(xs, call)
    }

    /// `λp. body`, with a pattern that is not a variable matched inside.
    fn param(&mut self, p: &ir::Pat, body: Term) -> Term {
        if let ir::Pat::Bind(x) = p {
            return Term::lam(x, body);
        }
        let x = self.fresh("p");
        let arm = Arm {
            pat: pat(p),
            guard: None,
            body,
        };
        Term::lam(
            &x,
            Term::Match(Rc::new(Term::Var(x.clone())), Rc::from([arm])),
        )
    }

    fn stmt(&mut self, s: &Stmt, rest: Term) -> Term {
        let rc = |t: Term| Rc::new(t);
        match s {
            Stmt::Let(ir::Pat::Bind(x), e) => {
                Term::Let(x.as_str().into(), None, rc(self.expr(e)), rc(rest))
            }
            Stmt::Let(ir::Pat::Wild, e) | Stmt::Expr(e) => {
                Term::Let("_".into(), None, rc(self.expr(e)), rc(rest))
            }
            Stmt::Let(p, e) => {
                let arm = Arm {
                    pat: pat(p),
                    guard: None,
                    body: rest,
                };
                Term::Match(rc(self.expr(e)), Rc::from([arm]))
            }
            Stmt::Fun {
                name,
                rec,
                dicts,
                value,
            } => {
                let value = lams(dicts.iter().map(|d| given(d)), self.expr(value));
                let name: Name = name.as_str().into();
                if !rec {
                    Term::Let(name, None, rc(value), rc(rest))
                } else if matches!(value, Term::Lam(..)) {
                    Term::LetRec(name, None, rc(value), rc(rest))
                } else {
                    stuck("a recursive local that is not a function")
                }
            }
        }
    }

    fn arms(&mut self, arms: &[ir::Arm]) -> Rc<[Arm]> {
        arms.iter()
            .map(|a| Arm {
                pat: pat(&a.pat),
                guard: a.guard.as_ref().map(|g| self.expr(g)),
                body: self.expr(&a.body),
            })
            .collect()
    }

    /// A handler of effects: the function of each operation as it is.
    fn handler(&mut self, h: &ir::Handler) -> Handler {
        let effects = h
            .effects
            .iter()
            .map(|e| Handled {
                effect: Path::parse(&e.effect),
                ops: e
                    .operations
                    .iter()
                    .map(|o| Op {
                        name: o.name.as_str().into(),
                        class: o.class,
                        dicts: o.dicts,
                        arity: o.arity,
                        covers: o.covers,
                        function: self.expr(&o.function),
                    })
                    .collect(),
            })
            .collect();
        Handler {
            effects,
            body: self.expr(&h.body),
            ret: h.ret.as_ref().map(|r| self.expr(r)),
            errors: self.arms(&h.errors),
        }
    }
}

pub fn pat(p: &ir::Pat) -> Pat {
    match p {
        ir::Pat::Wild => Pat::Wild,
        ir::Pat::Bind(x) => Pat::Var(x.as_str().into()),
        // a `BigInt` pattern matches no value the model has
        ir::Pat::Lit(l) => lit(l).map_or(Pat::Or(Rc::from([])), Pat::Lit),
        ir::Pat::Tuple(ps) if ps.is_empty() => Pat::Lit(Lit::Unit),
        ir::Pat::Tuple(ps) => Pat::Tuple(ps.iter().map(pat).collect()),
        ir::Pat::List(ps, rest) => Pat::List(
            ps.iter().map(pat).collect(),
            rest.as_ref().map(|r| Rc::new(pat(r))),
        ),
        ir::Pat::Record(fs) => Pat::Record(
            fs.iter()
                .map(|(l, p)| (Name::from(l.as_str()), pat(p)))
                .collect(),
        ),
        // the IR binds a value's dictionaries after its fields; the model
        // holds them first
        ir::Pat::Con(c, ps) => {
            let (fields, dicts) = ps.split_at(ps.len() - c.dicts.min(ps.len()));
            Pat::Con(con(c), dicts.iter().chain(fields).map(pat).collect())
        }
        ir::Pat::Or(ps) => Pat::Or(ps.iter().map(pat).collect()),
    }
}

fn lit(l: &ir::Lit) -> Option<Lit> {
    Some(match l {
        ir::Lit::Int(i) => Lit::Int(*i),
        ir::Lit::Float(x) => Lit::Float(*x),
        ir::Lit::String(s) => Lit::String(s.as_str().into()),
        ir::Lit::Bool(b) => Lit::Bool(*b),
        ir::Lit::Unit => Lit::Unit,
        ir::Lit::BigInt(_) => return None,
    })
}

fn binary(op: BinOp, a: Term, b: Term) -> Term {
    let prim = match op {
        // short-circuit
        BinOp::And => return cond(a, b, Term::bool(false)),
        BinOp::Or => return cond(a, Term::bool(true), b),
        BinOp::Eq => Prim::Eq,
        BinOp::NotEq => Prim::NotEq,
        BinOp::Same => Prim::Same,
        BinOp::NotSame => Prim::NotSame,
        BinOp::Lt => Prim::Lt,
        BinOp::Le => Prim::Le,
        BinOp::Gt => Prim::Gt,
        BinOp::Ge => Prim::Ge,
        BinOp::Concat => Prim::Concat,
        BinOp::ListConcat => Prim::ListConcat,
        BinOp::Arith(op, num) => Prim::Arith(op, num),
    };
    Term::apps(Term::prim(prim), [a, b])
}

impl Tr {
    /// `Scope.run arg`, which is `scope s { body }`: the core's form of it.
    fn scope(&mut self, arg: &ir::Expr) -> Term {
        if let ExprKind::Lambda { params, body } = &arg.kind
            && let [ir::Param { pat, .. }] = &params[..]
        {
            match pat {
                ir::Pat::Bind(s) => {
                    return Term::Scope(Name::from(s.as_str()), Rc::new(self.expr(body)));
                }
                ir::Pat::Wild => {
                    let s = self.fresh("s");
                    return Term::Scope(s, Rc::new(self.expr(body)));
                }
                _ => {}
            }
        }
        let s = self.fresh("s");
        let body = Term::app(self.expr(arg), Term::Var(s.clone()));
        Term::Scope(s, Rc::new(body))
    }

    /// `Layer.run layer arg`, which is `provide layer { body }`: the core's
    /// form of it.
    fn provide(&mut self, func: &ir::Expr, arg: &ir::Expr) -> Term {
        let layer = self.expr(layer_run(func).expect("a call of `Layer.run`"));
        let body = match &arg.kind {
            // what `provide` lowers to, which binds nothing
            ExprKind::Lambda { params, body }
                if matches!(
                    params[..],
                    [ir::Param {
                        pat: ir::Pat::Wild,
                        ..
                    }]
                ) =>
            {
                self.expr(body)
            }
            _ => Term::app(self.expr(arg), Term::unit()),
        };
        Term::Provide(Rc::new(layer), Rc::new(body))
    }
}

fn is_global(func: &ir::Expr, m: &str, n: &str) -> bool {
    matches!(&func.kind, ExprKind::Global { module, name } if module == m && name == n)
}

/// The layer of `Layer.run layer`.
fn layer_run(func: &ir::Expr) -> Option<&ir::Expr> {
    let ExprKind::App { func, arg, .. } = &func.kind else {
        return None;
    };
    match &func.kind {
        ExprKind::Global { module, name } if module == "Layer" && name == "run" => Some(arg),
        _ => None,
    }
}
