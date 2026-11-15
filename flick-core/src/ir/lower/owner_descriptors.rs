//! Ordinary-IR interpreter for finite original owner-type templates.
//! Descriptors are `(syntax node, parameter environment)`; environments return
//! lazy descriptors, so neither alias arguments nor recursive results expand
//! while a foreign value is constructed.
use super::Lower;
use crate::ir::{Arm, BinOp, Expr, ExprKind, Lit, Param, Pat, Stage, Stmt};
use crate::numeric::{Arith, Num};
use crate::source::Span;
use crate::types::lang::{self, Lang};
use crate::types::{
    Dict, Elem, OwnerAbstract, OwnerProductivity, OwnerRawHead, OwnerTypeExpr, OwnerTypeKind,
    OwnerTypeTemplates,
};

#[derive(Clone, Copy)]
enum Op {
    Resolve,
    Spine,
    Head,
    Row,
    Merge,
    Insert,
    Filter,
    Equal,
    Tag,
    Children,
    EqualChildren,
    Atom,
    Scope,
    Dictionary,
    Adapt,
    HasLabel,
    HasEntry,
    IsSymbol,
    Abstract,
    AbstractApply,
    AbstractApplyAll,
    AbstractPack,
    AbstractEntries,
    Productive,
    ProductiveSummary,
    AbstractProductive,
    AbstractRowOccupied,
    AliasBody,
    KnownNode,
}

struct Build<'a> {
    templates: &'a OwnerTypeTemplates,
    run: String,
    impossible: String,
    span: Span,
    next: usize,
}

impl Lower<'_> {
    pub(super) fn owner_descriptor(&mut self, value: Expr, templates: &OwnerTypeTemplates) -> Expr {
        assert!(
            !templates.has_meta_views(),
            "owner templates must be finalized before lowering"
        );
        let run = self.hidden("$ownerTypes");
        let mut build = Build {
            templates,
            impossible: format!("{run}$invalid"),
            run,
            span: value.span,
            next: 0,
        };
        build.lower(value)
    }
}

impl Build<'_> {
    fn e(&self, kind: ExprKind) -> Expr {
        Expr::new(kind, self.span)
    }
    fn local(&self, name: &str) -> Expr {
        self.e(ExprKind::Local(name.to_owned()))
    }
    fn int(&self, n: usize) -> Expr {
        self.e(ExprKind::Lit(Lit::Int(
            i32::try_from(n).expect("owner syntax index"),
        )))
    }
    fn boolean(&self, b: bool) -> Expr {
        self.e(ExprKind::Lit(Lit::Bool(b)))
    }
    fn string(&self, s: &str) -> Expr {
        self.e(ExprKind::Lit(Lit::String(s.to_owned())))
    }
    fn unit(&self) -> Expr {
        self.e(ExprKind::Lit(Lit::Unit))
    }
    fn tuple(&self, xs: Vec<Expr>) -> Expr {
        self.e(ExprKind::Tuple(xs))
    }
    fn list(&self, xs: Vec<Expr>) -> Expr {
        self.e(ExprKind::List(xs))
    }
    fn app(&self, f: Expr, a: Expr, stage: Stage) -> Expr {
        self.e(ExprKind::App {
            func: Box::new(f),
            arg: Box::new(a),
            stage,
            arg_first: false,
        })
    }
    fn pure(&self, f: Expr, a: Expr) -> Expr {
        self.app(f, a, Stage::Pure)
    }
    fn binary(&self, op: BinOp, a: Expr, b: Expr) -> Expr {
        self.e(ExprKind::Binary(op, Box::new(a), Box::new(b)))
    }
    fn same(&self, a: Expr, b: Expr) -> Expr {
        self.binary(BinOp::Eq, a, b)
    }
    fn both(&self, a: Expr, b: Expr) -> Expr {
        self.binary(BinOp::And, a, b)
    }
    fn choose(&self, c: Expr, a: Expr, b: Expr) -> Expr {
        if let ExprKind::Lit(Lit::Bool(condition)) = c.kind {
            return if condition { a } else { b };
        }
        self.e(ExprKind::If(Box::new(c), Box::new(a), Box::new(b)))
    }
    fn block(&self, statements: Vec<Stmt>, body: Expr) -> Expr {
        self.e(ExprKind::Block(statements, Box::new(body)))
    }
    fn lambda(&self, pat: Pat, stage: Stage, body: Expr) -> Expr {
        self.e(ExprKind::Lambda {
            params: vec![Param {
                pat,
                stage,
                ty: None,
            }],
            body: Box::new(body),
        })
    }
    fn case(&self, value: Expr, mut arms: Vec<(Pat, Expr)>) -> Expr {
        if arms
            .iter()
            .any(|(pat, _)| matches!(pat, Pat::Lit(Lit::Int(_))))
            && !arms
                .iter()
                .any(|(pat, _)| matches!(pat, Pat::Wild | Pat::Bind(_)))
        {
            arms.push((
                Pat::Wild,
                self.pure(self.local(&self.impossible), self.unit()),
            ));
        }
        self.e(ExprKind::Match(
            Box::new(value),
            arms.into_iter()
                .map(|(pat, body)| Arm {
                    pat,
                    guard: None,
                    body,
                })
                .collect(),
        ))
    }
    fn name(&mut self) -> String {
        let name = format!("{}${}", self.run, self.next);
        self.next += 1;
        name
    }
    fn bind(&mut self, value: Expr, body: impl FnOnce(&mut Self, Expr) -> Expr) -> Expr {
        let name = self.name();
        let result = body(self, self.local(&name));
        self.block(vec![Stmt::Let(Pat::Bind(name), value)], result)
    }
    fn unpack(&self, value: Expr, names: &[&str], body: Expr) -> Expr {
        self.block(
            vec![Stmt::Let(
                Pat::Tuple(names.iter().map(|s| Pat::Bind((*s).to_owned())).collect()),
                value,
            )],
            body,
        )
    }
    fn desc(&self, node: usize, env: Expr) -> Expr {
        self.tuple(vec![self.int(node), env])
    }
    fn root_env(&self) -> Expr {
        self.lambda(Pat::Wild, Stage::Pure, self.unit())
    }
    fn call(&self, op: Op, desc: Expr, x: Expr, y: Expr) -> Expr {
        self.pure(
            self.local(&self.run),
            self.tuple(vec![self.int(op as usize), desc, x, y]),
        )
    }
    fn observe(&self, op: Op, desc: Expr) -> Expr {
        self.call(op, desc, self.unit(), self.unit())
    }
    fn prepend(&self, head: Expr, tail: Expr) -> Expr {
        self.binary(BinOp::ListConcat, self.list(vec![head]), tail)
    }
    fn environment(&mut self, bindings: Vec<(String, Expr)>, parent: Option<Expr>) -> Expr {
        let key = self.name();
        let mut arms: Vec<_> = bindings
            .into_iter()
            .map(|(name, value)| (Pat::Lit(Lit::String(name)), value))
            .collect();
        arms.push((
            Pat::Wild,
            parent.map_or_else(|| self.unit(), |parent| self.pure(parent, self.local(&key))),
        ));
        self.lambda(
            Pat::Bind(key.clone()),
            Stage::Pure,
            self.case(self.local(&key), arms),
        )
    }
    fn child(&self, id: usize, env: &Expr) -> Expr {
        self.desc(id, env.clone())
    }

    fn lower(&mut self, value: Expr) -> Expr {
        let op = self.name();
        let desc = self.name();
        let x = self.name();
        let y = self.name();
        let arms = [
            Op::Resolve,
            Op::Spine,
            Op::Head,
            Op::Row,
            Op::Merge,
            Op::Insert,
            Op::Filter,
            Op::Equal,
            Op::Tag,
            Op::Children,
            Op::EqualChildren,
            Op::Atom,
            Op::Scope,
            Op::Dictionary,
            Op::Adapt,
            Op::HasLabel,
            Op::HasEntry,
            Op::IsSymbol,
            Op::Abstract,
            Op::AbstractApply,
            Op::AbstractApplyAll,
            Op::AbstractPack,
            Op::AbstractEntries,
            Op::Productive,
            Op::ProductiveSummary,
            Op::AbstractProductive,
            Op::AbstractRowOccupied,
            Op::AliasBody,
            Op::KnownNode,
        ]
        .into_iter()
        .map(|which| {
            (
                Pat::Lit(Lit::Int(which as i32)),
                self.operation(which, self.local(&desc), self.local(&x), self.local(&y)),
            )
        })
        .collect();
        let function = self.lambda(
            Pat::Tuple(vec![
                Pat::Bind(op.clone()),
                Pat::Bind(desc),
                Pat::Bind(x),
                Pat::Bind(y),
            ]),
            Stage::Pure,
            self.case(self.local(&op), arms),
        );
        let raw = self.name();
        let result = self.observe_adapt(
            self.desc(self.templates.root, self.root_env()),
            self.local(&raw),
        );
        self.block(
            vec![
                Stmt::Fun {
                    name: self.impossible.clone(),
                    rec: true,
                    dicts: Vec::new(),
                    value: self.lambda(
                        Pat::Wild,
                        Stage::Pure,
                        self.pure(self.local(&self.impossible), self.unit()),
                    ),
                },
                Stmt::Fun {
                    name: self.run.clone(),
                    rec: true,
                    dicts: Vec::new(),
                    value: function,
                },
                Stmt::Let(Pat::Bind(raw), value),
            ],
            result,
        )
    }
    fn observe_adapt(&self, desc: Expr, raw: Expr) -> Expr {
        self.call(Op::Adapt, desc, raw, self.unit())
    }

    fn nodes(
        &mut self,
        desc: Expr,
        body: impl Fn(&mut Self, usize, &OwnerTypeExpr, Expr) -> Expr,
    ) -> Expr {
        let node = self.name();
        let env = self.name();
        let templates = self.templates;
        let arms = templates
            .exprs
            .iter()
            .enumerate()
            .map(|(id, expression)| {
                (
                    Pat::Lit(Lit::Int(i32::try_from(id).expect("owner syntax index"))),
                    body(self, id, expression, self.local(&env)),
                )
            })
            .collect();
        self.unpack(desc, &[&node, &env], self.case(self.local(&node), arms))
    }

    fn nodes_with_default(
        &mut self,
        desc: Expr,
        default: impl FnOnce(&Self, Expr) -> Expr,
        body: impl Fn(&mut Self, usize, &OwnerTypeExpr, Expr) -> Option<Expr>,
    ) -> Expr {
        let node = self.name();
        let env = self.name();
        let templates = self.templates;
        let mut arms = Vec::new();
        let mut has_default = false;
        for (id, expression) in templates.exprs.iter().enumerate() {
            let pattern = Pat::Lit(Lit::Int(i32::try_from(id).expect("owner syntax index")));
            if let Some(body) = body(self, id, expression, self.local(&env)) {
                arms.push((pattern, body));
            } else {
                has_default = true;
            }
        }
        let fallback = if has_default {
            let current = self.tuple(vec![self.local(&node), self.local(&env)]);
            self.choose(
                self.observe(Op::KnownNode, self.local(&node)),
                default(self, current),
                self.impossible(),
            )
        } else {
            self.impossible()
        };
        // Share the exact original-ID membership table across selectors. The
        // original node and lazy environment still reach the default body.
        arms.push((Pat::Wild, fallback));
        self.unpack(desc, &[&node, &env], self.case(self.local(&node), arms))
    }

    fn known_node(&self, node: Expr) -> Expr {
        if self.templates.exprs.is_empty() {
            return self.boolean(false);
        }
        let ids = (0..self.templates.exprs.len())
            .map(|id| Pat::Lit(Lit::Int(i32::try_from(id).expect("owner syntax index"))))
            .collect();
        self.case(
            node,
            vec![
                (Pat::Or(ids), self.boolean(true)),
                (Pat::Wild, self.boolean(false)),
            ],
        )
    }

    fn nodes_grouped<K: Eq>(
        &mut self,
        desc: Expr,
        key: impl Fn(&Self, &OwnerTypeExpr) -> Option<K>,
        body: impl Fn(&mut Self, usize, &OwnerTypeExpr, Expr, Expr) -> Expr,
    ) -> Expr {
        let node = self.name();
        let env = self.name();
        let templates = self.templates;
        let mut arms = Vec::new();
        let mut groups: Vec<(K, usize, Vec<Pat>)> = Vec::new();
        for (id, expression) in templates.exprs.iter().enumerate() {
            let pattern = Pat::Lit(Lit::Int(i32::try_from(id).expect("owner syntax index")));
            if let Some(key) = key(self, expression) {
                if let Some((_, _, patterns)) =
                    groups.iter_mut().find(|(known, _, _)| known == &key)
                {
                    patterns.push(pattern);
                } else {
                    groups.push((key, id, vec![pattern]));
                }
            } else {
                let current = self.desc(id, self.local(&env));
                arms.push((
                    pattern,
                    body(self, id, expression, self.local(&env), current),
                ));
            }
        }
        for (_, id, patterns) in groups {
            let current = self.tuple(vec![self.local(&node), self.local(&env)]);
            arms.push((
                Pat::Or(patterns),
                body(self, id, &templates.exprs[id], self.local(&env), current),
            ));
        }
        arms.push((Pat::Wild, self.impossible()));
        self.unpack(desc, &[&node, &env], self.case(self.local(&node), arms))
    }

    fn operation(&mut self, op: Op, desc: Expr, x: Expr, y: Expr) -> Expr {
        match op {
            Op::Abstract => self.abstract_type(&desc),
            Op::AbstractApply => self.abstract_apply(x, &y),
            Op::AbstractApplyAll => self.abstract_apply_all(desc, x, y),
            Op::AbstractPack => self.abstract_pack(x),
            Op::AbstractEntries => self.abstract_entries(desc, x, y),
            Op::Productive => self.productive(desc),
            Op::ProductiveSummary => self.productive_summary(desc),
            Op::AbstractProductive => self.abstract_boolean_field(desc, true),
            Op::AbstractRowOccupied => self.abstract_boolean_field(desc, false),
            Op::AliasBody => self.alias_body(desc),
            Op::KnownNode => self.known_node(desc),
            Op::Resolve => self.resolve(desc),
            Op::Spine => self.spine(desc, &x),
            Op::Head => self.head(desc, x, y),
            Op::Row => self.row(desc),
            Op::Merge | Op::Insert | Op::Filter | Op::HasLabel | Op::HasEntry => {
                self.row_list(op, desc, x, y)
            }
            Op::Equal => self.equal(desc, x),
            Op::Tag | Op::Children => self.metadata(op, desc),
            Op::EqualChildren => self.equal_children(desc, x, y),
            Op::Atom => self.atom(desc, &x),
            Op::Scope => self.scope(desc),
            Op::Dictionary => self.dictionary(desc),
            Op::Adapt => self.adapt(desc, &x),
            Op::IsSymbol => self.bind(self.observe(Op::Resolve, desc), |this, resolved| {
                this.nodes_with_default(
                    resolved,
                    |this, _| this.boolean(false),
                    |this, _, node, _| {
                        matches!(node.kind, OwnerTypeKind::Symbol(_)).then(|| this.boolean(true))
                    },
                )
            }),
        }
    }
}

include!("owner_descriptors/resolution.rs");
include!("owner_descriptors/rows.rs");
include!("owner_descriptors/heads.rs");
include!("owner_descriptors/adaptation.rs");
include!("owner_descriptors/abstract_application.rs");
include!("owner_descriptors/abstract_types.rs");
include!("owner_descriptors/productivity.rs");

#[cfg(test)]
#[path = "owner_descriptors/tests.rs"]
mod sharing_tests;
