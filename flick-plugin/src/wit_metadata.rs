//! WIT conversions for static plugin metadata, derive targets, and written
//! type trees. Expanded inside `wit_conversions!` alongside the arena and
//! call-site helpers.

#[macro_export]
#[doc(hidden)]
macro_rules! wit_metadata_conversions {
    () => {
        pub fn to_metadata(m: &$crate::PluginMetadata) -> w::PluginMetadata {
            w::PluginMetadata {
                heads: m
                    .heads
                    .iter()
                    .map(|h| w::HeadSpec {
                        head: h.head.clone(),
                        module: h.module.clone(),
                        bare: h.bare,
                        declarations: h.declarations.clone(),
                        named: h.named,
                        top: h.top,
                        bracket_args: h.bracket_args,
                    })
                    .collect(),
                watch: m.watch.clone(),
                derives: m.derives.clone(),
                own_codes: m.own_codes.clone(),
            }
        }

        pub fn from_metadata(m: w::PluginMetadata) -> $crate::PluginMetadata {
            $crate::PluginMetadata {
                heads: m
                    .heads
                    .into_iter()
                    .map(|h| $crate::HeadSpec {
                        head: h.head,
                        module: h.module,
                        bare: h.bare,
                        declarations: h.declarations,
                        named: h.named,
                        top: h.top,
                        bracket_args: h.bracket_args,
                    })
                    .collect(),
                watch: m.watch,
                derives: m.derives,
                own_codes: m.own_codes,
            }
        }

        fn to_visibility(v: $crate::Visibility) -> w::Visibility {
            match v {
                $crate::Visibility::PublicConstructors => w::Visibility::PublicConstructors,
                $crate::Visibility::Opaque => w::Visibility::Opaque,
                $crate::Visibility::Private => w::Visibility::Private,
            }
        }

        fn from_visibility(v: w::Visibility) -> $crate::Visibility {
            match v {
                w::Visibility::PublicConstructors => $crate::Visibility::PublicConstructors,
                w::Visibility::Opaque => $crate::Visibility::Opaque,
                w::Visibility::Private => $crate::Visibility::Private,
            }
        }

        fn to_attribute(a: &$crate::AttributeMeta) -> w::AttributeMeta {
            w::AttributeMeta {
                name: a.name.clone(),
                args: a.args.clone(),
                span: to_span(a.span),
            }
        }

        fn from_attribute(a: w::AttributeMeta) -> $crate::AttributeMeta {
            $crate::AttributeMeta {
                name: a.name,
                args: a.args,
                span: from_span(a.span),
            }
        }

        fn to_constraint(c: &$crate::ConstraintMeta) -> w::ConstraintMeta {
            w::ConstraintMeta {
                class: c.class.clone(),
                args: c.args.iter().map(to_written).collect(),
                span: to_span(c.span),
            }
        }

        fn from_constraint(c: w::ConstraintMeta) -> $crate::ConstraintMeta {
            $crate::ConstraintMeta {
                class: c.class,
                args: c
                    .args
                    .into_iter()
                    .map(|a| {
                        let span = a.last().map_or_else(Default::default, |n| $crate::Span {
                            start: n.start,
                            end: n.end,
                        });
                        from_written(a).unwrap_or($crate::Written {
                            kind: $crate::WrittenKind::Other,
                            span,
                        })
                    })
                    .collect(),
                span: from_span(c.span),
            }
        }

        fn to_literal_value(v: &$crate::LiteralValue) -> w::LiteralValue {
            match v {
                $crate::LiteralValue::String(s) => w::LiteralValue::String(s.clone()),
                $crate::LiteralValue::Int(i) => w::LiteralValue::Int(i.clone()),
            }
        }

        fn from_literal_value(v: w::LiteralValue) -> $crate::LiteralValue {
            match v {
                w::LiteralValue::String(s) => $crate::LiteralValue::String(s),
                w::LiteralValue::Int(i) => $crate::LiteralValue::Int(i),
            }
        }

        pub fn to_target(t: &$crate::DeriveTarget) -> w::DeriveTarget {
            w::DeriveTarget {
                decl: to_block(&t.decl),
                name: t.name.clone(),
                name_span: to_span(t.name_span),
                params: t.params.clone(),
                param_spans: t.param_spans.iter().copied().map(to_span).collect(),
                kinds: t.kinds.clone(),
                cases: t
                    .cases
                    .iter()
                    .map(|c| w::CaseTarget {
                        name: c.name.clone(),
                        span: to_span(c.span),
                        name_span: to_span(c.name_span),
                        docs: c.docs.clone(),
                        attributes: c.attributes.iter().map(to_attribute).collect(),
                        constraints: c.constraints.iter().map(to_constraint).collect(),
                        fields: c.fields.iter().map(to_written).collect(),
                        result: c.result.as_ref().map(to_written),
                        literal: c.literal.map(to_span),
                        literal_value: c.literal_value.as_ref().map(to_literal_value),
                        refined: c.refined,
                        constrained: c.constrained,
                    })
                    .collect(),
                alias: t.alias.as_ref().map(to_written),
                wraps: t.wraps.as_ref().map(to_written),
                modifiers: t.modifiers.clone(),
                deriving: t.deriving.clone(),
                deriving_spans: t.deriving_spans.iter().copied().map(to_span).collect(),
                visibility: to_visibility(t.visibility),
                attributes: t.attributes.iter().map(to_attribute).collect(),
                local: t.local,
            }
        }

        pub fn from_target(t: w::DeriveTarget) -> $crate::DeriveTarget {
            $crate::DeriveTarget {
                decl: from_block(t.decl),
                name: t.name,
                name_span: from_span(t.name_span),
                params: t.params,
                param_spans: t.param_spans.into_iter().map(from_span).collect(),
                kinds: t.kinds,
                cases: t
                    .cases
                    .into_iter()
                    .map(|c| $crate::CaseTarget {
                        name: c.name,
                        span: from_span(c.span),
                        name_span: from_span(c.name_span),
                        docs: c.docs,
                        attributes: c.attributes.into_iter().map(from_attribute).collect(),
                        constraints: c.constraints.into_iter().map(from_constraint).collect(),
                        // an index out of order is a type nothing reads
                        fields: c
                            .fields
                            .into_iter()
                            .map(|f| {
                                let span =
                                    f.last().map_or_else(Default::default, |n| $crate::Span {
                                        start: n.start,
                                        end: n.end,
                                    });
                                from_written(f).unwrap_or($crate::Written {
                                    kind: $crate::WrittenKind::Other,
                                    span,
                                })
                            })
                            .collect(),
                        result: c.result.and_then(from_written),
                        literal: c.literal.map(from_span),
                        literal_value: c.literal_value.map(from_literal_value),
                        refined: c.refined,
                        constrained: c.constrained,
                    })
                    .collect(),
                alias: t.alias.and_then(from_written),
                wraps: t.wraps.and_then(from_written),
                modifiers: t.modifiers,
                deriving: t.deriving,
                deriving_spans: t.deriving_spans.into_iter().map(from_span).collect(),
                visibility: from_visibility(t.visibility),
                attributes: t.attributes.into_iter().map(from_attribute).collect(),
                local: t.local,
            }
        }

        fn to_written_span(s: $crate::Span) -> w::WrittenSpan {
            w::WrittenSpan {
                start: s.start,
                end: s.end,
            }
        }

        fn from_written_span(s: w::WrittenSpan) -> $crate::Span {
            $crate::Span {
                start: s.start,
                end: s.end,
            }
        }

        fn to_written(t: &$crate::Written) -> w::Written {
            t.flatten()
                .into_iter()
                .map(|n| w::WrittenNode {
                    start: n.span.start,
                    end: n.span.end,
                    kind: match n.kind {
                        $crate::FlatKind::Con { name, args } => {
                            w::WrittenKind::Con(w::WrittenName { name, args })
                        }
                        $crate::FlatKind::Var { name, args } => {
                            w::WrittenKind::Var(w::WrittenName { name, args })
                        }
                        $crate::FlatKind::Record { fields, open } => {
                            w::WrittenKind::Record(w::WrittenRecord {
                                fields: fields
                                    .into_iter()
                                    .map(|f| w::WrittenRecordField {
                                        name: f.name,
                                        name_span: to_written_span(f.name_span),
                                        span: to_written_span(f.span),
                                        ty: f.ty,
                                        docs: f.docs,
                                        source_index: f.source_index,
                                    })
                                    .collect(),
                                open,
                            })
                        }
                        $crate::FlatKind::Tuple(items) => w::WrittenKind::Tuple(items),
                        $crate::FlatKind::Unit => w::WrittenKind::Unit,
                        $crate::FlatKind::Arrow { from, to, row } => {
                            w::WrittenKind::Arrow(w::WrittenArrow {
                                takes: from,
                                gives: to,
                                row: row.map(|r| w::WrittenRow {
                                    services: r.services,
                                    services_open: r.services_open,
                                    errors: r.errors,
                                    errors_open: r.errors_open,
                                }),
                            })
                        }
                        $crate::FlatKind::Captured(of) => w::WrittenKind::Captured(of),
                        $crate::FlatKind::Forall {
                            vars,
                            constraints,
                            body,
                        } => w::WrittenKind::Forall(w::WrittenForall {
                            vars,
                            constraints: constraints
                                .into_iter()
                                .map(|c| w::WrittenConstraint {
                                    class: c.class,
                                    args: c.args,
                                    span: to_written_span(c.span),
                                })
                                .collect(),
                            body,
                        }),
                        $crate::FlatKind::StringLit(s) => w::WrittenKind::StringLit(s),
                        $crate::FlatKind::None => w::WrittenKind::None,
                        $crate::FlatKind::Other => w::WrittenKind::Other,
                    },
                })
                .collect()
        }

        fn from_written(nodes: w::Written) -> Option<$crate::Written> {
            let flat: Vec<$crate::FlatWritten> = nodes
                .into_iter()
                .map(|n| $crate::FlatWritten {
                    span: $crate::Span {
                        start: n.start,
                        end: n.end,
                    },
                    kind: match n.kind {
                        w::WrittenKind::Con(n) => $crate::FlatKind::Con {
                            name: n.name,
                            args: n.args,
                        },
                        w::WrittenKind::Var(n) => $crate::FlatKind::Var {
                            name: n.name,
                            args: n.args,
                        },
                        w::WrittenKind::Record(r) => $crate::FlatKind::Record {
                            fields: r
                                .fields
                                .into_iter()
                                .map(|f| $crate::FlatRecordField {
                                    name: f.name,
                                    name_span: from_written_span(f.name_span),
                                    span: from_written_span(f.span),
                                    ty: f.ty,
                                    docs: f.docs,
                                    source_index: f.source_index,
                                })
                                .collect(),
                            open: r.open,
                        },
                        w::WrittenKind::Tuple(items) => $crate::FlatKind::Tuple(items),
                        w::WrittenKind::Unit => $crate::FlatKind::Unit,
                        w::WrittenKind::Arrow(a) => $crate::FlatKind::Arrow {
                            from: a.takes,
                            to: a.gives,
                            row: a.row.map(|r| $crate::FlatRow {
                                services: r.services,
                                services_open: r.services_open,
                                errors: r.errors,
                                errors_open: r.errors_open,
                            }),
                        },
                        w::WrittenKind::Captured(of) => $crate::FlatKind::Captured(of),
                        w::WrittenKind::Forall(f) => $crate::FlatKind::Forall {
                            vars: f.vars,
                            constraints: f
                                .constraints
                                .into_iter()
                                .map(|c| $crate::FlatConstraint {
                                    class: c.class,
                                    args: c.args,
                                    span: from_written_span(c.span),
                                })
                                .collect(),
                            body: f.body,
                        },
                        w::WrittenKind::StringLit(s) => $crate::FlatKind::StringLit(s),
                        w::WrittenKind::None => $crate::FlatKind::None,
                        w::WrittenKind::Other => $crate::FlatKind::Other,
                    },
                })
                .collect();
            $crate::Written::unflatten(&flat)
        }
    };
}
