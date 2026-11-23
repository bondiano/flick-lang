//! Core foreign contract metadata words and backend-declared adapter /
//! type-form attribute specifications.

use std::fmt::Write as _;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Expected shape of one positional argument after a backend-declared foreign
/// adapter word. Mirrors `flick:backend/types.adapter-arg-kind`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AdapterArgKind {
    /// One of a closed set of lowercase keyword choices.
    Keyword(Vec<String>),
    /// A zero-based parameter index.
    ParamIndex,
    /// A non-negative integer literal.
    Uint,
    /// A string literal.
    Text,
    /// Any identifier word.
    Word,
}

/// One positional argument slot of a backend-declared foreign adapter word.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AdapterArgSpec {
    pub name: String,
    pub kind: AdapterArgKind,
}

/// A foreign metadata adapter word declared by a backend descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AdapterWordSpec {
    pub name: String,
    pub args: Vec<AdapterArgSpec>,
    pub repeatable: bool,
    pub doc: String,
}

impl AdapterWordSpec {
    /// Human-readable synopsis such as `cancel signal`, `cancel context | signal`,
    /// or `callback <fn> data <user_data>`.
    pub fn synopsis(&self) -> String {
        let mut out = self.name.clone();
        for arg in &self.args {
            out.push(' ');
            match &arg.kind {
                AdapterArgKind::Keyword(choices) => {
                    out.push_str(&choices.join(" | "));
                }
                AdapterArgKind::ParamIndex | AdapterArgKind::Uint | AdapterArgKind::Word => {
                    let _ = write!(out, "<{}>", arg.name);
                }
                AdapterArgKind::Text => {
                    let _ = write!(out, "\"<{}>\"", arg.name);
                }
            }
        }
        out
    }

    /// Snippet for IDE completion (`None` when the word takes no arguments).
    pub fn snippet(&self) -> Option<String> {
        if self.args.is_empty() {
            return None;
        }
        let mut out = self.name.clone();
        let mut tab = 1_usize;
        for arg in &self.args {
            out.push(' ');
            match &arg.kind {
                AdapterArgKind::Keyword(choices) if choices.len() == 1 => {
                    out.push_str(&choices[0]);
                }
                AdapterArgKind::Keyword(choices) => {
                    let _ = write!(out, "${{{tab}:{}}}", choices[0]);
                    tab += 1;
                }
                AdapterArgKind::ParamIndex | AdapterArgKind::Uint => {
                    let _ = write!(out, "${{{tab}:0}}");
                    tab += 1;
                }
                AdapterArgKind::Text => {
                    let _ = write!(out, "\"${{{tab}:{}}}\"", arg.name);
                    tab += 1;
                }
                AdapterArgKind::Word => {
                    let _ = write!(out, "${{{tab}:{}}}", arg.name);
                    tab += 1;
                }
            }
        }
        Some(out)
    }
}

/// A foreign type-form attribute declared by a backend descriptor (e.g. `@ts`, `@go`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeFormAttrSpec {
    pub name: String,
    pub doc: String,
}

/// Foreign metadata specification owned by the active package backend.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ForeignBackendSpec {
    pub backend_id: String,
    pub adapters: Vec<AdapterWordSpec>,
    pub type_form_attributes: Vec<TypeFormAttrSpec>,
}

impl ForeignBackendSpec {
    /// Looks up a backend-declared foreign adapter word by name.
    pub fn adapter(&self, name: &str) -> Option<&AdapterWordSpec> {
        self.adapters.iter().find(|word| word.name == name)
    }

    /// Looks up a backend-declared type-form attribute by name (without `@`).
    pub fn type_form_attribute(&self, name: &str) -> Option<&TypeFormAttrSpec> {
        self.type_form_attributes
            .iter()
            .find(|attr| attr.name == name)
    }

    /// The official `flick/js` backend's foreign adapter words and `@ts` attribute.
    /// Must stay in sync with `backends/js/src/component.rs`.
    pub fn js() -> Self {
        Self {
            backend_id: "flick/js".into(),
            adapters: vec![
                AdapterWordSpec {
                    name: "this".into(),
                    args: Vec::new(),
                    repeatable: false,
                    doc: "Pass the first argument as the JavaScript `this` receiver (`target.call(a0, ...)`).".into(),
                },
                AdapterWordSpec {
                    name: "cancel".into(),
                    args: vec![AdapterArgSpec {
                        name: "kind".into(),
                        kind: AdapterArgKind::Keyword(vec!["signal".into()]),
                    }],
                    repeatable: false,
                    doc: "Pass an `AbortSignal` as the last argument of an `Async` call, aborted on interruption.".into(),
                },
                AdapterWordSpec {
                    name: "variadic".into(),
                    args: Vec::new(),
                    repeatable: false,
                    doc: "Spread the last argument (a `List a`) into the JavaScript call.".into(),
                },
            ],
            type_form_attributes: vec![TypeFormAttrSpec {
                name: "ts".into(),
                doc: "Declare the TypeScript form of a foreign `type` or `service` (`@ts \"...\"`, `@ts from \"pkg\" \"Name\"`, or `@ts none`).".into(),
            }],
        }
    }

    /// The official `flick/go` backend's foreign adapter words and `@go` attribute.
    /// Must stay in sync with `backends/go/src/component.rs`.
    pub fn go() -> Self {
        Self {
            backend_id: "flick/go".into(),
            adapters: vec![
                AdapterWordSpec {
                    name: "this".into(),
                    args: Vec::new(),
                    repeatable: false,
                    doc: "Pass the first argument as the receiver of a Go method call (`flickrt.CallMethod`).".into(),
                },
                AdapterWordSpec {
                    name: "cancel".into(),
                    args: vec![AdapterArgSpec {
                        name: "kind".into(),
                        kind: AdapterArgKind::Keyword(vec!["context".into(), "signal".into()]),
                    }],
                    repeatable: false,
                    doc: "Receive cancellation via `flickrt.Context` on an `Async` foreign call (`cancel context`; `cancel signal` is accepted for portable bindings).".into(),
                },
                AdapterWordSpec {
                    name: "variadic".into(),
                    args: Vec::new(),
                    repeatable: false,
                    doc: "Spread the last argument (a `List a`) into Go variadic arguments.".into(),
                },
            ],
            type_form_attributes: vec![TypeFormAttrSpec {
                name: "go".into(),
                doc: "Declare the Go type form of a foreign `type` or `service` (`@go \"...\"`, `@go from \"pkg\" \"Name\"`, or `@go none`).".into(),
            }],
        }
    }

    /// Shared static reference to the default `flick/js` specification.
    pub fn default_js() -> &'static Self {
        static JS: OnceLock<ForeignBackendSpec> = OnceLock::new();
        JS.get_or_init(Self::js)
    }

    /// All foreign metadata words accepted under this backend: core contract
    /// words followed by backend-declared adapter words.
    pub fn all_foreign_words(&self) -> Vec<&str> {
        CORE_FOREIGN_META
            .iter()
            .map(|item| item.name)
            .chain(self.adapters.iter().map(|word| word.name.as_str()))
            .collect()
    }
}

impl Default for ForeignBackendSpec {
    fn default() -> Self {
        Self::js()
    }
}

/// One compiler-owned core foreign metadata word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreForeignMeta {
    pub name: &'static str,
    pub synopsis: &'static str,
    pub doc: &'static str,
    pub snippet: Option<&'static str>,
    pub keyword_choices: &'static [&'static str],
}

/// Core contract foreign metadata words owned by the compiler across all backends.
pub const CORE_FOREIGN_META: &[CoreForeignMeta] = &[
    CoreForeignMeta {
        name: "arity",
        synopsis: "arity <n>",
        doc: "Number of positional arguments the foreign function takes (`arity 0` drops a leading `Unit` parameter).",
        snippet: Some("arity ${1:1}"),
        keyword_choices: &[],
    },
    CoreForeignMeta {
        name: "catch",
        synopsis: "catch <fn>",
        doc: "Map a host exception or rejection to a typed error in the signature's error row.",
        snippet: Some("catch ${1:from_host}"),
        keyword_choices: &[],
    },
    CoreForeignMeta {
        name: "retention",
        synopsis: "retention call | owner",
        doc: "How long the foreign call may keep a scoped parameter (`call` = not past the call; `owner` = until the owning scope closes).",
        snippet: Some("retention ${1:call}"),
        keyword_choices: &["call", "owner"],
    },
    CoreForeignMeta {
        name: "nullable",
        synopsis: "nullable",
        doc: "Convert an absent host result (`null`, `undefined`, `nil`) to `Option.None`.",
        snippet: None,
        keyword_choices: &[],
    },
    CoreForeignMeta {
        name: "optional",
        synopsis: "optional <i>",
        doc: "Argument `i` (0-based) is `Option a`: `None` is omitted or passed as absent.",
        snippet: Some("optional ${1:0}"),
        keyword_choices: &[],
    },
    CoreForeignMeta {
        name: "reviewed",
        synopsis: "reviewed \"<fingerprint>\"",
        doc: "Record the package and fingerprint this foreign binding was audited against.",
        snippet: Some("reviewed \"${1:pkg@version}\""),
        keyword_choices: &[],
    },
    CoreForeignMeta {
        name: "owned",
        synopsis: "owned <i>",
        doc: "Argument `i` (0-based) transfers ownership to the foreign call.",
        snippet: Some("owned ${1:0}"),
        keyword_choices: &[],
    },
];

/// Looks up a core foreign contract metadata word by name.
pub fn core_foreign_meta(name: &str) -> Option<&'static CoreForeignMeta> {
    CORE_FOREIGN_META.iter().find(|item| item.name == name)
}
