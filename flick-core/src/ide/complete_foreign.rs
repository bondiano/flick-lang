//! Completions for foreign metadata words, argument choices, and `@`
//! attributes driven by the package's active backend descriptor.

use super::complete::{Completion, CompletionKind, item};
use super::{Ide, tree};
use crate::source::FileId;
use crate::syntax::cst::{Node, foreign_words};
use crate::syntax::foreign_meta::{AdapterArgKind, AdapterWordSpec, ForeignMeta};
use crate::syntax::kind::SyntaxKind as K;

const CORE_FOREIGN_DOCS: &[(&str, &str, Option<&str>)] = &[
    (
        "arity",
        "Number of positional arguments the foreign call takes (`arity n`).",
        Some("arity $1"),
    ),
    (
        "catch",
        "Map a thrown host exception or rejected promise to a typed error (`catch fn`).",
        Some("catch $1"),
    ),
    (
        "retention",
        "How long the foreign call keeps scope capabilities (`retention call` or `retention owner`).",
        Some("retention ${1|call,owner|}"),
    ),
    (
        "nullable",
        "Adapt a host `null` / `undefined` / `nil` result into `Option a`.",
        None,
    ),
    (
        "optional",
        "Omit an `Option a` argument at 0-based position `i` when `None` (`optional i`).",
        Some("optional $1"),
    ),
    (
        "reviewed",
        "Record the audited host package version and fingerprint (`reviewed \"pkg@ver sha256:...\"`).",
        Some("reviewed \"$1\""),
    ),
    (
        "owned",
        "Transfer ownership of argument at 0-based position `i` to the host callee (`owned i`).",
        Some("owned $1"),
    ),
];

const CORE_ATTR_DOCS: &[(&str, &str)] = &[
    (
        "lang",
        "Register a compiler lang item (built-in modules only).",
    ),
    ("derive", "Mark a class as derivable with `deriving`."),
    (
        "maker",
        "Mark a value as a maker of an atom, action, or view.",
    ),
    ("cell", "Mark a type whose values hold mutable state."),
    (
        "label",
        "Label top-level values of this type for debugging tools.",
    ),
    (
        "owner",
        "Mark a service whose handler owns resources created under it.",
    ),
    ("sync", "Mark a service whose operations run synchronously."),
    (
        "stores",
        "Mark a nominal type whose host values may store closures.",
    ),
    (
        "implicit",
        "Opt a ground conversion instance into implicit conversion.",
    ),
    (
        "when",
        "Restrict this module to matching target/host environment tags.",
    ),
];

impl Ide<'_> {
    /// Returns backend-scoped completions when `offset` is at an `@` attribute
    /// position or inside a `foreign` metadata block; `None` otherwise so
    /// normal expression/name completion runs.
    pub(super) fn foreign_or_attr_completions(
        &self,
        file: FileId,
        offset: u32,
    ) -> Option<Vec<Completion>> {
        let m = self.module(file)?;
        let text = self.text(file);
        if let Some(attrs) = attribute_completions(text, offset, &m.foreign_meta) {
            return Some(attrs);
        }
        foreign_meta_completions(&m.tree, text, offset, &m.foreign_meta)
    }
}

fn attribute_completions(text: &str, offset: u32, meta: &ForeignMeta) -> Option<Vec<Completion>> {
    let at = (offset as usize).min(text.len());
    let before = text[..at].trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_');
    let prefix = before.strip_suffix('@')?;
    // Ensure `@` starts an attribute rather than sitting inside an identifier.
    if prefix
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    let mut items = Vec::new();
    for &(name, doc) in CORE_ATTR_DOCS {
        let mut c = item(name, CompletionKind::Keyword, Some("core attribute".into()));
        c.documentation = Some(doc.to_owned());
        c.sort = 1;
        items.push(c);
    }
    for attr in &meta.type_form_attributes {
        let mut c = item(
            &attr.name,
            CompletionKind::Keyword,
            Some(format!("{} type-form attribute", meta.backend_id)),
        );
        c.documentation = Some(attr.help.clone());
        c.sort = 0;
        items.push(c);
    }
    Some(items)
}

fn foreign_meta_completions(
    tree: &Node,
    text: &str,
    offset: u32,
    meta: &ForeignMeta,
) -> Option<Vec<Completion>> {
    let path = tree::path_to(tree, offset);
    if let Some(meta_item) = path.iter().rev().find(|n| n.kind == K::MetaItem) {
        let first = meta_item.child_tokens().find(|t| t.kind == K::Ident);
        if first.is_none_or(|tok| offset <= tok.end) {
            return Some(word_completions(meta));
        }
        let first = first.expect("checked above");
        let word = first.text(text);
        let arg_index = meta_item
            .nodes()
            .filter(|arg| arg.range().1 <= offset)
            .count();
        return arg_completions(word, arg_index, meta);
    }

    // Handle a blank or partially typed indented line directly following a
    // non-blocked `foreign` declaration (where the parser ended `ForeignDecl`
    // before the trailing whitespace).
    let at = (offset as usize).min(text.len());
    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let line_before = &text[line_start..at];
    let indent = line_before
        .bytes()
        .take_while(|b| *b == b' ' || *b == b'\t')
        .count();
    if indent == 0 {
        return None;
    }
    let foreign_decl = preceding_foreign(tree, text, u32::try_from(line_start).ok()?)?;
    if foreign_words(foreign_decl)
        .0
        .is_some_and(|w| w.text(text) == "blocked")
    {
        return None;
    }
    let trimmed = line_before.trim_start();
    if trimmed.is_empty() {
        return Some(word_completions(meta));
    }
    let mut parts = trimmed.split_whitespace();
    let head = parts.next().unwrap_or("");
    let ends_with_space = line_before.ends_with([' ', '\t']);
    let completed_args = parts.count();
    if completed_args == 0 && !ends_with_space {
        return Some(word_completions(meta));
    }
    let arg_index = if ends_with_space {
        completed_args
    } else {
        completed_args.saturating_sub(1)
    };
    arg_completions(head, arg_index, meta)
}

fn preceding_foreign<'a>(tree: &'a Node, text: &str, line_start: u32) -> Option<&'a Node> {
    let mut last_foreign = None;
    for decl in tree.nodes() {
        let (start, end) = decl.range();
        if start > line_start {
            break;
        }
        if decl.kind == K::ForeignDecl && start <= line_start && line_start <= end {
            return Some(decl);
        }
        if decl.kind == K::ForeignDecl && end <= line_start {
            // Confirm nothing non-whitespace separates the declaration from the current line.
            let between = &text[end as usize..line_start as usize];
            if between.trim().is_empty() {
                last_foreign = Some(decl);
            } else {
                last_foreign = None;
            }
        } else {
            last_foreign = None;
        }
    }
    last_foreign
}

fn word_completions(meta: &ForeignMeta) -> Vec<Completion> {
    let mut items = Vec::new();
    for &(name, doc, snippet) in CORE_FOREIGN_DOCS {
        let mut c = item(name, CompletionKind::Keyword, Some("core contract".into()));
        c.documentation = Some(doc.to_owned());
        c.snippet = snippet.map(str::to_owned);
        c.sort = 1;
        items.push(c);
    }
    for word in &meta.adapter_words {
        items.push(adapter_word_completion(word, &meta.backend_id));
    }
    items
}

fn adapter_word_completion(word: &AdapterWordSpec, backend_id: &str) -> Completion {
    let sig = adapter_signature(word);
    let detail = if backend_id.is_empty() {
        sig
    } else {
        format!("{sig} ({backend_id})")
    };
    let mut c = item(&word.name, CompletionKind::Keyword, Some(detail));
    c.documentation = Some(word.help.clone());
    c.snippet = adapter_snippet(word);
    c.sort = 0;
    c
}

fn adapter_signature(word: &AdapterWordSpec) -> String {
    if word.args.is_empty() {
        return word.name.clone();
    }
    let args: Vec<String> = word
        .args
        .iter()
        .map(|arg| {
            if arg.choices.is_empty() {
                match arg.kind {
                    AdapterArgKind::Index | AdapterArgKind::Ident => format!("<{}>", arg.name),
                    AdapterArgKind::Text => format!("\"<{}>\"", arg.name),
                }
            } else {
                arg.choices.join("|")
            }
        })
        .collect();
    format!("{} {}", word.name, args.join(" "))
}

fn adapter_snippet(word: &AdapterWordSpec) -> Option<String> {
    if word.args.is_empty() {
        return None;
    }
    let mut parts = vec![word.name.clone()];
    for (i, arg) in word.args.iter().enumerate() {
        let slot = i + 1;
        if let [only] = arg.choices.as_slice() {
            match arg.kind {
                AdapterArgKind::Text => parts.push(format!("\"{only}\"")),
                AdapterArgKind::Ident | AdapterArgKind::Index => parts.push(only.clone()),
            }
        } else if !arg.choices.is_empty() {
            parts.push(format!("${{{slot}|{}|}}", arg.choices.join(",")));
        } else {
            match arg.kind {
                AdapterArgKind::Text => parts.push(format!("\"${{{slot}:{}}}\"", arg.name)),
                AdapterArgKind::Index | AdapterArgKind::Ident => {
                    parts.push(format!("${{{slot}:{}}}", arg.name));
                }
            }
        }
    }
    Some(parts.join(" "))
}

fn arg_completions(word: &str, arg_index: usize, meta: &ForeignMeta) -> Option<Vec<Completion>> {
    match word {
        // `catch <expr>` takes a Flick expression; fall through to normal completions.
        "catch" => None,
        "retention" if arg_index == 0 => {
            let mut call = item(
                "call",
                CompletionKind::Keyword,
                Some("retention call".into()),
            );
            call.documentation = Some("Capabilities are used only during the call.".into());
            let mut owner = item(
                "owner",
                CompletionKind::Keyword,
                Some("retention owner".into()),
            );
            owner.documentation =
                Some("Capabilities are kept until the owning scope closes.".into());
            Some(vec![call, owner])
        }
        other => {
            let Some(spec) = meta.adapter_word(other) else {
                return Some(Vec::new());
            };
            let Some(arg) = spec.args.get(arg_index) else {
                return Some(Vec::new());
            };
            let choices = arg
                .choices
                .iter()
                .map(|choice| {
                    let mut c = item(
                        choice,
                        CompletionKind::Keyword,
                        Some(format!("{} {}", spec.name, choice)),
                    );
                    c.documentation = Some(spec.help.clone());
                    c
                })
                .collect();
            Some(choices)
        }
    }
}
