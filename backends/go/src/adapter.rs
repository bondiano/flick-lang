//! Go backend-owned foreign adapter words (`this`, `cancel signal`, `variadic`):
//! helpers for emission and semantic validation with spans.

#[cfg(feature = "component")]
use std::collections::BTreeSet;

use flick_ir::{Adapter, AdapterArg, AdapterWord, ForeignMode, Span};
#[cfg(feature = "component")]
use flick_ir::{Foreign, IrType, ItemKind, Lang, LangItem, Namespace, Program};

/// Whether the foreign declaration uses the receiver method call (`this`).
pub fn has_this(adapter: &Adapter) -> bool {
    adapter.has_word("this")
}

/// Whether the foreign declaration declares `cancel signal`.
pub fn has_cancel_signal(adapter: &Adapter) -> bool {
    adapter
        .words_named("cancel")
        .any(|w| matches!(w.args.as_slice(), [AdapterArg::Ident(mode)] if mode == "signal"))
}

/// Whether the foreign declaration spreads its last `List a` argument (`variadic`).
pub fn has_variadic(adapter: &Adapter) -> bool {
    adapter.has_word("variadic")
}

/// One diagnostic reported while checking Go foreign adapter words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

/// Validates the Go backend-owned adapter words on one `foreign` declaration.
pub fn validate_foreign(
    mode: ForeignMode,
    arity: usize,
    words: &[AdapterWord],
    last_param_is_list: Option<bool>,
) -> Vec<AdapterDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen_this = false;
    let mut seen_cancel = false;
    let mut seen_variadic = false;

    for word in words {
        match word.name.as_str() {
            "this" => {
                if seen_this {
                    diagnostics.push(AdapterDiagnostic {
                        code: "F001",
                        message: "duplicate `this` in foreign metadata".into(),
                        span: word.span,
                    });
                    continue;
                }
                seen_this = true;
                if !word.args.is_empty() {
                    diagnostics.push(AdapterDiagnostic {
                        code: "F001",
                        message: "`this` takes no arguments".into(),
                        span: word.span,
                    });
                } else if arity == 0 {
                    diagnostics.push(AdapterDiagnostic {
                        code: "F001",
                        message: "`this` needs an argument to be the receiver".into(),
                        span: word.span,
                    });
                }
            }
            "cancel" => {
                if seen_cancel {
                    diagnostics.push(AdapterDiagnostic {
                        code: "F001",
                        message: "duplicate `cancel` in foreign metadata".into(),
                        span: word.span,
                    });
                    continue;
                }
                seen_cancel = true;
                let is_signal = matches!(
                    word.args.as_slice(),
                    [AdapterArg::Ident(mode)] if mode == "signal"
                );
                if !is_signal {
                    diagnostics.push(AdapterDiagnostic {
                        code: "F001",
                        message: "write `cancel signal`".into(),
                        span: word.span,
                    });
                } else if mode != ForeignMode::Async {
                    diagnostics.push(AdapterDiagnostic {
                        code: "F001",
                        message: "`cancel signal` is for a call whose row has `Async`: only a promise can be aborted".into(),
                        span: word.span,
                    });
                }
            }
            "variadic" => {
                if seen_variadic {
                    diagnostics.push(AdapterDiagnostic {
                        code: "F001",
                        message: "duplicate `variadic` in foreign metadata".into(),
                        span: word.span,
                    });
                    continue;
                }
                seen_variadic = true;
                if !word.args.is_empty() {
                    diagnostics.push(AdapterDiagnostic {
                        code: "F001",
                        message: "`variadic` takes no arguments".into(),
                        span: word.span,
                    });
                } else if arity == 0 || last_param_is_list == Some(false) {
                    diagnostics.push(AdapterDiagnostic {
                        code: "F001",
                        message: "`variadic` spreads a last argument of type `List a`".into(),
                        span: word.span,
                    });
                }
            }
            other => {
                diagnostics.push(AdapterDiagnostic {
                    code: "S011",
                    message: format!(
                        "unknown foreign metadata `{other}`; expected one of: arity, catch, retention, nullable, optional, reviewed, owned, this, cancel, variadic"
                    ),
                    span: word.span,
                });
            }
        }
    }

    diagnostics
}

#[cfg(feature = "component")]
fn last_param_is_list(foreign: &Foreign, list_lang: Option<&LangItem>) -> Option<bool> {
    if foreign.arity == 0 {
        return Some(false);
    }
    let ty = foreign.ty.as_ref()?;
    let list_type = list_lang?;
    let mut cur: &IrType = ty;
    let mut params = Vec::new();
    loop {
        match cur {
            IrType::Forall { body, .. } => cur = body,
            IrType::Fun { param, result, .. } => {
                params.push(param);
                cur = result;
            }
            _ => break,
        }
    }
    let Some(final_param) = params.get(foreign.arity - 1) else {
        return Some(false);
    };
    Some(final_param.is_named(&list_type.module, &list_type.name))
}

/// Validates Go foreign adapter words across all `own_modules` in `program`.
#[cfg(feature = "component")]
pub fn validate_program(
    program: &Program,
    own_modules: &BTreeSet<&str>,
) -> Vec<flick_backend::Diagnostic> {
    let list_lang = program
        .lang
        .iter()
        .find(|item| item.lang == Lang::List.name() && item.namespace == Namespace::Type);
    let mut out = Vec::new();
    for module in &program.modules {
        if !own_modules.contains(module.name.as_str()) {
            continue;
        }
        for item in &module.items {
            let ItemKind::Foreign(foreign) = &item.kind else {
                continue;
            };
            if foreign.adapter.words.is_empty() {
                continue;
            }
            let is_list = last_param_is_list(foreign, list_lang);
            for diag in
                validate_foreign(foreign.mode, foreign.arity, &foreign.adapter.words, is_list)
            {
                out.push(flick_backend::Diagnostic {
                    code: diag.code.into(),
                    severity: flick_backend::Severity::Error,
                    message: diag.message,
                    span: Some(flick_backend::Span {
                        file: diag.span.file.index(),
                        start: diag.span.start,
                        end: diag.span.end,
                    }),
                    notes: vec![],
                });
            }
        }
    }
    out
}
