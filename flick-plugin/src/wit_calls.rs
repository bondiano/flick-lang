//! The places of a module a plugin is told of to and from the types of
//! `wit/plugin.wit`: part of the module [`wit_conversions!`] writes, kept
//! apart for its size.
//!
//! [`wit_conversions!`]: crate::wit_conversions

/// `to_call_site` and `from_call_site`, where `w`, `to_span`, `from_span`,
/// `to_type` and `from_type` of [`wit_conversions!`](crate::wit_conversions)
/// are in scope.
#[macro_export]
#[doc(hidden)]
macro_rules! wit_call_conversions {
    () => {
        pub fn to_call_site(c: &$crate::CallSite) -> w::CallSite {
            w::CallSite {
                module: c.module.clone(),
                name: c.name.clone(),
                span: to_span(c.span),
                arguments: c.arguments.iter().map(to_argument).collect(),
                refinement: c.refinement.as_ref().map(to_refinement),
                binding: c.binding.as_ref().map(|b| w::Binding {
                    module: b.module.clone(),
                    name: b.name.clone(),
                    span: to_span(b.span),
                    constrained: b.constrained,
                    in_function: b.in_function,
                    applied: b.applied,
                    gives: b.gives.as_ref().map(to_type),
                }),
                generated: c.generated,
                tagged: c.tagged.as_ref().map(|t| w::Tagged {
                    tag: t.tag.clone(),
                    named: t.named.clone(),
                    reaches: t.reaches.clone(),
                    inside: t
                        .inside
                        .iter()
                        .map(|f| w::Enclosing {
                            services: f.services.clone(),
                            open: f.open,
                        })
                        .collect(),
                }),
                lacking: c.lacking.as_ref().map(|l| w::Lacking {
                    allowed: w::Enclosing {
                        services: l.allowed.services.clone(),
                        open: l.allowed.open,
                    },
                    what: l.what.clone(),
                    services: l.services.clone(),
                    labels: l.labels.clone(),
                    errors: l.errors.clone(),
                }),
            }
        }

        pub fn from_call_site(c: w::CallSite) -> $crate::CallSite {
            $crate::CallSite {
                module: c.module,
                name: c.name,
                span: from_span(c.span),
                arguments: c.arguments.into_iter().map(from_argument).collect(),
                refinement: c.refinement.map(from_refinement),
                binding: c.binding.map(|b| $crate::Binding {
                    module: b.module,
                    name: b.name,
                    span: from_span(b.span),
                    constrained: b.constrained,
                    in_function: b.in_function,
                    applied: b.applied,
                    gives: b.gives.map(from_type),
                }),
                generated: c.generated,
                tagged: c.tagged.map(|t| $crate::Tagged {
                    tag: t.tag,
                    named: t.named,
                    reaches: t.reaches,
                    inside: t
                        .inside
                        .into_iter()
                        .map(|f| $crate::Enclosing {
                            services: f.services,
                            open: f.open,
                        })
                        .collect(),
                }),
                lacking: c.lacking.map(|l| $crate::Lacking {
                    allowed: $crate::Enclosing {
                        services: l.allowed.services,
                        open: l.allowed.open,
                    },
                    what: l.what,
                    services: l.services,
                    labels: l.labels,
                    errors: l.errors,
                }),
            }
        }

        fn to_type(t: &$crate::FactType) -> w::FactType {
            w::FactType {
                shown: t.shown.clone(),
                exact: t.exact.clone(),
                literal_of: t.literal_of.clone(),
            }
        }

        fn from_type(t: w::FactType) -> $crate::FactType {
            $crate::FactType {
                shown: t.shown,
                exact: t.exact,
                literal_of: t.literal_of,
            }
        }

        fn to_argument(a: &$crate::Argument) -> w::Argument {
            let ty = to_type;
            w::Argument {
                span: to_span(a.span),
                ty: ty(&a.ty),
                fields: a.fields.as_ref().map(|fs| {
                    fs.iter()
                        .map(|x| w::FactField {
                            label: x.label.clone(),
                            ty: ty(&x.ty),
                        })
                        .collect()
                }),
                constant: a.constant.as_ref().map(to_constant),
                lambda: a.lambda,
            }
        }

        fn from_argument(a: w::Argument) -> $crate::Argument {
            let ty = from_type;
            $crate::Argument {
                span: from_span(a.span),
                ty: ty(a.ty),
                fields: a.fields.map(|fs| {
                    fs.into_iter()
                        .map(|x| $crate::FactField {
                            label: x.label,
                            ty: ty(x.ty),
                        })
                        .collect()
                }),
                constant: a.constant.map(from_constant),
                lambda: a.lambda,
            }
        }

        fn to_constant(c: &$crate::Constant) -> w::Constant {
            match c {
                $crate::Constant::Number(n) => w::Constant::Number(*n),
                $crate::Constant::Text(t) => w::Constant::Text(t.clone()),
                $crate::Constant::Boolean(b) => w::Constant::Boolean(*b),
            }
        }

        fn from_constant(c: w::Constant) -> $crate::Constant {
            match c {
                w::Constant::Number(n) => $crate::Constant::Number(n),
                w::Constant::Text(t) => $crate::Constant::Text(t),
                w::Constant::Boolean(b) => $crate::Constant::Boolean(b),
            }
        }

        fn to_expression_kind(e: &$crate::ExpressionKind) -> w::ExpressionKind {
            use $crate::ExpressionKind as E;
            match e {
                E::Constant(c) => w::ExpressionKind::Constant(to_constant(c)),
                E::BigInt(n) => w::ExpressionKind::BigInt(n.clone()),
                E::Reference { module, name } => w::ExpressionKind::Reference(w::ReferenceFact {
                    module: module.clone(),
                    name: name.clone(),
                }),
                E::Variable(v) => w::ExpressionKind::Variable(v.clone()),
                E::Unary { op, value } => w::ExpressionKind::Unary(w::UnaryFact {
                    op: op.clone(),
                    value: *value,
                }),
                E::Binary { op, left, right } => w::ExpressionKind::Binary(w::BinaryFact {
                    op: op.clone(),
                    left: *left,
                    right: *right,
                }),
                E::If {
                    condition,
                    then,
                    otherwise,
                } => w::ExpressionKind::If(w::IfFact {
                    condition: *condition,
                    then: *then,
                    otherwise: *otherwise,
                }),
                E::Lambda { parameter, body } => w::ExpressionKind::Lambda(w::LambdaFact {
                    parameter: parameter.clone(),
                    body: *body,
                }),
                E::Unknown => w::ExpressionKind::Unknown,
            }
        }
        fn from_expression_kind(e: w::ExpressionKind) -> $crate::ExpressionKind {
            use $crate::ExpressionKind as E;
            match e {
                w::ExpressionKind::Constant(c) => E::Constant(from_constant(c)),
                w::ExpressionKind::BigInt(n) => E::BigInt(n),
                w::ExpressionKind::Reference(r) => E::Reference {
                    module: r.module,
                    name: r.name,
                },
                w::ExpressionKind::Variable(v) => E::Variable(v),
                w::ExpressionKind::Unary(u) => E::Unary {
                    op: u.op,
                    value: u.value,
                },
                w::ExpressionKind::Binary(b) => E::Binary {
                    op: b.op,
                    left: b.left,
                    right: b.right,
                },
                w::ExpressionKind::If(i) => E::If {
                    condition: i.condition,
                    then: i.then,
                    otherwise: i.otherwise,
                },
                w::ExpressionKind::Lambda(l) => E::Lambda {
                    parameter: l.parameter,
                    body: l.body,
                },
                w::ExpressionKind::Unknown => E::Unknown,
            }
        }

        fn to_refinement(r: &$crate::Refinement) -> w::Refinement {
            match r {
                $crate::Refinement::Literals(ls) => {
                    w::Refinement::Literals(ls.iter().map(to_constant).collect())
                }
                $crate::Refinement::Predicate(e) => w::Refinement::Predicate(w::ExpressionFact {
                    root: e.root,
                    nodes: e.nodes.iter().map(to_expression_kind).collect(),
                }),
            }
        }

        fn from_refinement(r: w::Refinement) -> $crate::Refinement {
            match r {
                w::Refinement::Literals(ls) => {
                    $crate::Refinement::Literals(ls.into_iter().map(from_constant).collect())
                }
                w::Refinement::Predicate(e) => {
                    $crate::Refinement::Predicate($crate::ExpressionFact {
                        root: e.root,
                        nodes: e
                            .nodes
                            .into_iter()
                            .map(from_expression_kind)
                            .collect::<::std::vec::Vec<_>>()
                            .into(),
                    })
                }
            }
        }
    };
}
