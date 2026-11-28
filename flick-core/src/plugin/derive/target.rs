//! Extracting rich [`DeriveTarget`] metadata (spans, visibility, leading
//! attributes, variant docs/attributes/constraints/GADT results/enum literals)
//! from a parsed `TypeDecl` or `ErrorDecl`.

use flick_plugin::{
    AttributeMeta, Block, CaseTarget, DeriveTarget, LiteralValue, Span as BlockSpan, Visibility,
};

use super::named_classes;
use crate::plugin::source::docs_above_item;
use crate::plugin::written::{constraints_of, written};
use crate::resolve::{ItemKind, ModuleSyntax};
use crate::syntax::attr::attr_parts;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::is_type;

/// What `decl`, a `TypeDecl` or `ErrorDecl` read as `block`, declares, for a
/// derive: its spans count from its start. `qualifier` names the module of a
/// type of another module, which qualifies its names and makes no holes.
pub fn target(
    decl: &Node,
    text: &str,
    block: Block,
    qualifier: Option<&str>,
) -> Option<DeriveTarget> {
    target_in(decl, None, None, text, block, qualifier)
}

/// [`target`] with enclosing module `tree` and `syntax`, populating leading
/// declaration attributes and export [`Visibility`].
#[allow(clippy::too_many_lines)]
pub fn target_in(
    decl: &Node,
    tree: Option<&Node>,
    syntax: Option<&ModuleSyntax>,
    text: &str,
    block: Block,
    qualifier: Option<&str>,
) -> Option<DeriveTarget> {
    let start = decl.range().0;
    let span = |n: &Node| {
        let (s, e) = n.range();
        BlockSpan {
            start: s.saturating_sub(start),
            end: e.saturating_sub(start),
        }
    };
    let ty = |n: &Node| written(n, text, start);
    let named = |name: &str| match qualifier {
        Some(q) => format!("{q}.{name}"),
        None => name.to_owned(),
    };
    let name_tok = decl.child_tokens().find(|t| t.kind == K::Con)?;
    let name = name_tok.text(text);
    let name_span = BlockSpan {
        start: name_tok.start.saturating_sub(start),
        end: name_tok.end.saturating_sub(start),
    };
    let mut params = Vec::new();
    let mut param_spans = Vec::new();
    let mut kinds = Vec::new();
    for p in decl.nodes().filter(|n| n.kind == K::TypeParam) {
        let Some(p_name) = p.child_tokens().find(|t| t.kind == K::Ident) else {
            continue;
        };
        // Canonical token spacing, independent of how the kind was written.
        let kind = p.nodes().next().map_or_else(String::new, |k| {
            k.tokens()
                .iter()
                .filter(|t| !t.kind.is_trivia())
                .map(|t| t.text(text))
                .collect::<Vec<_>>()
                .join(" ")
        });
        params.push(p_name.text(text).to_owned());
        param_spans.push(span(p));
        kinds.push(kind);
    }
    // the words before its name: `type`, `alias`, `opaque`, `unboxed`, `error`
    let modifiers: Vec<String> = (decl.child_tokens())
        .take_while(|t| t.kind != K::Con)
        .map(|t| t.text(text).to_owned())
        .collect();
    let (deriving, deriving_spans): (Vec<String>, Vec<BlockSpan>) = (decl.nodes())
        .filter(|n| n.kind == K::DerivingClause)
        .flat_map(|clause| named_classes(clause, text))
        .map(|n| {
            let class_name = if n.qualifier.is_empty() {
                n.name
            } else {
                format!("{}.{}", n.qualifier, n.name)
            };
            let sp = BlockSpan {
                start: n.span.0.saturating_sub(start),
                end: n.span.1.saturating_sub(start),
            };
            (class_name, sp)
        })
        .unzip();
    let mut raw_case_names = Vec::new();
    let cases: Vec<CaseTarget> = (decl.nodes())
        .filter(|n| n.kind == K::Variant)
        .filter_map(|v| {
            let con_tok = v.child_tokens().find(|t| t.kind == K::Con)?;
            let con = con_tok.text(text);
            raw_case_names.push(con);
            let fields = v.nodes().filter(|n| is_type(n.kind)).map(ty).collect();
            let attributes = (v.nodes())
                .filter(|n| n.kind == K::Attribute)
                .filter_map(|a| attribute_meta(a, text, Some(start)))
                .collect();
            let constraints = (v.nodes())
                .filter(|n| n.kind == K::Context)
                .flat_map(|c| constraints_of(c, text, start))
                .collect();
            let result = v
                .nodes()
                .find(|n| n.kind == K::VariantResult)
                .and_then(|r| r.nodes().find(|n| is_type(n.kind)))
                .map(ty);
            let lit_node = v.nodes().find(|n| n.kind == K::LitPat);
            let literal = lit_node.map(span);
            let literal_value = lit_node.and_then(|lit| parse_literal_value(lit, text));
            Some(CaseTarget {
                name: named(con),
                span: span(v),
                name_span: BlockSpan {
                    start: con_tok.start.saturating_sub(start),
                    end: con_tok.end.saturating_sub(start),
                },
                docs: docs_above_item(text, v.range().0, true),
                attributes,
                constraints,
                fields,
                result,
                literal,
                literal_value,
                refined: v.nodes().any(|n| n.kind == K::VariantResult),
                constrained: v.nodes().any(|n| n.kind == K::Context),
            })
        })
        .collect();
    let body = decl
        .nodes()
        .find(|n| is_type(n.kind) && n.kind != K::TypeParam);
    let alias = decl
        .child_tokens()
        .any(|t| t.kind == K::Ident && t.text(text) == "alias");
    let visibility = decl_visibility(syntax, name, &modifiers, alias, &raw_case_names);
    let attributes = tree
        .and_then(|t| find_leading_attributes(t, decl.range(), text))
        .unwrap_or_default();
    Some(DeriveTarget {
        decl: block,
        name: named(name),
        name_span,
        params,
        param_spans,
        kinds,
        cases,
        alias: body.filter(|_| alias).map(ty),
        wraps: None,
        modifiers,
        deriving,
        deriving_spans,
        visibility,
        attributes,
        local: qualifier.is_none(),
    })
}

/// Computes the export visibility of a type declaration from its modifiers and
/// the enclosing module's `exposing` list.
fn decl_visibility(
    syntax: Option<&ModuleSyntax>,
    name: &str,
    modifiers: &[String],
    is_alias: bool,
    raw_cases: &[&str],
) -> Visibility {
    let is_opaque = modifiers.iter().any(|m| m == "opaque");
    let Some(header) = syntax.and_then(|s| s.header.as_ref()) else {
        return if is_opaque {
            Visibility::Opaque
        } else {
            Visibility::PublicConstructors
        };
    };
    let Some(exposing) = header.exposing.as_deref() else {
        return Visibility::Private;
    };
    if exposing.iter().any(|i| i.kind == ItemKind::All) {
        return if is_opaque {
            Visibility::Opaque
        } else {
            Visibility::PublicConstructors
        };
    }
    let exported_all = exposing
        .iter()
        .any(|i| i.kind == ItemKind::ConAll && i.name == name);
    let exported_type = exported_all
        || exposing
            .iter()
            .any(|i| i.kind == ItemKind::Con && i.name == name);
    if !exported_type {
        return Visibility::Private;
    }
    if is_opaque {
        return Visibility::Opaque;
    }
    if is_alias || exported_all {
        return Visibility::PublicConstructors;
    }
    let all_cases_exported = !raw_cases.is_empty()
        && raw_cases.iter().all(|c| {
            *c != name
                && exposing
                    .iter()
                    .any(|i| i.kind == ItemKind::Con && i.name == *c)
        });
    if all_cases_exported {
        Visibility::PublicConstructors
    } else {
        Visibility::Opaque
    }
}

/// Extracts `@name args...` (or `@ns.name args...`) metadata from a `K::Attribute` node.
pub(crate) fn attribute_meta(
    attr: &Node,
    text: &str,
    origin: Option<u32>,
) -> Option<AttributeMeta> {
    let parts = attr_parts(attr, text)?;
    let (start, end) = attr.range();
    let span = match origin {
        Some(orig) if start >= orig => BlockSpan {
            start: start - orig,
            end: end - orig,
        },
        _ => BlockSpan::default(),
    };
    Some(AttributeMeta {
        name: parts.name.to_owned(),
        args: parts.args.to_owned(),
        span,
    })
}

/// Finds consecutive leading `K::Attribute` sibling nodes immediately preceding
/// the type/error declaration at `target_range` in `container`.
fn find_leading_attributes(
    container: &Node,
    target_range: (u32, u32),
    text: &str,
) -> Option<Vec<AttributeMeta>> {
    let mut pending: Vec<&Node> = Vec::new();
    for child in container.nodes() {
        if child.kind == K::Attribute {
            pending.push(child);
            continue;
        }
        if child.range() == target_range && matches!(child.kind, K::TypeDecl | K::ErrorDecl) {
            return Some(
                pending
                    .into_iter()
                    .filter_map(|a| attribute_meta(a, text, None))
                    .collect(),
            );
        }
        pending.clear();
        if matches!(child.kind, K::RecGroup | K::Members)
            && let Some(found) = find_leading_attributes(child, target_range, text)
        {
            return Some(found);
        }
    }
    None
}

/// Decodes a `K::LitPat` node on an enum variant into a [`LiteralValue`].
fn parse_literal_value(lit: &Node, text: &str) -> Option<LiteralValue> {
    if let Some(tok) = lit.child_tokens().find(|t| t.kind == K::String) {
        return Some(LiteralValue::String(flick_plugin::string_value(
            tok.text(text),
        )));
    }
    if lit.child_tokens().any(|t| t.kind == K::Number) {
        return Some(LiteralValue::Int(flick_plugin::literal_text(
            &lit.text(text),
        )));
    }
    None
}
