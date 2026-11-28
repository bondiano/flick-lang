//! Surface syntax re-exported from [`flick_syntax`], with parsing entry points
//! wired to the first-party [`HeadCatalog`] built from plugin metadata.

use std::sync::{Arc, OnceLock};

use flick_plugin::PluginMetadata;
pub use flick_syntax::syntax::*;

use crate::source::FileId;

/// Aggregated metadata of all first-party `std` plugins, built once from each
/// plugin crate's `metadata()`.
#[must_use]
pub fn std_metadata() -> &'static PluginMetadata {
    static META: OnceLock<PluginMetadata> = OnceLock::new();
    META.get_or_init(|| {
        let mut combined = PluginMetadata::default();
        for part in [
            flick_plugin_cli::metadata(),
            flick_plugin_derive::metadata(),
            flick_plugin_sql::metadata(),
            flick_plugin_ui::metadata(),
            flick_plugin_refined::metadata(),
            flick_plugin_test::metadata(),
        ] {
            combined.heads.extend(part.heads);
            combined.watch.extend(part.watch);
            combined.derives.extend(part.derives);
            combined.own_codes.extend(part.own_codes);
            combined.attributes.extend(part.attributes);
        }
        combined
    })
}

/// The parser [`HeadCatalog`] for `std`'s implicit plugin heads, built once
/// from [`std_metadata`].
#[must_use]
pub fn std_head_catalog() -> &'static HeadCatalog {
    std_head_catalog_arc().as_ref()
}

/// Shared [`Arc`] handle to `std`'s [`HeadCatalog`].
#[must_use]
pub fn std_head_catalog_arc() -> &'static Arc<HeadCatalog> {
    static CATALOG: OnceLock<Arc<HeadCatalog>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        Arc::new(HeadCatalog::from_specs(
            std_metadata().heads.iter().map(|spec| (spec, true)),
        ))
    })
}

/// Parses `text` with `std`'s plugin [`HeadCatalog`].
#[must_use]
pub fn parse(text: &str, file: FileId) -> Parse {
    parse_with_catalog(text, file, std_head_catalog())
}

/// Parses `text` with explicit imported plugin `heads` and `std`'s [`HeadCatalog`].
#[must_use]
pub fn parse_with_heads<'t>(text: &'t str, file: FileId, heads: Heads<'t>) -> Parse {
    parse_with_heads_and_catalog(text, file, heads, std_head_catalog())
}

/// Parses an expression hole with `std`'s [`HeadCatalog`].
#[must_use]
pub fn parse_expr_at<'t>(
    text: &'t str,
    file: FileId,
    tokens: &[cst::Token],
    heads: Heads<'t>,
) -> Option<Fragment> {
    parse_expr_at_with_catalog(text, file, tokens, heads, std_head_catalog())
}

/// Parses a body hole with `std`'s [`HeadCatalog`].
#[must_use]
pub fn parse_body_at<'t>(
    text: &'t str,
    file: FileId,
    tokens: &[cst::Token],
    heads: Heads<'t>,
) -> Option<Fragment> {
    parse_body_at_with_catalog(text, file, tokens, heads, std_head_catalog())
}

/// Parses a line-children body hole with `std`'s [`HeadCatalog`].
#[must_use]
pub fn parse_lines_at<'t>(
    text: &'t str,
    file: FileId,
    tokens: &[cst::Token],
    heads: Heads<'t>,
) -> Option<Fragment> {
    parse_lines_at_with_catalog(text, file, tokens, heads, std_head_catalog())
}

/// Parses a type hole with `std`'s [`HeadCatalog`].
#[must_use]
pub fn parse_type_at<'t>(
    text: &'t str,
    file: FileId,
    tokens: &[cst::Token],
    heads: Heads<'t>,
) -> Option<Fragment> {
    parse_type_at_with_catalog(text, file, tokens, heads, std_head_catalog())
}

/// Parses a pattern hole with `std`'s [`HeadCatalog`].
#[must_use]
pub fn parse_pat_at<'t>(
    text: &'t str,
    file: FileId,
    tokens: &[cst::Token],
    heads: Heads<'t>,
) -> Option<Fragment> {
    parse_pat_at_with_catalog(text, file, tokens, heads, std_head_catalog())
}

/// Parses a declaration hole with `std`'s [`HeadCatalog`].
#[must_use]
pub fn parse_decl_at<'t>(
    text: &'t str,
    file: FileId,
    tokens: &[cst::Token],
    heads: Heads<'t>,
) -> Option<Fragment> {
    parse_decl_at_with_catalog(text, file, tokens, heads, std_head_catalog())
}

/// Parses declarations with `std`'s [`HeadCatalog`].
#[must_use]
pub fn parse_decls_at<'t>(
    text: &'t str,
    file: FileId,
    tokens: &[cst::Token],
    heads: Heads<'t>,
) -> Option<Fragment> {
    parse_decls_at_with_catalog(text, file, tokens, heads, std_head_catalog())
}
