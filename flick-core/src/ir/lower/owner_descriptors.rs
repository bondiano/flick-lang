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
use std::collections::HashSet;

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
    AbstractAlias,
    HeadAlias,
}

struct Build<'a> {
    templates: &'a OwnerTypeTemplates,
    run: String,
    temporaries: String,
    impossible: String,
    span: Span,
    next: usize,
    alias_groups: Vec<usize>,
    alias_representatives: Vec<usize>,
    head_groups: Vec<usize>,
    head_representatives: Vec<usize>,
}

impl Lower<'_> {
    pub(super) fn owner_descriptor(&mut self, value: Expr, templates: &OwnerTypeTemplates) -> Expr {
        assert!(
            !templates.has_meta_views(),
            "owner templates must be finalized before lowering"
        );
        let run = self.hidden("$ownerTypes");
        // The dispatcher keeps its descriptive debug name. Its numerous local
        // temporaries use a short unique prefix in emitted code and wire IR.
        let temporaries = self.hidden("$ot");
        let (alias_groups, alias_representatives) = alias_groups(templates);
        let mut head_representatives: Vec<usize> = Vec::new();
        let head_groups = templates
            .aliases
            .iter()
            .enumerate()
            .map(|(alias, definition)| {
                head_representatives
                    .iter()
                    .position(|&other| templates.aliases[other].params == definition.params)
                    .unwrap_or_else(|| {
                        head_representatives.push(alias);
                        head_representatives.len() - 1
                    })
            })
            .collect();
        let mut build = Build {
            templates,
            impossible: format!("{run}$invalid"),
            run,
            temporaries,
            span: value.span,
            next: 0,
            alias_groups,
            alias_representatives,
            head_groups,
            head_representatives,
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
        match c.kind {
            ExprKind::Lit(Lit::Bool(true)) => a,
            ExprKind::Lit(Lit::Bool(false)) => b,
            _ => self.e(ExprKind::If(Box::new(c), Box::new(a), Box::new(b))),
        }
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
                binder_types: Vec::new(),
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
        // Descriptor tables use distinct integer keys and a final fallback.
        // Identical results can share one arm without changing which key wins.
        let mut keys = HashSet::new();
        let table = arms.iter().enumerate().all(|(index, (pat, _))| match pat {
            Pat::Lit(Lit::Int(key)) => keys.insert(*key),
            Pat::Wild => index + 1 == arms.len(),
            _ => false,
        });
        let mut shared: Vec<(Pat, Expr)> = Vec::new();
        for (pat, body) in arms {
            if table
                && matches!(&pat, Pat::Lit(Lit::Int(_)))
                && let Some((previous, _)) = shared.iter_mut().find(|(previous, result)| {
                    matches!(previous, Pat::Lit(Lit::Int(_)) | Pat::Or(_)) && *result == body
                })
            {
                match previous {
                    Pat::Or(alternatives) => alternatives.push(pat),
                    _ => *previous = Pat::Or(vec![previous.clone(), pat]),
                }
            } else {
                shared.push((pat, body));
            }
        }
        // These private dispatchers take an Int index into a complete dense
        // table. A bounds check preserves the invalid-index branch, while the
        // most common result becomes the default inside that checked domain.
        // Only locals are eligible, so the scrutinee is still evaluated once.
        let dense = table
            && matches!(value.kind, ExprKind::Local(_))
            && (0..keys.len())
                .all(|index| keys.contains(&i32::try_from(index).expect("owner table index")));
        let default = shared
            .iter()
            .enumerate()
            .filter_map(|(index, (pat, _))| {
                if let Pat::Or(keys) = pat {
                    (keys.len() >= 4).then_some((index, keys.len()))
                } else {
                    None
                }
            })
            .max_by_key(|(_, count)| *count);
        let bounds = if dense
            && let Some((index, _)) = default
            && shared
                .last()
                .is_some_and(|(pat, _)| matches!(pat, Pat::Wild))
        {
            let (_, invalid) = shared.pop().expect("checked fallback");
            let (_, body) = shared.remove(index);
            shared.push((Pat::Wild, body));
            Some((
                self.both(
                    self.binary(BinOp::Ge, value.clone(), self.int(0)),
                    self.binary(BinOp::Lt, value.clone(), self.int(keys.len())),
                ),
                invalid,
            ))
        } else {
            None
        };
        let matched = self.e(ExprKind::Match(
            Box::new(value),
            shared
                .into_iter()
                .map(|(pat, body)| Arm {
                    pat,
                    binder_types: Vec::new(),
                    guard: None,
                    body,
                })
                .collect(),
        ));
        match bounds {
            Some((valid, invalid)) => self.choose(valid, matched, invalid),
            None => matched,
        }
    }
    fn name(&mut self) -> String {
        let name = format!("{}${}", self.temporaries, self.next);
        self.next += 1;
        name
    }
    fn bind(&mut self, value: Expr, body: impl FnOnce(&mut Self, Expr) -> Expr) -> Expr {
        let name = self.name();
        let result = body(self, self.local(&name));
        self.block(vec![Stmt::let_bind(Pat::Bind(name), value)], result)
    }
    fn unpack(&self, value: Expr, names: &[&str], body: Expr) -> Expr {
        self.block(
            vec![Stmt::let_bind(
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
            Op::AbstractAlias,
            Op::HeadAlias,
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
                    type_params: Vec::new(),
                    dicts: Vec::new(),
                    dict_constraints: Vec::new(),
                    ty: None,
                    value: self.lambda(
                        Pat::Wild,
                        Stage::Pure,
                        self.pure(self.local(&self.impossible), self.unit()),
                    ),
                },
                Stmt::Fun {
                    name: self.run.clone(),
                    rec: true,
                    type_params: Vec::new(),
                    dicts: Vec::new(),
                    dict_constraints: Vec::new(),
                    ty: None,
                    value: function,
                },
                Stmt::let_bind(Pat::Bind(raw), value),
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
        body: impl Fn(&mut Self, usize, &OwnerTypeExpr, Expr, Expr) -> Expr,
    ) -> Expr {
        self.bind(desc, |this, current| {
            let node = this.name();
            let env = this.name();
            let templates = this.templates;
            let arms = templates
                .exprs
                .iter()
                .enumerate()
                .map(|(id, expression)| {
                    (
                        Pat::Lit(Lit::Int(i32::try_from(id).expect("owner syntax index"))),
                        body(this, id, expression, this.local(&env), current.clone()),
                    )
                })
                .collect();
            this.unpack(current, &[&node, &env], this.case(this.local(&node), arms))
        })
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
            Op::HeadAlias => self.head_alias(&desc, &x, y),
            Op::AbstractAlias => {
                let representatives = self.alias_representatives.clone();
                let mut arms: Vec<_> = representatives
                    .into_iter()
                    .enumerate()
                    .map(|(group, alias)| {
                        (
                            Pat::Lit(Lit::Int(i32::try_from(group).expect("alias summary group"))),
                            self.abstract_alias(alias, desc.clone(), x.clone()),
                        )
                    })
                    .collect();
                arms.push((Pat::Wild, self.impossible()));
                self.case(y, arms)
            }
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
                this.nodes(resolved, |this, _, node, _, _| {
                    this.boolean(matches!(node.kind, OwnerTypeKind::Symbol(_)))
                })
            }),
        }
    }
}

fn alias_groups(templates: &OwnerTypeTemplates) -> (Vec<usize>, Vec<usize>) {
    let certificate = templates
        .productivity
        .as_ref()
        .expect("owner productivity certificate");
    let mut rows = vec![Vec::new(); templates.aliases.len()];
    for row in &certificate.aliases {
        rows[row.alias].push((row.arguments.clone(), row.value));
    }
    for row in &mut rows {
        row.sort();
    }
    let mut groups = Vec::new();
    let mut representatives: Vec<usize> = Vec::new();
    for (alias, definition) in templates.aliases.iter().enumerate() {
        let group = representatives
            .iter()
            .position(|&other| {
                templates.aliases[other].params.len() == definition.params.len()
                    && rows[other] == rows[alias]
            })
            .unwrap_or_else(|| {
                representatives.push(alias);
                representatives.len() - 1
            });
        groups.push(group);
    }
    (groups, representatives)
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
