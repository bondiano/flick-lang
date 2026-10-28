//! `flick ffi check`: a generated module must be what a new import would
//! write, and each reviewed binding — a `foreign` with `reviewed "<pkg>@<ver>
//! sha256:…"` — must still match the declaration it was reviewed against
//! and state the policies its shape needs.

use super::import::HEADER;
use super::{ExportShape, Shape, Surface};
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, Span};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal::string_value;
use crate::syntax::parse;

/// What the header of a generated module says was imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub package: String,
    pub select: Option<Vec<String>>,
}

/// Reads `// flick ffi import <package> [--select a,b]`.
pub fn header(text: &str) -> Option<Header> {
    let line = text.lines().next()?.strip_prefix(HEADER)?;
    let mut words = line.split_whitespace();
    let package = words.next()?.to_owned();
    let select = match (words.next(), words.next()) {
        (Some("--select"), Some(list)) => Some(list.split(',').map(str::to_owned).collect()),
        _ => None,
    };
    Some(Header { package, select })
}

/// A reviewed `foreign` binding of a package export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reviewed {
    pub name: String,
    pub package: String,
    pub export: String,
    /// What the review vouches for: `pkg@1.0.0 sha256:…`.
    pub provenance: String,
    pub fingerprint: String,
    /// The metadata words of the binding: `catch`, `nullable`, …
    pub metadata: Vec<String>,
    pub span: Span,
}

/// The reviewed bindings of a source file.
pub fn reviewed(text: &str, file: FileId) -> Vec<Reviewed> {
    let tree = parse(text, file).tree;
    tree.nodes()
        .filter(|d| d.kind == K::ForeignDecl)
        .filter_map(|decl| binding(decl, text, file))
        .collect()
}

fn binding(decl: &Node, text: &str, file: FileId) -> Option<Reviewed> {
    let name = decl
        .child_tokens()
        .filter(|t| t.kind == K::Ident)
        .nth(1)?
        .text(text)
        .to_owned();
    let source = decl.nodes().find(|n| n.kind == K::ForeignSource)?;
    let strings: Vec<String> = source
        .child_tokens()
        .filter(|t| t.kind == K::String)
        .map(|t| string_value(t.text(text)))
        .collect();
    let [package, export] = &strings[..] else {
        return None;
    };
    let items: Vec<&Node> = decl
        .nodes()
        .filter(|n| n.kind == K::ForeignMeta)
        .flat_map(Node::nodes)
        .collect();
    let word = |item: &Node| {
        item.child_tokens()
            .find(|t| t.kind == K::Ident)
            .map(|t| t.text(text).to_owned())
    };
    let review = items
        .iter()
        .find(|i| word(i).as_deref() == Some("reviewed"))?;
    let provenance = review
        .nodes()
        .next()
        .map(|n| string_value(n.text(text).trim()))?;
    let fingerprint = provenance
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    let (start, end) = review.range();
    Some(Reviewed {
        name,
        package: package.clone(),
        export: export.clone(),
        provenance,
        fingerprint,
        metadata: items.iter().filter_map(|i| word(i)).collect(),
        span: Span::new(file, start, end),
    })
}

/// Checks the reviewed bindings of `surface`'s package.
pub fn check_reviewed(bindings: &[Reviewed], surface: &Surface) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for b in bindings.iter().filter(|b| b.package == surface.package) {
        let Some(export) = surface.find(&b.export) else {
            out.push(Diagnostic::error(
                codes::F007,
                format!(
                    "`{}` no longer exports `{}`, which `{}` was reviewed against",
                    surface.provenance(),
                    b.export,
                    b.name
                ),
                b.span,
            ));
            continue;
        };
        if export.fingerprint != b.fingerprint {
            out.push(
                Diagnostic::error(
                    codes::F007,
                    format!(
                        "the declaration of `{}` changed since `{}` was reviewed against {}",
                        b.export, b.name, b.provenance
                    ),
                    b.span,
                )
                .with_fix(|| {
                    (
                        format!(
                            "review the binding against {} and write `reviewed \"{} {}\"`",
                            surface.provenance(),
                            surface.provenance(),
                            export.fingerprint
                        ),
                        Vec::new(),
                    )
                }),
            );
        }
        for (meta, why) in required(export) {
            if !b.metadata.iter().any(|m| m == meta) {
                out.push(Diagnostic::error(
                    codes::F007,
                    format!(
                        "`{}` {why}: its reviewed binding `{}` needs `{meta}`",
                        b.export, b.name
                    ),
                    b.span,
                ));
            }
        }
    }
    out
}

/// The policies an export's shape makes mandatory, as metadata words.
pub fn required(export: &ExportShape) -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    let results = export.signatures.iter().map(|s| match &s.result {
        Shape::Promise { ty } => (true, ty.as_ref()),
        other => (false, other),
    });
    for (promise, result) in results {
        if promise && !out.iter().any(|(m, _)| *m == "catch") {
            out.push(("catch", "returns a Promise, which may reject"));
        }
        let empty = matches!(result, Shape::Union { types }
            if types.iter().any(|t| matches!(t, Shape::Undefined | Shape::Null)));
        if empty && !out.iter().any(|(m, _)| *m == "nullable") {
            out.push(("nullable", "may return `undefined` or `null`"));
        }
    }
    let callback = export
        .signatures
        .iter()
        .flat_map(|s| &s.params)
        .any(|p| matches!(p.ty, Shape::Function { .. }));
    if callback {
        out.push(("retention", "takes a callback"));
    }
    out
}

/// A generated module that differs from what the import writes now.
pub fn stale(file: FileId, path: &str, surface: &Surface) -> Diagnostic {
    Diagnostic::error(
        codes::F006,
        format!(
            "`{path}` is not what `flick ffi import` writes for {} now",
            surface.provenance()
        ),
        Span::new(file, 0, 0),
    )
    .with_fix(|| {
        (
            format!(
                "run `flick ffi import {}` again, then `flick ffi check`",
                surface.package
            ),
            Vec::new(),
        )
    })
}
