//! The types of this crate to and from the types bindings of
//! `wit/plugin.wit` generate: wit-bindgen's in a plugin, wasmtime's in the
//! compiler. Both name the types alike, so one macro writes both mappings.

/// Functions `to_*` and `from_*` between this crate's types and the types
/// of `wit/plugin.wit` in the module at the given path, in a module
/// `convert` where the macro is used.
#[macro_export]
#[doc(hidden)]
macro_rules! wit_conversions {
    ($($wit:ident)::+) => {
        #[allow(dead_code)]
        mod convert {
            use super::$($wit)::+ as w;

            pub fn to_span(s: $crate::Span) -> w::Span {
                w::Span { start: s.start, end: s.end }
            }

            pub fn from_span(s: w::Span) -> $crate::Span {
                $crate::Span { start: s.start, end: s.end }
            }

            pub fn to_block(b: &$crate::Block) -> w::Block {
                w::Block {
                    head: b.head.clone(),
                    text: b.text.clone(),
                    body: b.body,
                    place: match b.place {
                        $crate::Place::Expression => w::Place::Expression,
                        $crate::Place::Declaration => w::Place::Declaration,
                    },
                    tokens: b.tokens.iter().map(|t| w::Token {
                        kind: match t.kind {
                            $crate::TokenKind::Ident => w::TokenKind::Ident,
                            $crate::TokenKind::Con => w::TokenKind::Con,
                            $crate::TokenKind::Number => w::TokenKind::Number,
                            $crate::TokenKind::String => w::TokenKind::String,
                            $crate::TokenKind::Keyword => w::TokenKind::Keyword,
                            $crate::TokenKind::Punct => w::TokenKind::Punct,
                        },
                        span: to_span(t.span),
                    }).collect(),
                }
            }

            pub fn from_block(b: w::Block) -> $crate::Block {
                $crate::Block {
                    head: b.head,
                    text: b.text,
                    body: b.body,
                    place: match b.place {
                        w::Place::Expression => $crate::Place::Expression,
                        w::Place::Declaration => $crate::Place::Declaration,
                    },
                    tokens: b.tokens.into_iter().map(|t| $crate::Token {
                        kind: match t.kind {
                            w::TokenKind::Ident => $crate::TokenKind::Ident,
                            w::TokenKind::Con => $crate::TokenKind::Con,
                            w::TokenKind::Number => $crate::TokenKind::Number,
                            w::TokenKind::String => $crate::TokenKind::String,
                            w::TokenKind::Keyword => $crate::TokenKind::Keyword,
                            w::TokenKind::Punct => $crate::TokenKind::Punct,
                        },
                        span: from_span(t.span),
                    }).collect(),
                }
            }

            pub fn to_hole(h: $crate::Hole) -> w::Hole {
                w::Hole {
                    kind: match h.kind {
                        $crate::HoleKind::Expr => w::HoleKind::Expr,
                        $crate::HoleKind::Ty => w::HoleKind::Ty,
                        $crate::HoleKind::Pat => w::HoleKind::Pat,
                        $crate::HoleKind::Decl => w::HoleKind::Decl,
                    },
                    span: to_span(h.span),
                }
            }

            pub fn from_hole(h: w::Hole) -> $crate::Hole {
                $crate::Hole {
                    kind: match h.kind {
                        w::HoleKind::Expr => $crate::HoleKind::Expr,
                        w::HoleKind::Ty => $crate::HoleKind::Ty,
                        w::HoleKind::Pat => $crate::HoleKind::Pat,
                        w::HoleKind::Decl => $crate::HoleKind::Decl,
                    },
                    span: from_span(h.span),
                }
            }

            pub fn to_diagnostic(d: $crate::Diagnostic) -> w::Diagnostic {
                w::Diagnostic { message: d.message, span: to_span(d.span), code: d.code }
            }

            pub fn from_diagnostic(d: w::Diagnostic) -> $crate::Diagnostic {
                $crate::Diagnostic { message: d.message, span: from_span(d.span), code: d.code }
            }

            pub fn to_completion(c: $crate::Completion) -> w::Completion {
                w::Completion {
                    label: c.label,
                    detail: c.detail,
                    documentation: c.documentation,
                    snippet: c.snippet,
                }
            }

            pub fn from_completion(c: w::Completion) -> $crate::Completion {
                $crate::Completion {
                    label: c.label,
                    detail: c.detail,
                    documentation: c.documentation,
                    snippet: c.snippet,
                }
            }

            pub fn to_highlight(h: $crate::Highlight) -> w::Highlight {
                w::Highlight {
                    span: to_span(h.span),
                    kind: match h.kind {
                        $crate::HighlightKind::Keyword => w::HighlightKind::Keyword,
                        $crate::HighlightKind::Parameter => w::HighlightKind::Parameter,
                    },
                }
            }

            pub fn from_highlight(h: w::Highlight) -> $crate::Highlight {
                $crate::Highlight {
                    span: from_span(h.span),
                    kind: match h.kind {
                        w::HighlightKind::Keyword => $crate::HighlightKind::Keyword,
                        w::HighlightKind::Parameter => $crate::HighlightKind::Parameter,
                    },
                }
            }

            pub fn to_hover(h: $crate::Hover) -> w::Hover {
                w::Hover { markdown: h.markdown, span: to_span(h.span) }
            }

            pub fn from_hover(h: w::Hover) -> $crate::Hover {
                $crate::Hover { markdown: h.markdown, span: from_span(h.span) }
            }

            fn to_fields(fields: Vec<$crate::Field>) -> Vec<w::Field> {
                fields.into_iter().map(|f| w::Field { label: f.label, value: f.value }).collect()
            }

            fn from_fields(fields: Vec<w::Field>) -> Vec<$crate::Field> {
                fields.into_iter().map(|f| $crate::Field { label: f.label, value: f.value }).collect()
            }

            pub fn to_arena(a: $crate::Arena) -> w::Arena {
                let nodes = a.nodes.into_iter().map(|n| w::Node {
                    kind: match n.kind {
                        $crate::Kind::Expr(e) => w::Kind::Expr(match e {
                            $crate::Expr::Name(s) => w::Expr::Name(s),
                            $crate::Expr::Number(n) => w::Expr::Number(n),
                            $crate::Expr::Text(s) => w::Expr::Text(s),
                            $crate::Expr::Boolean(b) => w::Expr::Boolean(b),
                            $crate::Expr::Unit => w::Expr::Unit,
                            $crate::Expr::Apply { function, arguments } => {
                                w::Expr::Apply(w::Apply { function, arguments })
                            }
                            $crate::Expr::Lambda { params, body } => {
                                w::Expr::Lambda(w::Lambda { params, body })
                            }
                            $crate::Expr::Record(f) => w::Expr::Record(to_fields(f)),
                            $crate::Expr::Get { record, label } => w::Expr::Get(w::Get { record, label }),
                            $crate::Expr::Items(i) => w::Expr::Items(i),
                            $crate::Expr::Tuple(i) => w::Expr::Tuple(i),
                            $crate::Expr::Annot { expr, ty } => w::Expr::Annot(w::Annot { expr, ty }),
                            $crate::Expr::Hole(s) => w::Expr::Hole(to_span(s)),
                        }),
                        $crate::Kind::Ty(t) => w::Kind::Ty(match t {
                            $crate::Ty::Name(s) => w::Ty::Name(s),
                            $crate::Ty::Apply { function, arguments } => {
                                w::Ty::Apply(w::Apply { function, arguments })
                            }
                            $crate::Ty::Record(f) => w::Ty::Record(to_fields(f)),
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
                            $crate::Pat::Con { name, arguments } => w::Pat::Con(w::ConPat { name, arguments }),
                            $crate::Pat::Record(f) => w::Pat::Record(to_fields(f)),
                            $crate::Pat::Hole(s) => w::Pat::Hole(to_span(s)),
                        }),
                        $crate::Kind::Decl(d) => w::Kind::Decl(match d {
                            $crate::Decl::Let { name, params, ty, body } => {
                                w::Decl::Let(w::LetDecl { name, params, ty, body })
                            }
                            $crate::Decl::Signature { name, ty } => {
                                w::Decl::Signature(w::Signature { name, ty })
                            }
                            $crate::Decl::Alias { name, params, ty } => {
                                w::Decl::Alias(w::Alias { name, params, ty })
                            }
                            $crate::Decl::Adt { name, params, cases, deriving } => {
                                w::Decl::Adt(w::Adt {
                                    name,
                                    params,
                                    cases: cases.into_iter().map(|c| w::Case { name: c.name, fields: c.fields }).collect(),
                                    deriving,
                                })
                            }
                            $crate::Decl::Instance { context, head, members } => {
                                w::Decl::Instance(w::Instance { context, head, members })
                            }
                            $crate::Decl::Group(d) => w::Decl::Group(d),
                            $crate::Decl::Hole(s) => w::Decl::Hole(to_span(s)),
                        }),
                    },
                    origin: to_span(n.origin),
                }).collect();
                w::Arena { nodes, root: a.root }
            }

            pub fn from_arena(a: w::Arena) -> $crate::Arena {
                let nodes = a.nodes.into_iter().map(|n| $crate::Node {
                    kind: match n.kind {
                        w::Kind::Expr(e) => $crate::Kind::Expr(match e {
                            w::Expr::Name(s) => $crate::Expr::Name(s),
                            w::Expr::Number(n) => $crate::Expr::Number(n),
                            w::Expr::Text(s) => $crate::Expr::Text(s),
                            w::Expr::Boolean(b) => $crate::Expr::Boolean(b),
                            w::Expr::Unit => $crate::Expr::Unit,
                            w::Expr::Apply(a) => $crate::Expr::Apply { function: a.function, arguments: a.arguments },
                            w::Expr::Lambda(l) => $crate::Expr::Lambda { params: l.params, body: l.body },
                            w::Expr::Record(f) => $crate::Expr::Record(from_fields(f)),
                            w::Expr::Get(g) => $crate::Expr::Get { record: g.record, label: g.label },
                            w::Expr::Items(i) => $crate::Expr::Items(i),
                            w::Expr::Tuple(i) => $crate::Expr::Tuple(i),
                            w::Expr::Annot(a) => $crate::Expr::Annot { expr: a.expr, ty: a.ty },
                            w::Expr::Hole(s) => $crate::Expr::Hole(from_span(s)),
                        }),
                        w::Kind::Ty(t) => $crate::Kind::Ty(match t {
                            w::Ty::Name(s) => $crate::Ty::Name(s),
                            w::Ty::Apply(a) => $crate::Ty::Apply { function: a.function, arguments: a.arguments },
                            w::Ty::Record(f) => $crate::Ty::Record(from_fields(f)),
                            w::Ty::Function(a) => $crate::Ty::Function { input: a.input, output: a.output },
                            w::Ty::Text(s) => $crate::Ty::Text(s),
                            w::Ty::Wildcard => $crate::Ty::Wildcard,
                            w::Ty::Hole(s) => $crate::Ty::Hole(from_span(s)),
                        }),
                        w::Kind::Pat(q) => $crate::Kind::Pat(match q {
                            w::Pat::Var(s) => $crate::Pat::Var(s),
                            w::Pat::Wildcard => $crate::Pat::Wildcard,
                            w::Pat::Con(c) => $crate::Pat::Con { name: c.name, arguments: c.arguments },
                            w::Pat::Record(f) => $crate::Pat::Record(from_fields(f)),
                            w::Pat::Hole(s) => $crate::Pat::Hole(from_span(s)),
                        }),
                        w::Kind::Decl(d) => $crate::Kind::Decl(match d {
                            w::Decl::Let(l) => $crate::Decl::Let { name: l.name, params: l.params, ty: l.ty, body: l.body },
                            w::Decl::Signature(s) => $crate::Decl::Signature { name: s.name, ty: s.ty },
                            w::Decl::Alias(a) => $crate::Decl::Alias { name: a.name, params: a.params, ty: a.ty },
                            w::Decl::Adt(a) => $crate::Decl::Adt {
                                name: a.name,
                                params: a.params,
                                cases: a.cases.into_iter().map(|c| $crate::Case { name: c.name, fields: c.fields }).collect(),
                                deriving: a.deriving,
                            },
                            w::Decl::Instance(i) => $crate::Decl::Instance { context: i.context, head: i.head, members: i.members },
                            w::Decl::Group(d) => $crate::Decl::Group(d),
                            w::Decl::Hole(s) => $crate::Decl::Hole(from_span(s)),
                        }),
                    },
                    origin: from_span(n.origin),
                }).collect();
                $crate::Arena { nodes, root: a.root }
            }
        }
    };
}
