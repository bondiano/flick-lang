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
    Arena, Block, Builder, DeriveTarget, Diagnostic, Expr, Field, NodeId, Pat, RecordField,
    TokenKind, Ty, Visibility, Written, WrittenKind, string_value,
};

#[derive(Clone)]
enum DefaultVal {
    String(String),
    Int(i32),
    Bool(bool),
}

#[derive(Default)]
struct FieldConfig {
    rename: BTreeMap<String, String>,
    skip: Vec<String>,
    defaults: BTreeMap<String, DefaultVal>,
}

#[derive(Clone, Copy)]
enum SupportedTy {
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

fn type_tag(target: &DeriveTarget) -> Option<String> {
    target
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
        .filter(|s| !s.is_empty())
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

    let cfg = parse_config(args, target)?;
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

    let mut b = Builder::new(whole);
    let tag = type_tag(target);

    // Build `encode`:
    //   let encode value = ...
    let mut encode_parts: Vec<NodeId> = Vec::new();
    for (field, wire, skipped, supported) in &checked {
        if *skipped {
            continue;
        }
        let rec = b.expr(Expr::Name("row".into()));
        let get = b.expr(Expr::Get {
            record: rec,
            label: field.name.clone(),
        });
        let encoded_val = match supported.expect("checked above") {
            SupportedTy::String => get,
            SupportedTy::Int => {
                let f = b.expr(Expr::Name("Meta.encode_int".into()));
                b.expr(Expr::Apply {
                    function: f,
                    arguments: vec![get],
                })
            }
            SupportedTy::Bool => {
                let f = b.expr(Expr::Name("Meta.encode_bool".into()));
                b.expr(Expr::Apply {
                    function: f,
                    arguments: vec![get],
                })
            }
            SupportedTy::OptionString => {
                let f = b.expr(Expr::Name("Meta.encode_opt_string".into()));
                b.expr(Expr::Apply {
                    function: f,
                    arguments: vec![get],
                })
            }
        };
        let prefix = b.expr(Expr::Text(format!("{wire}=")));
        let pair = b.expr(Expr::Concat(vec![prefix, encoded_val]));
        encode_parts.push(pair);
    }
    let list = b.expr(Expr::Items(encode_parts));
    let join = b.expr(Expr::Name("Meta.join_kv".into()));
    let joined = b.expr(Expr::Apply {
        function: join,
        arguments: vec![list],
    });
    let encode_body = if let Some(tag_name) = &tag {
        let prefix = b.expr(Expr::Text(format!("{tag_name}:")));
        b.expr(Expr::Concat(vec![prefix, joined]))
    } else {
        joined
    };
    let encode_param = if let Some(con) = wrapper_con {
        let inner = b.pat(Pat::Var("row".into()));
        b.pat(Pat::Con {
            name: con.to_owned(),
            arguments: vec![inner],
        })
    } else {
        b.pat(Pat::Var("row".into()))
    };
    let encode_let = b.decl(flick_plugin::Decl::Let {
        name: "encode".into(),
        params: vec![encode_param],
        ty: None,
        body: encode_body,
        rec: false,
    });

    // Build `decode`:
    //   let decode raw = ...
    let mut record_fields_out = Vec::new();
    for (idx, (field, _, _, _)) in checked.iter().enumerate() {
        let var = b.expr(Expr::Name(format!("f{idx}")));
        record_fields_out.push(Field {
            label: field.name.clone(),
            value: var,
        });
    }
    let rec_expr = b.expr(Expr::Record(record_fields_out));
    let value_expr = if let Some(con) = wrapper_con {
        let con_expr = b.expr(Expr::Name(con.to_owned()));
        b.expr(Expr::Apply {
            function: con_expr,
            arguments: vec![rec_expr],
        })
    } else {
        rec_expr
    };
    let ok_con = b.expr(Expr::Name("Ok".into()));
    let mut cont = b.expr(Expr::Apply {
        function: ok_con,
        arguments: vec![value_expr],
    });

    for (idx, (field, wire, skipped, supported)) in checked.iter().enumerate().rev() {
        let ty = supported.expect("checked above");
        let step_expr = if *skipped {
            let def_expr = default_expr(&mut b, ty, cfg.defaults.get(&field.name));
            let ok = b.expr(Expr::Name("Ok".into()));
            b.expr(Expr::Apply {
                function: ok,
                arguments: vec![def_expr],
            })
        } else if let Some(def) = cfg.defaults.get(&field.name) {
            let def_expr = default_expr(&mut b, ty, Some(def));
            let helper = match ty {
                SupportedTy::String => "Meta.decode_string_default",
                SupportedTy::Int => "Meta.decode_int_default",
                SupportedTy::Bool => "Meta.decode_bool_default",
                SupportedTy::OptionString => "Meta.decode_opt_string",
            };
            let f = b.expr(Expr::Name(helper.into()));
            let key_arg = b.expr(Expr::Text(wire.clone()));
            let input_arg = b.expr(Expr::Name("body".into()));
            if matches!(ty, SupportedTy::OptionString) {
                b.expr(Expr::Apply {
                    function: f,
                    arguments: vec![key_arg, input_arg],
                })
            } else {
                b.expr(Expr::Apply {
                    function: f,
                    arguments: vec![key_arg, def_expr, input_arg],
                })
            }
        } else {
            let helper = match ty {
                SupportedTy::String => "Meta.decode_string",
                SupportedTy::Int => "Meta.decode_int",
                SupportedTy::Bool => "Meta.decode_bool",
                SupportedTy::OptionString => "Meta.decode_opt_string",
            };
            let f = b.expr(Expr::Name(helper.into()));
            let key_arg = b.expr(Expr::Text(wire.clone()));
            let input_arg = b.expr(Expr::Name("body".into()));
            b.expr(Expr::Apply {
                function: f,
                arguments: vec![key_arg, input_arg],
            })
        };
        let param = b.pat(Pat::Var(format!("f{idx}")));
        let lam = b.expr(Expr::Lambda {
            params: vec![param],
            body: cont,
        });
        let bind = b.expr(Expr::Name("Meta.bind_res".into()));
        cont = b.expr(Expr::Apply {
            function: bind,
            arguments: vec![step_expr, lam],
        });
    }

    let decode_body = if let Some(tag_name) = &tag {
        let strip = b.expr(Expr::Name("Meta.strip_tag".into()));
        let tag_lit = b.expr(Expr::Text(tag_name.clone()));
        let raw_ref = b.expr(Expr::Name("raw".into()));
        let stripped = b.expr(Expr::Apply {
            function: strip,
            arguments: vec![tag_lit, raw_ref],
        });
        let body_pat = b.pat(Pat::Var("body".into()));
        let lam = b.expr(Expr::Lambda {
            params: vec![body_pat],
            body: cont,
        });
        let bind = b.expr(Expr::Name("Meta.bind_res".into()));
        b.expr(Expr::Apply {
            function: bind,
            arguments: vec![stripped, lam],
        })
    } else {
        let raw_ref = b.expr(Expr::Name("raw".into()));
        let body_pat = b.pat(Pat::Var("body".into()));
        let lam = b.expr(Expr::Lambda {
            params: vec![body_pat],
            body: cont,
        });
        b.expr(Expr::Apply {
            function: lam,
            arguments: vec![raw_ref],
        })
    };

    let raw_pat = b.pat(Pat::Var("raw".into()));
    let decode_let = b.decl(flick_plugin::Decl::Let {
        name: "decode".into(),
        params: vec![raw_pat],
        ty: None,
        body: decode_body,
        rec: false,
    });

    let target_ty = b.ty(Ty::Name(target.name.clone()));
    let class_ty = b.ty(Ty::Name("Meta.Codec".into()));
    let head = b.ty(Ty::Apply {
        function: class_ty,
        arguments: vec![target_ty],
    });
    let instance = b.decl(flick_plugin::Decl::Instance {
        context: Vec::new(),
        head,
        members: vec![encode_let, decode_let],
    });
    Ok(b.finish(instance))
}

fn default_expr(b: &mut Builder, ty: SupportedTy, explicit: Option<&DefaultVal>) -> NodeId {
    match (ty, explicit) {
        (SupportedTy::String, Some(DefaultVal::String(s))) => b.expr(Expr::Text(s.clone())),
        (SupportedTy::String, _) => b.expr(Expr::Text(String::new())),
        (SupportedTy::Int, Some(DefaultVal::Int(n))) => b.expr(Expr::Number(f64::from(*n))),
        (SupportedTy::Int, _) => b.expr(Expr::Number(0.0)),
        (SupportedTy::Bool, Some(DefaultVal::Bool(v))) => b.expr(Expr::Boolean(*v)),
        (SupportedTy::Bool, _) => b.expr(Expr::Boolean(false)),
        (SupportedTy::OptionString, Some(DefaultVal::String(s))) => {
            let some = b.expr(Expr::Name("Some".into()));
            let val = b.expr(Expr::Text(s.clone()));
            b.expr(Expr::Apply {
                function: some,
                arguments: vec![val],
            })
        }
        (SupportedTy::OptionString, _) => b.expr(Expr::Name("None".into())),
    }
}
