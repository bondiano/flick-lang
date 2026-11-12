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
                    docs: b.docs.clone(),
                    tokens: to_tokens(&b.tokens),
                    signature: b.signature.as_ref().map(|s| w::SignatureAbove {
                        text: s.text.clone(),
                        tokens: to_tokens(&s.tokens),
                    }),
                    module: b.module.clone(),
                }
            }

            fn to_tokens(tokens: &[$crate::Token]) -> Vec<w::Token> {
                tokens.iter().map(|t| w::Token {
                    kind: match t.kind {
                        $crate::TokenKind::Ident => w::TokenKind::Ident,
                        $crate::TokenKind::Con => w::TokenKind::Con,
                        $crate::TokenKind::Number => w::TokenKind::Number,
                        $crate::TokenKind::String => w::TokenKind::String,
                        $crate::TokenKind::Keyword => w::TokenKind::Keyword,
                        $crate::TokenKind::Punct => w::TokenKind::Punct,
                    },
                    span: to_span(t.span),
                }).collect()
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
                    docs: b.docs,
                    tokens: from_tokens(b.tokens),
                    signature: b.signature.map(|s| $crate::Signature {
                        text: s.text,
                        tokens: from_tokens(s.tokens),
                    }),
                    module: b.module,
                }
            }

            fn from_tokens(tokens: Vec<w::Token>) -> Vec<$crate::Token> {
                tokens.into_iter().map(|t| $crate::Token {
                    kind: match t.kind {
                        w::TokenKind::Ident => $crate::TokenKind::Ident,
                        w::TokenKind::Con => $crate::TokenKind::Con,
                        w::TokenKind::Number => $crate::TokenKind::Number,
                        w::TokenKind::String => $crate::TokenKind::String,
                        w::TokenKind::Keyword => $crate::TokenKind::Keyword,
                        w::TokenKind::Punct => $crate::TokenKind::Punct,
                    },
                    span: from_span(t.span),
                }).collect()
            }

            pub fn to_hole(h: $crate::Hole) -> w::Hole {
                w::Hole {
                    kind: match h.kind {
                        $crate::HoleKind::Expr => w::HoleKind::Expr,
                        $crate::HoleKind::Ty => w::HoleKind::Ty,
                        $crate::HoleKind::Pat => w::HoleKind::Pat,
                        $crate::HoleKind::Decl => w::HoleKind::Decl,
                        $crate::HoleKind::Lines => w::HoleKind::Lines,
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
                        w::HoleKind::Lines => $crate::HoleKind::Lines,
                    },
                    span: from_span(h.span),
                }
            }

            pub fn to_artifact(a: $crate::Artifact) -> w::Artifact {
                w::Artifact { path: a.path, contents: a.contents }
            }

            pub fn from_artifact(a: w::Artifact) -> $crate::Artifact {
                $crate::Artifact { path: a.path, contents: a.contents }
            }

            pub fn to_diagnostic(d: $crate::Diagnostic) -> w::Diagnostic {
                w::Diagnostic {
                    message: d.message,
                    span: to_span(d.span),
                    code: d.code,
                    severity: match d.severity {
                        $crate::Severity::Error => w::Severity::Error,
                        $crate::Severity::Warning => w::Severity::Warning,
                        $crate::Severity::Note => w::Severity::Note,
                    },
                    fixes: (d.fixes.into_iter())
                        .map(|f| w::Fix {
                            message: f.message,
                            edits: (f.edits.into_iter())
                                .map(|e| w::Edit { span: to_span(e.span), replacement: e.replacement })
                                .collect(),
                            safe: f.safe,
                        })
                        .collect(),
                }
            }

            pub fn from_diagnostic(d: w::Diagnostic) -> $crate::Diagnostic {
                $crate::Diagnostic {
                    message: d.message,
                    span: from_span(d.span),
                    code: d.code,
                    severity: match d.severity {
                        w::Severity::Error => $crate::Severity::Error,
                        w::Severity::Warning => $crate::Severity::Warning,
                        w::Severity::Note => $crate::Severity::Note,
                    },
                    fixes: (d.fixes.into_iter())
                        .map(|f| $crate::Fix {
                            message: f.message,
                            edits: (f.edits.into_iter())
                                .map(|e| $crate::Edit { span: from_span(e.span), replacement: e.replacement })
                                .collect(),
                            safe: f.safe,
                        })
                        .collect(),
                }
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

            pub fn to_target(t: &$crate::DeriveTarget) -> w::DeriveTarget {
                w::DeriveTarget {
                    decl: to_block(&t.decl),
                    name: t.name.clone(),
                    params: t.params.clone(),
                    cases: t.cases.iter().map(|c| w::CaseTarget {
                        name: c.name.clone(),
                        fields: c.fields.iter().copied().map(to_span).collect(),
                        literal: c.literal.map(to_span),
                        refined: c.refined,
                        constrained: c.constrained,
                    }).collect(),
                    alias: t.alias.map(to_span),
                    wraps: t.wraps.map(to_span),
                    local: t.local,
                }
            }

            pub fn from_target(t: w::DeriveTarget) -> $crate::DeriveTarget {
                $crate::DeriveTarget {
                    decl: from_block(t.decl),
                    name: t.name,
                    params: t.params,
                    cases: t.cases.into_iter().map(|c| $crate::CaseTarget {
                        name: c.name,
                        fields: c.fields.into_iter().map(from_span).collect(),
                        literal: c.literal.map(from_span),
                        refined: c.refined,
                        constrained: c.constrained,
                    }).collect(),
                    alias: t.alias.map(from_span),
                    wraps: t.wraps.map(from_span),
                    local: t.local,
                }
            }

            pub fn to_piece(p: $crate::Piece) -> w::Piece {
                match p {
                    $crate::Piece::Text(t) => w::Piece::Text(t),
                    $crate::Piece::Line => w::Piece::Line,
                    $crate::Piece::Indent => w::Piece::Indent,
                    $crate::Piece::Dedent => w::Piece::Dedent,
                    $crate::Piece::Hole(h) => w::Piece::Hole(to_hole(h)),
                }
            }

            pub fn from_piece(p: w::Piece) -> $crate::Piece {
                match p {
                    w::Piece::Text(t) => $crate::Piece::Text(t),
                    w::Piece::Line => $crate::Piece::Line,
                    w::Piece::Indent => $crate::Piece::Indent,
                    w::Piece::Dedent => $crate::Piece::Dedent,
                    w::Piece::Hole(h) => $crate::Piece::Hole(from_hole(h)),
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

            pub fn to_structure(s: &$crate::Structure) -> w::Structure {
                w::Structure { lines: s.lines, complexity: s.complexity, nesting: s.nesting.iter().map(|n| w::Nesting { span: to_span(n.span), depth: n.depth }).collect(), max_lines: s.max_lines, max_complexity: s.max_complexity, max_nesting: s.max_nesting }
            }
            pub fn from_structure(s: w::Structure) -> $crate::Structure {
                $crate::Structure { lines: s.lines, complexity: s.complexity, nesting: s.nesting.into_iter().map(|n| $crate::Nesting { span: from_span(n.span), depth: n.depth }).collect(), max_lines: s.max_lines, max_complexity: s.max_complexity, max_nesting: s.max_nesting }
            }

            pub fn to_fact(f: &$crate::Fact) -> w::Fact {
                let ty = to_type;
                w::Fact {
                    span: to_span(f.span),
                    ty: ty(&f.ty),
                    fields: f.fields.as_ref().map(|fs| fs.iter().map(|x| w::FactField { label: x.label.clone(), ty: ty(&x.ty) }).collect()),
                }
            }

            pub fn from_fact(f: w::Fact) -> $crate::Fact {
                let ty = from_type;
                $crate::Fact {
                    span: from_span(f.span),
                    ty: ty(f.ty),
                    fields: f.fields.map(|fs| fs.into_iter().map(|x| $crate::FactField { label: x.label, ty: ty(x.ty) }).collect()),
                }
            }

            $crate::wit_call_conversions!();

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

            $crate::wit_arena_conversions!();
        }
    };
}
