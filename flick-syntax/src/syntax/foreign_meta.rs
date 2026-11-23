//! Core contract words vs backend-declared foreign adapter words and
//! type-form attributes (`FLICK-S011`, `FLICK-S014`).
//!
//! The parser stays backend-agnostic and parses any `<ident> <args...>` line
//! under `foreign` as a `MetaItem` and any `@<ident> ...` as an `Attribute`.
//! This module validates foreign metadata words and non-core attributes against
//! the active backend's declared schema.

use std::fmt::Write as _;

use super::cst::{Element, Node};
use super::heads::{CHECKED_BY, INVARIANT_OF};
use super::kind::SyntaxKind as K;
use super::ts_mark;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, Span};

/// Foreign metadata contract words owned by the Flick compiler core.
pub const CORE_FOREIGN_META: [&str; 7] = [
    "arity",
    "catch",
    "retention",
    "nullable",
    "optional",
    "reviewed",
    "owned",
];

/// Core compiler attributes owned by the Flick compiler core (excluding
/// backend-declared type-form attributes such as `@ts`).
pub const CORE_ATTRIBUTES: [&str; 10] = [
    "lang", "derive", "maker", "cell", "label", "owner", "sync", "stores", "implicit", "when",
];

/// The TypeScript type-form attribute help text published by the JS backend.
pub const TS_ATTR_HELP: &str = "Says what the type or service declared next is in TypeScript: it is not declared in `.d.ts`, and every module that names it writes this form. In a form `{0}` is the first argument of the type, `{0:row}` that argument as a row, and `$Export` an export that `@ts import \"module\" \"Export\"` brings from a package or from a file beside the module. `@ts none` says the type has no form. The built-in types have their forms from the backend. The compiler does not check that a form is true of the values.";

/// Syntactic shape of one positional argument of a backend-declared foreign
/// adapter word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterArgKind {
    /// A non-negative integer literal (for example a 0-based parameter index).
    Index,
    /// An identifier word (restricted to `choices` when non-empty).
    Ident,
    /// A string literal (restricted to `choices` when non-empty).
    Text,
}

/// One positional argument of a backend-declared foreign adapter word.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdapterArgSpec {
    pub name: String,
    pub kind: AdapterArgKind,
    pub choices: Vec<String>,
}

/// One backend-declared foreign adapter word and its argument shape.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdapterWordSpec {
    pub name: String,
    pub args: Vec<AdapterArgSpec>,
    pub repeatable: bool,
    pub help: String,
}

/// One backend-declared type-form attribute (for example `@ts` on JS).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeFormAttrSpec {
    pub name: String,
    pub help: String,
}

/// Foreign adapter words and type-form attributes declared by one backend.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ForeignMeta {
    pub backend_id: String,
    pub adapter_words: Vec<AdapterWordSpec>,
    pub type_form_attributes: Vec<TypeFormAttrSpec>,
}

impl ForeignMeta {
    /// Metadata declared by the official `flick/js` backend.
    /// Must stay in sync with `backends/js/src/component.rs`.
    pub fn js() -> Self {
        Self {
            backend_id: "flick/js".into(),
            adapter_words: vec![
                AdapterWordSpec {
                    name: "this".into(),
                    args: vec![],
                    repeatable: false,
                    help: "Pass the first argument as the JavaScript `this` receiver (`target.call(a0, a1, …)`).".into(),
                },
                AdapterWordSpec {
                    name: "cancel".into(),
                    args: vec![AdapterArgSpec {
                        name: "mode".into(),
                        kind: AdapterArgKind::Ident,
                        choices: vec!["signal".into()],
                    }],
                    repeatable: false,
                    help: "Pass an `AbortSignal` as the last argument of an `Async` call, aborted on interruption.".into(),
                },
                AdapterWordSpec {
                    name: "variadic".into(),
                    args: vec![],
                    repeatable: false,
                    help: "Spread the last `List a` argument into the JavaScript call (`...xs`).".into(),
                },
            ],
            type_form_attributes: vec![TypeFormAttrSpec {
                name: ts_mark::TAG.into(),
                help: TS_ATTR_HELP.into(),
            }],
        }
    }

    /// Metadata declared by the official `flick/go` backend.
    /// Must stay in sync with `backends/go/src/component.rs`.
    pub fn go() -> Self {
        Self {
            backend_id: "flick/go".into(),
            adapter_words: vec![
                AdapterWordSpec {
                    name: "this".into(),
                    args: vec![],
                    repeatable: false,
                    help: "Call the exported Go method on the first argument via `flickrt.CallMethod`.".into(),
                },
                AdapterWordSpec {
                    name: "cancel".into(),
                    args: vec![AdapterArgSpec {
                        name: "mode".into(),
                        kind: AdapterArgKind::Ident,
                        choices: vec!["signal".into()],
                    }],
                    repeatable: false,
                    help: "Deliver interruption on an `Async` call through its `flickrt.Context`.".into(),
                },
                AdapterWordSpec {
                    name: "variadic".into(),
                    args: vec![],
                    repeatable: false,
                    help: "Spread the last `List a` argument into Go variadic parameters (`flickrt.List(xs)...`).".into(),
                },
            ],
            type_form_attributes: vec![],
        }
    }

    /// Look up a backend-declared adapter word by name.
    pub fn adapter_word(&self, name: &str) -> Option<&AdapterWordSpec> {
        self.adapter_words.iter().find(|w| w.name == name)
    }

    /// Look up a backend-declared type-form attribute by name.
    pub fn type_form_attr(&self, name: &str) -> Option<&TypeFormAttrSpec> {
        self.type_form_attributes.iter().find(|a| a.name == name)
    }

    /// Whether `name` is declared as a type-form attribute by this backend.
    pub fn has_type_form_attr(&self, name: &str) -> bool {
        self.type_form_attr(name).is_some()
    }

    /// All valid foreign metadata words (core contract words plus this
    /// backend's adapter words).
    pub fn expected_foreign_words(&self) -> Vec<&str> {
        let mut out: Vec<&str> = CORE_FOREIGN_META.to_vec();
        for word in &self.adapter_words {
            if !out.contains(&word.name.as_str()) {
                out.push(word.name.as_str());
            }
        }
        out
    }

    /// All valid attribute spellings (`@name`) for this backend, preserving
    /// the canonical order (`@lang`..`@sync`, backend type-form attributes,
    /// `@stores`, `@implicit`, `@when`).
    pub fn expected_attributes(&self) -> Vec<String> {
        let mut out: Vec<String> = ["lang", "derive", "maker", "cell", "label", "owner", "sync"]
            .into_iter()
            .map(|a| format!("`@{a}`"))
            .collect();
        for attr in &self.type_form_attributes {
            out.push(format!("`@{}`", attr.name));
        }
        for tail in ["stores", "implicit", "when"] {
            out.push(format!("`@{tail}`"));
        }
        out
    }

    /// Stable string fingerprint covering backend id, adapter words, argument
    /// shapes, and type-form attributes for compiler cache keys.
    pub fn fingerprint(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "backend={}", self.backend_id);
        for w in &self.adapter_words {
            let _ = write!(out, "w:{}:{}", w.name, w.repeatable);
            for a in &w.args {
                let kind = match a.kind {
                    AdapterArgKind::Index => "i",
                    AdapterArgKind::Ident => "w",
                    AdapterArgKind::Text => "s",
                };
                let _ = write!(out, ":{kind}:{}:[{}]", a.name, a.choices.join(","));
            }
            out.push('\n');
        }
        for a in &self.type_form_attributes {
            let _ = writeln!(out, "a:{}", a.name);
        }
        out
    }
}

/// Whether `name` is a core compiler attribute (or compiler-generated internal
/// attribute) validated unconditionally by `validate_attrs`.
pub fn is_core_attribute(name: &str) -> bool {
    CORE_ATTRIBUTES.contains(&name)
        || (name.contains('$') && (CHECKED_BY.ends_with(name) || INVARIANT_OF.ends_with(name)))
}

/// Validates every foreign metadata word (`FLICK-S011`) and every non-core
/// attribute (`FLICK-S014`, including backend type-form attributes such as
/// `@ts`) in `tree` against `meta`.
pub fn validate_tree(
    tree: &Node,
    src: &str,
    file: FileId,
    meta: &ForeignMeta,
    allow_trailing_attribute: bool,
    errors: &mut Vec<Diagnostic>,
) {
    enum Work<'a> {
        Node(&'a Node),
        Attr { node: &'a Node, next: Option<K> },
    }

    let mut stack = vec![Work::Node(tree)];
    while let Some(work) = stack.pop() {
        let node = match work {
            Work::Attr { node, next } => {
                check_attribute(
                    src,
                    file,
                    allow_trailing_attribute,
                    node,
                    next,
                    meta,
                    errors,
                );
                continue;
            }
            Work::Node(node) => node,
        };
        if node.kind == K::ForeignMeta {
            for item in node.nodes().filter(|n| n.kind == K::MetaItem) {
                let Some(name) = item.child_tokens().find(|t| t.kind == K::Ident) else {
                    continue;
                };
                let word = name.text(src);
                if !CORE_FOREIGN_META.contains(&word) && meta.adapter_word(word).is_none() {
                    errors.push(Diagnostic::error(
                        codes::S011,
                        format!(
                            "unknown foreign metadata `{word}`; expected one of: {}",
                            meta.expected_foreign_words().join(", ")
                        ),
                        Span::new(file, name.start, name.end),
                    ));
                }
            }
            continue;
        }
        let mut next_non_attr = None;
        for element in node.children.iter().rev() {
            let Element::Node(child) = element else {
                continue;
            };
            if child.kind == K::Attribute {
                stack.push(Work::Attr {
                    node: child,
                    next: next_non_attr,
                });
            } else {
                next_non_attr = Some(child.kind);
                stack.push(Work::Node(child));
            }
        }
    }
}

fn check_attribute(
    src: &str,
    file: FileId,
    allow_trailing_attribute: bool,
    node: &Node,
    next: Option<K>,
    meta: &ForeignMeta,
    errors: &mut Vec<Diagnostic>,
) {
    let Some(name_tok) = node.child_tokens().find(|t| t.kind == K::Ident) else {
        return;
    };
    let name = name_tok.text(src);
    if is_core_attribute(name) {
        return;
    }
    let valid_place = matches!(next, Some(K::TypeDecl | K::ServiceDecl | K::Attribute))
        || (next.is_none() && allow_trailing_attribute);
    let message = if name == ts_mark::TAG && meta.has_type_form_attr(ts_mark::TAG) {
        match ts_mark::read(node, src) {
            Some(Err(why)) => why,
            _ if valid_place => return,
            _ => "`@ts` belongs before the declaration of a type or a service".to_owned(),
        }
    } else if meta.has_type_form_attr(name) {
        if valid_place {
            return;
        }
        format!("`@{name}` belongs before the declaration of a type or a service")
    } else {
        format!(
            "unknown attribute `@{name}`: the attributes are {}",
            format_list(&meta.expected_attributes())
        )
    };
    let (start, end) = node.range();
    errors.push(Diagnostic::error(
        codes::S014,
        message,
        Span::new(file, start, end),
    ));
}

fn format_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [head @ .., last] => format!("{} and {last}", head.join(", ")),
    }
}
