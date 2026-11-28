//! Configurable-field `Meta.Codec` derive:
//! - Reads field config (`rename`, `skip`, `default`) from `deriving Meta.Codec { ... }`
//!   and optional `@rename "<tag>"` type attribute.
//! - Rejects unsupported field types with `M001` at the exact field type span.
//! - Refuses opaque/private targets or public targets containing opaque/private
//!   field types with `M002` at the exact offending span.
//! - Emits executable `encode : T -> String` and `decode : String -> Result String T`
//!   in declaration order (`source_index`).

use std::collections::BTreeMap;

use flick_plugin::{
    Arena, Block, DeriveTarget, Diagnostic, RecordField, TokenKind, Visibility, Written,
    WrittenKind, string_value,
};

use crate::codec_emit::emit_codec;

#[derive(Clone)]
pub(crate) enum DefaultVal {
    String(String),
    Int(i32),
    Bool(bool),
}

#[derive(Default)]
pub(crate) struct FieldConfig {
    pub(crate) rename: BTreeMap<String, String>,
    pub(crate) skip: Vec<String>,
    pub(crate) defaults: BTreeMap<String, DefaultVal>,
}

#[derive(Clone, Copy)]
pub(crate) enum SupportedTy {
    String,
    Int,
    Bool,
    OptionString,
}

fn classify_type(ty: &Written) -> Option<SupportedTy> {
    match &ty.kind {
        WrittenKind::Con { name, args } if args.is_empty() => match name.as_str() {
            "String" => Some(SupportedTy::String),
            "Int" => Some(SupportedTy::Int),
            "Bool" => Some(SupportedTy::Bool),
            _ => None,
        },
        WrittenKind::Con { name, args } if name == "Option" && args.len() == 1 => {
            if matches!(
                &args[0].kind,
                WrittenKind::Con { name, args } if name == "String" && args.is_empty()
            ) {
                Some(SupportedTy::OptionString)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_config(
    args: Option<&Block>,
    target: &DeriveTarget,
) -> Result<FieldConfig, Vec<Diagnostic>> {
    let Some(block) = args else {
        return Ok(FieldConfig::default());
    };
    let mut cfg = FieldConfig::default();
    let tokens: Vec<_> = block
        .tokens
        .iter()
        .filter(|t| {
            let txt = block.text_of(t.span);
            txt != "{" && txt != "}" && txt != ";" && txt != ","
        })
        .collect();
    let mut i = 0;
    while i < tokens.len() {
        let word = block.text_of(tokens[i].span);
        match word {
            "rename" => {
                let (Some(field_tok), Some(wire_tok)) = (tokens.get(i + 1), tokens.get(i + 2))
                else {
                    return Err(vec![
                        Diagnostic::error(
                            "`rename` expects `<field> \"<wire_name>\"`",
                            target.name_span,
                        )
                        .with_code("M004"),
                    ]);
                };
                if wire_tok.kind != TokenKind::String {
                    return Err(vec![
                        Diagnostic::error(
                            "`rename` wire name must be a string literal",
                            target.name_span,
                        )
                        .with_code("M004"),
                    ]);
                }
                cfg.rename.insert(
                    block.text_of(field_tok.span).to_owned(),
                    string_value(block.text_of(wire_tok.span)),
                );
                i += 3;
            }
            "skip" => {
                let Some(field_tok) = tokens.get(i + 1) else {
                    return Err(vec![
                        Diagnostic::error("`skip` expects a field name", target.name_span)
                            .with_code("M004"),
                    ]);
                };
                cfg.skip.push(block.text_of(field_tok.span).to_owned());
                i += 2;
            }
            "default" => {
                let (Some(field_tok), Some(val_tok)) = (tokens.get(i + 1), tokens.get(i + 2))
                else {
                    return Err(vec![
                        Diagnostic::error(
                            "`default` expects `<field> <literal>`",
                            target.name_span,
                        )
                        .with_code("M004"),
                    ]);
                };
                let val_text = block.text_of(val_tok.span);
                let parsed = if val_tok.kind == TokenKind::String {
                    DefaultVal::String(string_value(val_text))
                } else if val_text == "true" {
                    DefaultVal::Bool(true)
                } else if val_text == "false" {
                    DefaultVal::Bool(false)
                } else if let Ok(n) = val_text.parse::<i32>() {
                    DefaultVal::Int(n)
                } else {
                    return Err(vec![
                        Diagnostic::error(
                            format!("unsupported default literal `{val_text}`"),
                            target.name_span,
                        )
                        .with_code("M004"),
                    ]);
                };
                cfg.defaults
                    .insert(block.text_of(field_tok.span).to_owned(), parsed);
                i += 3;
            }
            other => {
                return Err(vec![
                    Diagnostic::error(
                        format!("unknown `Meta.Codec` option `{other}`"),
                        target.name_span,
                    )
                    .with_code("M004"),
                ]);
            }
        }
    }
    Ok(cfg)
}

/// Checks whether `ty` mentions any type declared in `module` whose visibility
/// does not expose its constructors (opaque or private).
fn hidden_type_in<'a>(ty: &Written, module: &'a [DeriveTarget]) -> Option<&'a DeriveTarget> {
    match &ty.kind {
        WrittenKind::Con { name, args } => {
            if let Some(found) = module
                .iter()
                .find(|t| t.name == *name && !t.visibility.exposes_constructors())
            {
                return Some(found);
            }
            args.iter().find_map(|a| hidden_type_in(a, module))
        }
        WrittenKind::Record { fields, .. } => {
            fields.iter().find_map(|f| hidden_type_in(&f.ty, module))
        }
        WrittenKind::Tuple(items) => items.iter().find_map(|a| hidden_type_in(a, module)),
        WrittenKind::Captured(inner) => hidden_type_in(inner, module),
        WrittenKind::Forall { body, .. } => hidden_type_in(body, module),
        _ => None,
    }
}

fn record_fields(target: &DeriveTarget) -> Option<(Option<&str>, &[RecordField])> {
    if let Some(alias) = &target.alias
        && let WrittenKind::Record {
            fields,
            open: false,
        } = &alias.kind
    {
        return Some((None, fields.as_slice()));
    }
    if let [case] = target.cases.as_slice()
        && let [field] = case.fields.as_slice()
        && let WrittenKind::Record {
            fields,
            open: false,
        } = &field.kind
    {
        return Some((Some(case.name.as_str()), fields.as_slice()));
    }
    None
}

fn parse_default_literal(text: &str) -> Option<DefaultVal> {
    let trimmed = text.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        Some(DefaultVal::String(string_value(trimmed)))
    } else if trimmed == "true" {
        Some(DefaultVal::Bool(true))
    } else if trimmed == "false" {
        Some(DefaultVal::Bool(false))
    } else if let Ok(n) = trimmed.parse::<i32>() {
        Some(DefaultVal::Int(n))
    } else {
        None
    }
}

fn parse_string_arg(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        Some(string_value(trimmed))
    } else {
        None
    }
}

fn apply_field_attributes(
    fields: &[&RecordField],
    cfg: &mut FieldConfig,
) -> Result<(), Vec<Diagnostic>> {
    let mut errors = Vec::new();
    for field in fields {
        for attr in &field.attributes {
            match attr.name.as_str() {
                "rename" => {
                    if let Some(wire) = parse_string_arg(&attr.args) {
                        cfg.rename.insert(field.name.clone(), wire);
                    } else {
                        errors.push(
                            Diagnostic::error(
                                "`@rename` expects a string literal wire key, as `@rename \"user_id\"`",
                                attr.span,
                            )
                            .with_code("M004"),
                        );
                    }
                }
                "skip" => {
                    if attr.args.trim().is_empty() {
                        if !cfg.skip.iter().any(|s| s == &field.name) {
                            cfg.skip.push(field.name.clone());
                        }
                    } else {
                        errors.push(
                            Diagnostic::error("`@skip` takes no arguments", attr.span)
                                .with_code("M004"),
                        );
                    }
                }
                "default" => {
                    if let Some(val) = parse_default_literal(&attr.args) {
                        cfg.defaults.insert(field.name.clone(), val);
                    } else {
                        errors.push(
                            Diagnostic::error(
                                format!(
                                    "`@default` expects a string, int, or bool literal, got `{}`",
                                    attr.args.trim()
                                ),
                                attr.span,
                            )
                            .with_code("M004"),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn type_tag(target: &DeriveTarget) -> Result<Option<String>, Vec<Diagnostic>> {
    for attr in &target.attributes {
        if matches!(attr.name.as_str(), "tag" | "meta.prefix") {
            let Some(val) = parse_string_arg(&attr.args).filter(|s| !s.is_empty()) else {
                return Err(vec![
                    Diagnostic::error(
                        format!(
                            "`@{}` expects a non-empty string literal, as `@{} \"usr\"`",
                            attr.name, attr.name
                        ),
                        target.name_span,
                    )
                    .with_code("M004"),
                ]);
            };
            return Ok(Some(val));
        }
    }
    Ok(target
        .attributes
        .iter()
        .find(|a| a.name == "label")
        .map(|a| {
            let trimmed = a.args.trim();
            if trimmed.starts_with('"') && trimmed.ends_with('"') {
                string_value(trimmed)
            } else {
                trimmed.to_owned()
            }
        })
        .filter(|s| !s.is_empty()))
}

#[allow(clippy::too_many_lines)]
pub fn derive(
    target: &DeriveTarget,
    args: Option<&Block>,
    module: &[DeriveTarget],
) -> Result<Arena, Vec<Diagnostic>> {
    let whole = target.decl.whole();
    if target.visibility == Visibility::Opaque
        || (!target.local && !target.visibility.exposes_constructors())
    {
        return Err(vec![
            Diagnostic::error(
                format!(
                    "cannot derive `Meta.Codec` for opaque or non-public type `{}`",
                    target.name
                ),
                target.name_span,
            )
            .with_code("M002"),
        ]);
    }

    let mut cfg = parse_config(args, target)?;
    let Some((wrapper_con, raw_fields)) = record_fields(target) else {
        return Err(vec![
            Diagnostic::error(
                format!(
                    "`Meta.Codec` expects a closed record type or single-constructor record, got `{}`",
                    target.name
                ),
                target.name_span,
            )
            .with_code("M001"),
        ]);
    };

    // Preserve declaration order (`source_index`).
    let mut fields: Vec<&RecordField> = raw_fields.iter().collect();
    fields.sort_by_key(|f| f.source_index);
    apply_field_attributes(&fields, &mut cfg)?;
    let tag = type_tag(target)?;

    let mut checked: Vec<(&RecordField, String, bool, Option<SupportedTy>)> = Vec::new();
    let mut errors = Vec::new();
    for field in &fields {
        let skipped = cfg.skip.iter().any(|s| s == &field.name);
        if target.visibility.is_public()
            && !skipped
            && let Some(hidden) = hidden_type_in(&field.ty, module)
        {
            errors.push(
                Diagnostic::error(
                    format!(
                        "cannot expose non-public or opaque type `{}` in field `{}` of `Meta.Codec`",
                        hidden.name, field.name
                    ),
                    field.span,
                )
                .with_code("M002"),
            );
            continue;
        }
        let wire = cfg
            .rename
            .get(&field.name)
            .cloned()
            .unwrap_or_else(|| field.name.clone());
        let supported = classify_type(&field.ty);
        if supported.is_none() {
            errors.push(
                Diagnostic::error(
                    format!(
                        "unsupported field type for `{}` in `Meta.Codec`",
                        field.name
                    ),
                    field.ty.span,
                )
                .with_code("M001"),
            );
            continue;
        }
        checked.push((field, wire, skipped, supported));
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(emit_codec(
        target,
        whole,
        wrapper_con,
        tag.as_deref(),
        &cfg,
        &checked,
    ))
}
