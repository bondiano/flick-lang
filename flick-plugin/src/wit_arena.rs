//! The arena of a plugin's code to and from the types of
//! `wit/plugin.wit`: part of the module [`wit_conversions!`] writes, kept
//! apart for its size.
//!
//! [`wit_conversions!`]: crate::wit_conversions

/// `to_arena` and `from_arena`, where `w`, `to_span` and `from_span` of
/// [`wit_conversions!`](crate::wit_conversions) are in scope.
#[macro_export]
#[doc(hidden)]
macro_rules! wit_arena_conversions {
    () => {
        #[allow(clippy::too_many_lines)]
        pub fn to_arena(a: $crate::Arena) -> w::Arena {
            let nodes = a
                .nodes
                .into_iter()
                .map(|n| w::Node {
                    kind: match n.kind {
                        $crate::Kind::Expr(e) => w::Kind::Expr(match e {
                            $crate::Expr::Name(s) => w::Expr::Name(s),
                            $crate::Expr::Number(n) => w::Expr::Number(n),
                            $crate::Expr::Text(s) => w::Expr::Text(s),
                            $crate::Expr::Boolean(b) => w::Expr::Boolean(b),
                            $crate::Expr::Unit => w::Expr::Unit,
                            $crate::Expr::Apply {
                                function,
                                arguments,
                            } => w::Expr::Apply(w::Apply {
                                function,
                                arguments,
                            }),
                            $crate::Expr::Lambda { params, body } => {
                                w::Expr::Lambda(w::Lambda { params, body })
                            }
                            $crate::Expr::Record(f) => w::Expr::Record(to_fields(f)),
                            $crate::Expr::Get { record, label } => {
                                w::Expr::Get(w::Get { record, label })
                            }
                            $crate::Expr::Items(i) => w::Expr::Items(i),
                            $crate::Expr::Tuple(i) => w::Expr::Tuple(i),
                            $crate::Expr::Concat(i) => w::Expr::Concat(i),
                            $crate::Expr::Annot { expr, ty } => {
                                w::Expr::Annot(w::Annot { expr, ty })
                            }
                            $crate::Expr::Hole(s) => w::Expr::Hole(to_span(s)),
                            $crate::Expr::Match { scrutinee, arms } => w::Expr::Match(w::MatchOn {
                                scrutinee,
                                arms: (arms.into_iter())
                                    .map(|a| w::Arm {
                                        pattern: a.pattern,
                                        guard: a.guard,
                                        body: a.body,
                                    })
                                    .collect(),
                            }),
                            $crate::Expr::If {
                                condition,
                                then,
                                otherwise,
                            } => w::Expr::If(w::Branches {
                                condition,
                                then,
                                otherwise,
                            }),
                            $crate::Expr::Block { statements, result } => {
                                w::Expr::Block(w::Statements {
                                    items: statements,
                                    result,
                                })
                            }
                        }),
                        $crate::Kind::Ty(t) => w::Kind::Ty(match t {
                            $crate::Ty::Name(s) => w::Ty::Name(s),
                            $crate::Ty::Apply {
                                function,
                                arguments,
                            } => w::Ty::Apply(w::Apply {
                                function,
                                arguments,
                            }),
                            $crate::Ty::Record(f) => w::Ty::Record(to_fields(f)),
                            $crate::Ty::Forall { vars, body } => {
                                w::Ty::Forall(w::ForallTy { vars, body })
                            }
                            $crate::Ty::Row(entries) => w::Ty::Row(entries),
                            $crate::Ty::Signed(s) => w::Ty::Signed(to_span(s)),
                            $crate::Ty::Tuple(items) => w::Ty::Tuple(items),
                            $crate::Ty::Function { input, output } => {
                                w::Ty::Function(w::Arrow { input, output })
                            }
                            $crate::Ty::Text(s) => w::Ty::Text(s),
                            $crate::Ty::Wildcard => w::Ty::Wildcard,
                            $crate::Ty::Hole(s) => w::Ty::Hole(to_span(s)),
                        }),
                        $crate::Kind::Pat(q) => w::Kind::Pat(match q {
                            $crate::Pat::Var(s) => w::Pat::Var(s),
                            $crate::Pat::Wildcard => w::Pat::Wildcard,
                            $crate::Pat::Con { name, arguments } => {
                                w::Pat::Con(w::ConPat { name, arguments })
                            }
                            $crate::Pat::Record(f) => w::Pat::Record(to_fields(f)),
                            $crate::Pat::Tuple(items) => w::Pat::Tuple(items),
                            $crate::Pat::Hole(s) => w::Pat::Hole(to_span(s)),
                        }),
                        $crate::Kind::Decl(d) => w::Kind::Decl(match d {
                            $crate::Decl::Let {
                                name,
                                params,
                                ty,
                                body,
                                rec,
                            } => w::Decl::Let(w::LetDecl {
                                name,
                                params,
                                ty,
                                body,
                                rec,
                            }),
                            $crate::Decl::Signature { name, ty } => {
                                w::Decl::Signature(w::Signature { name, ty })
                            }
                            $crate::Decl::Alias { name, params, ty } => {
                                w::Decl::Alias(w::Alias { name, params, ty })
                            }
                            $crate::Decl::Adt {
                                name,
                                params,
                                cases,
                                deriving,
                            } => w::Decl::Adt(w::Adt {
                                name,
                                params,
                                cases: cases
                                    .into_iter()
                                    .map(|c| w::Case {
                                        name: c.name,
                                        fields: c.fields,
                                    })
                                    .collect(),
                                deriving,
                            }),
                            $crate::Decl::Instance {
                                context,
                                head,
                                members,
                            } => w::Decl::Instance(w::Instance {
                                context,
                                head,
                                members,
                            }),
                            $crate::Decl::Group(d) => w::Decl::Group(d),
                            $crate::Decl::Hole(s) => w::Decl::Hole(to_span(s)),
                            $crate::Decl::Doc { text, decl } => w::Decl::Doc(w::Doc { text, decl }),
                        }),
                    },
                    origin: to_span(n.origin),
                })
                .collect();
            w::Arena {
                nodes,
                root: a.root,
            }
        }

        #[allow(clippy::too_many_lines)]
        pub fn from_arena(a: w::Arena) -> $crate::Arena {
            let nodes = a
                .nodes
                .into_iter()
                .map(|n| $crate::Node {
                    kind: match n.kind {
                        w::Kind::Expr(e) => $crate::Kind::Expr(match e {
                            w::Expr::Name(s) => $crate::Expr::Name(s),
                            w::Expr::Number(n) => $crate::Expr::Number(n),
                            w::Expr::Text(s) => $crate::Expr::Text(s),
                            w::Expr::Boolean(b) => $crate::Expr::Boolean(b),
                            w::Expr::Unit => $crate::Expr::Unit,
                            w::Expr::Apply(a) => $crate::Expr::Apply {
                                function: a.function,
                                arguments: a.arguments,
                            },
                            w::Expr::Lambda(l) => $crate::Expr::Lambda {
                                params: l.params,
                                body: l.body,
                            },
                            w::Expr::Record(f) => $crate::Expr::Record(from_fields(f)),
                            w::Expr::Get(g) => $crate::Expr::Get {
                                record: g.record,
                                label: g.label,
                            },
                            w::Expr::Items(i) => $crate::Expr::Items(i),
                            w::Expr::Tuple(i) => $crate::Expr::Tuple(i),
                            w::Expr::Concat(i) => $crate::Expr::Concat(i),
                            w::Expr::Annot(a) => $crate::Expr::Annot {
                                expr: a.expr,
                                ty: a.ty,
                            },
                            w::Expr::Hole(s) => $crate::Expr::Hole(from_span(s)),
                            w::Expr::Match(m) => $crate::Expr::Match {
                                scrutinee: m.scrutinee,
                                arms: (m.arms.into_iter())
                                    .map(|a| $crate::Arm {
                                        pattern: a.pattern,
                                        guard: a.guard,
                                        body: a.body,
                                    })
                                    .collect(),
                            },
                            w::Expr::If(b) => $crate::Expr::If {
                                condition: b.condition,
                                then: b.then,
                                otherwise: b.otherwise,
                            },
                            w::Expr::Block(s) => $crate::Expr::Block {
                                statements: s.items,
                                result: s.result,
                            },
                        }),
                        w::Kind::Ty(t) => $crate::Kind::Ty(match t {
                            w::Ty::Name(s) => $crate::Ty::Name(s),
                            w::Ty::Apply(a) => $crate::Ty::Apply {
                                function: a.function,
                                arguments: a.arguments,
                            },
                            w::Ty::Record(f) => $crate::Ty::Record(from_fields(f)),
                            w::Ty::Function(a) => $crate::Ty::Function {
                                input: a.input,
                                output: a.output,
                            },
                            w::Ty::Forall(f) => $crate::Ty::Forall {
                                vars: f.vars,
                                body: f.body,
                            },
                            w::Ty::Row(entries) => $crate::Ty::Row(entries),
                            w::Ty::Signed(s) => $crate::Ty::Signed(from_span(s)),
                            w::Ty::Tuple(items) => $crate::Ty::Tuple(items),
                            w::Ty::Text(s) => $crate::Ty::Text(s),
                            w::Ty::Wildcard => $crate::Ty::Wildcard,
                            w::Ty::Hole(s) => $crate::Ty::Hole(from_span(s)),
                        }),
                        w::Kind::Pat(q) => $crate::Kind::Pat(match q {
                            w::Pat::Var(s) => $crate::Pat::Var(s),
                            w::Pat::Wildcard => $crate::Pat::Wildcard,
                            w::Pat::Con(c) => $crate::Pat::Con {
                                name: c.name,
                                arguments: c.arguments,
                            },
                            w::Pat::Record(f) => $crate::Pat::Record(from_fields(f)),
                            w::Pat::Tuple(items) => $crate::Pat::Tuple(items),
                            w::Pat::Hole(s) => $crate::Pat::Hole(from_span(s)),
                        }),
                        w::Kind::Decl(d) => $crate::Kind::Decl(match d {
                            w::Decl::Let(l) => $crate::Decl::Let {
                                name: l.name,
                                params: l.params,
                                ty: l.ty,
                                body: l.body,
                                rec: l.rec,
                            },
                            w::Decl::Signature(s) => $crate::Decl::Signature {
                                name: s.name,
                                ty: s.ty,
                            },
                            w::Decl::Alias(a) => $crate::Decl::Alias {
                                name: a.name,
                                params: a.params,
                                ty: a.ty,
                            },
                            w::Decl::Adt(a) => $crate::Decl::Adt {
                                name: a.name,
                                params: a.params,
                                cases: a
                                    .cases
                                    .into_iter()
                                    .map(|c| $crate::Case {
                                        name: c.name,
                                        fields: c.fields,
                                    })
                                    .collect(),
                                deriving: a.deriving,
                            },
                            w::Decl::Instance(i) => $crate::Decl::Instance {
                                context: i.context,
                                head: i.head,
                                members: i.members,
                            },
                            w::Decl::Group(d) => $crate::Decl::Group(d),
                            w::Decl::Hole(s) => $crate::Decl::Hole(from_span(s)),
                            w::Decl::Doc(d) => $crate::Decl::Doc {
                                text: d.text,
                                decl: d.decl,
                            },
                        }),
                    },
                    origin: from_span(n.origin),
                })
                .collect();
            $crate::Arena {
                nodes,
                root: a.root,
            }
        }
    };
}
