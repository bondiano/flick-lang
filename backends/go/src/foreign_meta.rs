//! Go-owned foreign adapter metadata validation.
//!
//! Shared between the `flick/go` component's `check-target` / `generate` path
//! and `flick check` / IDE in `flick-core`.

use std::collections::BTreeSet;

use flick_ir::{
    AdapterArg, Foreign, ForeignMode, IrType, ItemKind, LangItem, Namespace, Program, Span,
};

/// A Go-backend diagnostic on a foreign adapter word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

fn err(code: &'static str, message: impl Into<String>, span: Span) -> ForeignDiagnostic {
    ForeignDiagnostic {
        code,
        message: message.into(),
        span,
    }
}

fn list_type_name(lang: &[LangItem]) -> (&str, &str) {
    lang.iter()
        .find(|item| item.lang == "list" && item.namespace == Namespace::Type)
        .map_or(("Prelude", "List"), |item| {
            (item.module.as_str(), item.name.as_str())
        })
}

fn is_list_param(
    foreign: &Foreign,
    index: usize,
    list_type: (&str, &str),
    list_params: Option<&[bool]>,
) -> bool {
    if let Some(flags) = list_params {
        return flags.get(index).copied().unwrap_or(false);
    }
    let Some(mut ty) = foreign.ty.as_ref() else {
        return false;
    };
    while let IrType::Forall { body, .. } = &**ty {
        ty = body;
    }
    let mut current = 0;
    while let IrType::Fun { param, result, .. } = &**ty {
        if current == index {
            return matches!(
                &**param,
                IrType::Named { module, name, args }
                    if module.as_str() == list_type.0
                        && name.as_str() == list_type.1
                        && args.len() == 1
            );
        }
        current += 1;
        ty = result;
    }
    false
}

/// Validate every backend-declared adapter word on a single `foreign` binding.
pub fn validate_foreign(
    foreign: &Foreign,
    list_type: (&str, &str),
    list_params: Option<&[bool]>,
    diagnostics: &mut Vec<ForeignDiagnostic>,
) {
    let (mut seen_this, mut seen_cancel, mut seen_variadic) = (false, false, false);
    for word in &foreign.adapter.words {
        match word.name.as_str() {
            "this" => {
                if !word.args.is_empty() {
                    diagnostics.push(err("F001", "`this` takes no arguments", word.span));
                } else if seen_this {
                    diagnostics.push(err("F001", "duplicate foreign metadata `this`", word.span));
                } else {
                    seen_this = true;
                    if foreign.binding.inline().is_some() {
                        diagnostics.push(err(
                            "F001",
                            "`this` is not supported on inline Go foreign bindings; pass the receiver as a parameter",
                            word.span,
                        ));
                    } else if foreign.arity == 0 {
                        diagnostics.push(err(
                            "F001",
                            "`this` needs an argument to be the receiver",
                            word.span,
                        ));
                    }
                }
            }
            "cancel" => {
                let kind = match word.args.as_slice() {
                    [AdapterArg::Word(kind)] if matches!(kind.as_str(), "context" | "signal") => {
                        Some(kind.as_str())
                    }
                    _ => None,
                };
                let Some(kind) = kind else {
                    diagnostics.push(err(
                        "F001",
                        "write `cancel context` or `cancel signal`",
                        word.span,
                    ));
                    continue;
                };
                if seen_cancel {
                    diagnostics.push(err(
                        "F001",
                        "duplicate foreign metadata `cancel`",
                        word.span,
                    ));
                } else {
                    seen_cancel = true;
                    if foreign.mode != ForeignMode::Async {
                        diagnostics.push(err(
                            "F001",
                            format!("`cancel {kind}` is for a call whose row has `Async`"),
                            word.span,
                        ));
                    }
                }
            }
            "variadic" => {
                if !word.args.is_empty() {
                    diagnostics.push(err("F001", "`variadic` takes no arguments", word.span));
                } else if seen_variadic {
                    diagnostics.push(err(
                        "F001",
                        "duplicate foreign metadata `variadic`",
                        word.span,
                    ));
                } else {
                    seen_variadic = true;
                    if foreign.arity == 0
                        || !is_list_param(foreign, foreign.arity - 1, list_type, list_params)
                    {
                        diagnostics.push(err(
                            "F001",
                            "`variadic` spreads a last argument of type `List a`",
                            word.span,
                        ));
                    }
                }
            }
            other => diagnostics.push(err(
                "S011",
                format!("unknown foreign metadata `{other}`"),
                word.span,
            )),
        }
    }
}

/// Validate all foreign adapter words in `own_modules` of `program`.
pub fn validate_program(program: &Program, own_modules: &BTreeSet<&str>) -> Vec<ForeignDiagnostic> {
    let list_type = list_type_name(&program.lang);
    let mut diagnostics = Vec::new();
    for module in &program.modules {
        if !own_modules.contains(module.name.as_str()) {
            continue;
        }
        for item in &module.items {
            if let ItemKind::Foreign(foreign) = &item.kind {
                validate_foreign(foreign, list_type, None, &mut diagnostics);
            }
        }
    }
    diagnostics
}
