//! Parsing helper for the neutral `@export "name"` attribute on top-level
//! functions.

use super::attr::attr_parts;
use super::cst::Node;
use super::kind::SyntaxKind as K;
use super::literal::string_value;

/// The attribute name for host-named function exports.
pub const TAG: &str = "export";

const USAGE: &str = "`@export` takes one host export name string, as `@export \"add\"`";

/// A parsed `@export "name"` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportAttr {
    /// Decoded host export name (validated by the target backend).
    pub name: String,
    /// Full byte range of the `@export "…"` attribute node in source.
    pub span: (u32, u32),
}

/// Whether `node` is an `@export` attribute (regardless of argument validity).
pub fn is_export(node: &Node, text: &str) -> bool {
    attr_parts(node, text).is_some_and(|parts| !parts.namespaced && parts.name == TAG)
}

/// Reads the raw string token of an `@export "<name>"` attribute without
/// decoding escapes or allocating.
///
/// Returns `None` if `node` is not an `@export` attribute, or `Some(Err(..))`
/// if its argument is missing, non-string, or followed by extra arguments.
pub fn read_raw<'a>(node: &Node, text: &'a str) -> Option<Result<&'a str, &'static str>> {
    let parts = attr_parts(node, text)?;
    if parts.namespaced || parts.name != TAG {
        return None;
    }
    let mut tokens = node.child_tokens().filter(|t| t.start >= parts.name_end);
    let Some(arg) = tokens.next() else {
        return Some(Err(USAGE));
    };
    if arg.kind != K::String || tokens.next().is_some() {
        return Some(Err(USAGE));
    }
    Some(Ok(arg.text(text)))
}

/// Reads an `@export "<name>"` attribute node.
///
/// Returns `None` if `node` is not an `@export` attribute, or `Some(Err(..))`
/// if its argument is missing, non-string, or followed by extra arguments.
pub fn read(node: &Node, text: &str) -> Option<Result<ExportAttr, &'static str>> {
    Some(read_raw(node, text)?.map(|raw| ExportAttr {
        name: string_value(raw),
        span: node.range(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::codes;
    use crate::source::FileId;
    use crate::syntax::parse;

    #[test]
    fn reads_valid_export_attribute() {
        let src = "@export \"addNumbers\"\nlet add a b = a + b\n";
        let parsed = parse(src, FileId::new(0));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let attr = parsed
            .tree
            .nodes()
            .find(|n| n.kind == K::Attribute)
            .expect("attribute");
        assert!(is_export(attr, src));
        let parsed_attr = read(attr, src)
            .expect("is export")
            .expect("valid export attr");
        assert_eq!(parsed_attr.name, "addNumbers");
        assert_eq!(
            &src[parsed_attr.span.0 as usize..parsed_attr.span.1 as usize],
            "@export \"addNumbers\""
        );
    }

    #[test]
    fn rejects_invalid_args_and_non_top_level_placement() {
        for bad in [
            "@export\nlet f x = x\n",
            "@export foo\nlet f x = x\n",
            "@export \"a\" \"b\"\nlet f x = x\n",
            "@export \"a\"\ntype T = A\n",
            "instance Show Int =\n  @export \"inner\"\n  let show _ = \"\"\n",
        ] {
            let parsed = parse(bad, FileId::new(0));
            assert!(
                parsed.diagnostics.iter().any(|d| d.code == codes::S014),
                "expected S014 for:\n{bad}\ngot: {:?}",
                parsed.diagnostics
            );
        }
        let in_block = "let outer x =\n  @export \"inner\"\n  let g y = y\n  g x\n";
        let parsed = parse(in_block, FileId::new(0));
        assert!(
            !parsed.diagnostics.is_empty(),
            "expected syntax error inside block"
        );
    }

    #[test]
    fn namespaced_export_attribute_is_not_treated_as_export() {
        for src in [
            "@export.csv\nlet f x = x\n",
            "@export.csv \"out\"\nlet f x = x\n",
        ] {
            let parsed = parse(src, FileId::new(0));
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let attr = parsed
                .tree
                .nodes()
                .find(|n| n.kind == K::Attribute)
                .expect("attribute");
            assert!(!is_export(attr, src), "{src}");
            assert_eq!(read(attr, src), None, "{src}");
        }
    }
}
