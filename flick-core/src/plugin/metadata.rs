//! Host-side validation of [`flick_plugin::PluginMetadata`] declared by
//! plugins. First-party `std` may declare compiler-owned diagnostic codes;
//! external plugins cannot declare `own_codes`, collide with `std` or `result`
//! heads, or supply malformed head/watch/derive specifications.

use std::collections::HashSet;

use flick_plugin::{AttributeSpec, HeadSpec, PluginMetadata};

use super::first_party;
use crate::diagnostics::codes::{self, Code};
use crate::iface;
use crate::source::FileId;
use crate::syntax::HeadCatalog;
use crate::syntax::kind::SyntaxKind;
use crate::syntax::lexer::lex;
use crate::types::is_reserved_core_attribute;

/// A single validation problem in a plugin's [`PluginMetadata`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataIssue {
    pub code: Code,
    pub message: String,
}

/// Validates `metadata` declared by the plugin of `package`.
#[must_use]
pub fn validate(
    package: &str,
    metadata: &PluginMetadata,
    std_catalog: &HeadCatalog,
) -> Vec<MetadataIssue> {
    let mut issues = Vec::new();
    let is_std = package == first_party::PACKAGE;

    if !is_std && !metadata.own_codes.is_empty() {
        issues.push(MetadataIssue {
            code: codes::P036,
            message: format!(
                "plugin `{package}` declares `own_codes` ({}), which is reserved for `{}`",
                metadata.own_codes.join(", "),
                first_party::PACKAGE
            ),
        });
    }

    let mut seen_heads = HashSet::new();
    for spec in &metadata.heads {
        validate_head(
            package,
            is_std,
            spec,
            std_catalog,
            &mut seen_heads,
            &mut issues,
        );
    }

    let mut seen_watch = HashSet::new();
    for entry in &metadata.watch {
        if !is_watched(entry) {
            issues.push(MetadataIssue {
                code: codes::P036,
                message: format!(
                    "plugin `{package}` declared invalid watched name `{entry}`: expected `Module.name`, `*.name`, `Module.Service`, `@maker`, or `@cell`"
                ),
            });
        } else if !seen_watch.insert(entry.as_str()) {
            issues.push(MetadataIssue {
                code: codes::P036,
                message: format!(
                    "plugin `{package}` declared duplicate watched name `{entry}` in metadata"
                ),
            });
        }
    }

    let mut seen_derives = HashSet::new();
    for derive in &metadata.derives {
        if !is_qualified_type(derive) {
            issues.push(MetadataIssue {
                code: codes::P036,
                message: format!(
                    "plugin `{package}` declared invalid derive class `{derive}`: expected a qualified name such as `Module.Class`"
                ),
            });
        } else if !seen_derives.insert(derive.as_str()) {
            issues.push(MetadataIssue {
                code: codes::P036,
                message: format!(
                    "plugin `{package}` declared duplicate derive class `{derive}` in metadata"
                ),
            });
        }
    }

    let mut seen_attributes = HashSet::new();
    for spec in &metadata.attributes {
        validate_attribute(package, is_std, spec, &mut seen_attributes, &mut issues);
    }

    issues
}

fn validate_attribute<'a>(
    package: &str,
    is_std: bool,
    spec: &'a AttributeSpec,
    seen_attributes: &mut HashSet<&'a str>,
    issues: &mut Vec<MetadataIssue>,
) {
    if !is_valid_attribute_name(&spec.name) {
        issues.push(MetadataIssue {
            code: codes::P036,
            message: format!(
                "plugin `{package}` declared invalid attribute `@{}`: expected `name` or `namespace.name` of lowercase identifiers",
                spec.name
            ),
        });
        return;
    }
    if is_reserved_core_attribute(&spec.name) {
        issues.push(MetadataIssue {
            code: codes::P044,
            message: format!(
                "plugin `{package}` cannot declare attribute `@{}`: `@{}` is a built-in compiler attribute",
                spec.name, spec.name
            ),
        });
        return;
    }
    if !is_std
        && first_party::metadata()
            .attributes
            .iter()
            .any(|a| a.name == spec.name)
    {
        issues.push(MetadataIssue {
            code: codes::P044,
            message: format!(
                "`@{}` is an attribute of `{}`'s plugin, which every module has, so the plugin of `{package}` cannot declare it",
                spec.name,
                first_party::PACKAGE
            ),
        });
        return;
    }
    if !seen_attributes.insert(spec.name.as_str()) {
        issues.push(MetadataIssue {
            code: codes::P036,
            message: format!(
                "plugin `{package}` declared duplicate attribute `@{}` in metadata",
                spec.name
            ),
        });
    }
    if spec.targets.is_empty() {
        issues.push(MetadataIssue {
            code: codes::P036,
            message: format!(
                "plugin `{package}` declared attribute `@{}` with no targets",
                spec.name
            ),
        });
    } else {
        let mut seen_targets = HashSet::new();
        if spec.targets.iter().any(|t| !seen_targets.insert(*t)) {
            issues.push(MetadataIssue {
                code: codes::P036,
                message: format!(
                    "plugin `{package}` declared attribute `@{}` with duplicate targets",
                    spec.name
                ),
            });
        }
    }
}

/// Whether `name` is `ident` or `namespace.ident`, each segment a valid lowercase word.
fn is_valid_attribute_name(name: &str) -> bool {
    match name.split_once('.') {
        Some((ns, attr)) => !attr.contains('.') && is_valid_word(ns) && is_valid_word(attr),
        None => is_valid_word(name),
    }
}

fn validate_head<'a>(
    package: &str,
    is_std: bool,
    spec: &'a HeadSpec,
    std_catalog: &HeadCatalog,
    seen_heads: &mut HashSet<&'a str>,
    issues: &mut Vec<MetadataIssue>,
) {
    if spec.head == "result" {
        issues.push(MetadataIssue {
            code: codes::P044,
            message: format!(
                "plugin `{package}` cannot declare head `result`: `result` is a built-in block head (`result {{ … }}`)"
            ),
        });
        return;
    }
    if !is_valid_head_ident(&spec.head) {
        issues.push(MetadataIssue {
            code: codes::P036,
            message: format!(
                "plugin `{package}` declared invalid block head `{}`: expected a lowercase non-keyword identifier",
                spec.head
            ),
        });
        return;
    }
    if !is_std && std_catalog.is_implicit(&spec.head) {
        issues.push(MetadataIssue {
            code: codes::P044,
            message: format!(
                "`{}` is a head of `{}`'s plugin, which every module has, so the plugin of `{package}` cannot declare it",
                spec.head,
                first_party::PACKAGE
            ),
        });
        return;
    }
    if !seen_heads.insert(spec.head.as_str()) {
        issues.push(MetadataIssue {
            code: codes::P036,
            message: format!(
                "plugin `{package}` declared duplicate block head `{}` in metadata",
                spec.head
            ),
        });
    }
    if !is_module_path(&spec.module) {
        issues.push(MetadataIssue {
            code: codes::P036,
            message: format!(
                "plugin `{package}` declared head `{}` with invalid module `{}`: expected a capitalized module path",
                spec.head, spec.module
            ),
        });
    }
    if spec.top && spec.bare {
        issues.push(MetadataIssue {
            code: codes::P036,
            message: format!(
                "plugin `{package}` declared head `{}` with both `top` and `bare`, which are contradictory",
                spec.head
            ),
        });
    }
    let mut seen_decls = HashSet::new();
    for word in &spec.declarations {
        if !is_valid_word(word) {
            issues.push(MetadataIssue {
                code: codes::P036,
                message: format!(
                    "plugin `{package}` declared head `{}` with invalid declaration keyword `{word}`",
                    spec.head
                ),
            });
        } else if !seen_decls.insert(word.as_str()) {
            issues.push(MetadataIssue {
                code: codes::P036,
                message: format!(
                    "plugin `{package}` declared head `{}` with duplicate declaration keyword `{word}`",
                    spec.head
                ),
            });
        }
    }
}

/// Whether `name` is a valid lowercase identifier that is not a Flick keyword.
fn is_valid_head_ident(name: &str) -> bool {
    if !is_valid_word(name) {
        return false;
    }
    let (tokens, diagnostics) = lex(name, FileId::default());
    if !diagnostics.is_empty() {
        return false;
    }
    matches!(tokens.as_slice(), [tok] if tok.kind == SyntaxKind::Ident)
}

/// Whether `word` is a non-empty lowercase/underscore ASCII word (keywords like
/// `type` or `fn` are allowed as declaration second-words such as `refined type`).
fn is_valid_word(word: &str) -> bool {
    word.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && word.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Whether `path` is a non-empty dot-separated capitalized module path.
fn is_module_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('.').all(|seg| {
            seg.starts_with(|c: char| c.is_ascii_uppercase())
                && seg.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
}

/// Whether `name` is a qualified type/class name (`Module.Class` or `A.B.Class`).
fn is_qualified_type(name: &str) -> bool {
    name.contains('.') && is_module_path(name)
}

/// Whether `watched` names what a plugin is told of: a value of a module
/// (`Ui.Event.queue`, or `*.parse` for the name of any module), a service
/// (`Atom.Read`), or a tag the compiler follows (`@maker`, `@cell`).
#[must_use]
pub fn is_watched(watched: &str) -> bool {
    if let Some(tag) = watched.strip_prefix('@') {
        return tag == iface::MAKER || tag == iface::CELL;
    }
    let Some((module, name)) = watched.rsplit_once('.') else {
        return false;
    };
    let value = |n: &str| {
        n.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
            && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    };
    let part = |p: &str| {
        p.starts_with(|c: char| c.is_ascii_uppercase())
            && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    };
    (value(name) || part(name)) && (module == "*" || module.split('.').all(part))
}

#[cfg(test)]
mod tests {
    use flick_plugin::{AttributeSpec, AttributeTarget, HeadSpec, PluginMetadata};

    use super::validate;
    use crate::diagnostics::codes;
    use crate::plugin::first_party;

    #[test]
    fn std_metadata_is_valid() {
        let issues = validate(
            first_party::PACKAGE,
            first_party::metadata(),
            first_party::head_catalog(),
        );
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn external_plugin_rejects_own_codes_conflicts_and_malformed_specs() {
        let bad = PluginMetadata {
            heads: vec![
                HeadSpec {
                    head: "sql".into(),
                    module: "Other".into(),
                    ..HeadSpec::default()
                },
                HeadSpec {
                    head: "result".into(),
                    module: "Other".into(),
                    ..HeadSpec::default()
                },
                HeadSpec {
                    head: "bad_combo".into(),
                    module: "Other".into(),
                    top: true,
                    bare: true,
                    ..HeadSpec::default()
                },
                HeadSpec {
                    head: "BadCap".into(),
                    module: "not_cap".into(),
                    ..HeadSpec::default()
                },
            ],
            watch: vec!["not_valid".into()],
            derives: vec!["Unqualified".into()],
            own_codes: vec!["U001".into()],
            attributes: vec![
                AttributeSpec {
                    name: "BadCap".into(),
                    targets: vec![AttributeTarget::Declaration],
                    doc: String::new(),
                },
                AttributeSpec {
                    name: "a.b.c".into(),
                    targets: vec![AttributeTarget::Declaration],
                    doc: String::new(),
                },
                AttributeSpec {
                    name: "cell".into(),
                    targets: vec![AttributeTarget::Declaration],
                    doc: String::new(),
                },
                AttributeSpec {
                    name: "empty_targets".into(),
                    targets: Vec::new(),
                    doc: String::new(),
                },
                AttributeSpec {
                    name: "dup_targets".into(),
                    targets: vec![AttributeTarget::RecordField, AttributeTarget::RecordField],
                    doc: String::new(),
                },
                AttributeSpec {
                    name: "dup_attr".into(),
                    targets: vec![AttributeTarget::RecordField],
                    doc: String::new(),
                },
                AttributeSpec {
                    name: "dup_attr".into(),
                    targets: vec![AttributeTarget::Variant],
                    doc: String::new(),
                },
            ],
        };
        let issues = validate("ext", &bad, first_party::head_catalog());
        assert!(
            issues
                .iter()
                .any(|i| i.code == codes::P036 && i.message.contains("own_codes"))
        );
        assert!(
            issues
                .iter()
                .any(|i| i.code == codes::P044 && i.message.contains("`sql`"))
        );
        assert!(
            issues
                .iter()
                .any(|i| i.code == codes::P044 && i.message.contains("`result`"))
        );
        assert!(
            issues
                .iter()
                .any(|i| i.code == codes::P036 && i.message.contains("contradictory"))
        );
        assert!(
            issues
                .iter()
                .any(|i| i.code == codes::P036 && i.message.contains("invalid block head"))
        );
        assert!(
            issues
                .iter()
                .any(|i| i.code == codes::P036 && i.message.contains("invalid watched name"))
        );
        assert!(
            issues
                .iter()
                .any(|i| i.code == codes::P036 && i.message.contains("invalid derive class"))
        );
        assert!(
            issues
                .iter()
                .any(|i| i.code == codes::P036 && i.message.contains("`@BadCap`"))
        );
        assert!(
            issues
                .iter()
                .any(|i| i.code == codes::P036 && i.message.contains("`@a.b.c`"))
        );
        assert!(
            issues
                .iter()
                .any(|i| i.code == codes::P044 && i.message.contains("`@cell`"))
        );
        assert!(
            issues
                .iter()
                .any(|i| i.code == codes::P036 && i.message.contains("no targets"))
        );
        assert!(
            issues
                .iter()
                .any(|i| i.code == codes::P036 && i.message.contains("duplicate targets"))
        );
        assert!(
            issues
                .iter()
                .any(|i| i.code == codes::P036 && i.message.contains("duplicate attribute"))
        );
    }
}
