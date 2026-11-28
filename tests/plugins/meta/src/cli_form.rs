//! CLI / form derive (`Meta.CliForm`):
//! - Reads `///` doc comments on the type (`target.decl.docs`), variants
//!   (`case.docs`), and record fields (`field.docs`).
//! - Preserves user declaration field order via `RecordField::source_index`
//!   (instead of alphabetical/UTF-16 order).
//! - Reads variant `@alias "..."` attributes (`case.attributes`) and optional
//!   constraints (`case.constraints`).
//! - Emits executable `help_text : Option T -> String` and
//!   `parse_form : List String -> Result String T`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use flick_plugin::{
    Arena, Arm, Block, Builder, CaseTarget, Decl, DeriveTarget, Diagnostic, Expr, Field, NodeId,
    Pat, RecordField, TokenKind, Ty, Written, WrittenKind, string_value,
};

#[derive(Clone, Copy)]
enum FieldTy {
    String,
    Int,
    Bool,
    OptionString,
}

fn classify(ty: &Written) -> Option<(FieldTy, &'static str)> {
    match &ty.kind {
        WrittenKind::Con { name, args } if args.is_empty() => match name.as_str() {
            "String" => Some((FieldTy::String, "String")),
            "Int" => Some((FieldTy::Int, "Int")),
            "Bool" => Some((FieldTy::Bool, "Bool")),
            _ => None,
        },
        WrittenKind::Con { name, args } if name == "Option" && args.len() == 1 => {
            if matches!(
                &args[0].kind,
                WrittenKind::Con { name, args } if name == "String" && args.is_empty()
            ) {
                Some((FieldTy::OptionString, "Option String"))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn ordered_fields(fields: &[RecordField]) -> Vec<&RecordField> {
    let mut ordered: Vec<&RecordField> = fields.iter().collect();
    ordered.sort_by_key(|f| f.source_index);
    ordered
}

fn parse_aliases(args: Option<&Block>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(block) = args else {
        return out;
    };
    let tokens: Vec<_> = block
        .tokens
        .iter()
        .filter(|t| {
            let txt = block.text_of(t.span);
            txt != "{" && txt != "}" && txt != ";" && txt != ","
        })
        .collect();
    let mut i = 0;
    while i + 2 < tokens.len() {
        if block.text_of(tokens[i].span) == "alias" && tokens[i + 2].kind == TokenKind::String {
            out.insert(
                block.text_of(tokens[i + 1].span).to_owned(),
                string_value(block.text_of(tokens[i + 2].span)),
            );
            i += 3;
        } else {
            i += 1;
        }
    }
    out
}

fn parse_string_arg(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        Some(string_value(trimmed))
    } else {
        None
    }
}

fn field_flag_name(field: &RecordField) -> Result<String, Vec<Diagnostic>> {
    if let Some(attr) = field.attributes.iter().find(|a| a.name == "rename") {
        let Some(renamed) = parse_string_arg(&attr.args).filter(|s| !s.is_empty()) else {
            return Err(vec![
                Diagnostic::error(
                    "`@rename` expects a non-empty string literal flag name",
                    attr.span,
                )
                .with_code("M005"),
            ]);
        };
        return Ok(renamed);
    }
    Ok(field.name.clone())
}

fn variant_alias(
    case: &CaseTarget,
    aliases: &BTreeMap<String, String>,
) -> Result<Option<String>, Vec<Diagnostic>> {
    if let Some(attr) = case.attributes.iter().find(|a| a.name == "alias") {
        let Some(alias) = parse_string_arg(&attr.args).filter(|s| !s.is_empty()) else {
            return Err(vec![
                Diagnostic::error(
                    "`@alias` expects a non-empty string literal, as `@alias \"s\"`",
                    attr.span,
                )
                .with_code("M005"),
            ]);
        };
        return Ok(Some(alias));
    }
    Ok(aliases.get(&case.name).cloned().or_else(|| {
        case.attributes
            .iter()
            .find(|a| a.name == "label")
            .map(|a| a.args.trim().to_owned())
            .filter(|s| !s.is_empty())
    }))
}

fn parse_record_body(
    b: &mut Builder,
    fields: &[&RecordField],
    wrapper_con: Option<&str>,
    args_var: &str,
) -> Result<NodeId, Vec<Diagnostic>> {
    let mut classified = Vec::new();
    for field in fields {
        let Some((fty, _)) = classify(&field.ty) else {
            return Err(vec![
                Diagnostic::error(
                    format!("unsupported CLI/form field type for `{}`", field.name),
                    field.ty.span,
                )
                .with_code("M005"),
            ]);
        };
        classified.push((*field, fty));
    }

    let mut rec_fields = Vec::new();
    for (idx, (field, _)) in classified.iter().enumerate() {
        let var = b.expr(Expr::Name(format!("v{idx}")));
        rec_fields.push(Field {
            label: field.name.clone(),
            value: var,
        });
    }
    let rec = b.expr(Expr::Record(rec_fields));
    let val = if let Some(con) = wrapper_con {
        let c = b.expr(Expr::Name(con.to_owned()));
        b.expr(Expr::Apply {
            function: c,
            arguments: vec![rec],
        })
    } else {
        rec
    };
    let ok = b.expr(Expr::Name("Ok".into()));
    let mut cont = b.expr(Expr::Apply {
        function: ok,
        arguments: vec![val],
    });

    for (idx, (field, fty)) in classified.iter().enumerate().rev() {
        let helper = match fty {
            FieldTy::String => "Meta.flag_string",
            FieldTy::Int => "Meta.flag_int",
            FieldTy::Bool => "Meta.flag_bool",
            FieldTy::OptionString => "Meta.flag_opt_string",
        };
        let flag_name = field_flag_name(field)?;
        let f = b.expr(Expr::Name(helper.into()));
        let flag = b.expr(Expr::Text(format!("--{flag_name}")));
        let args_ref = b.expr(Expr::Name(args_var.to_owned()));
        let call = b.expr(Expr::Apply {
            function: f,
            arguments: vec![flag, args_ref],
        });
        let pat = b.pat(Pat::Var(format!("v{idx}")));
        let lam = b.expr(Expr::Lambda {
            params: vec![pat],
            body: cont,
        });
        let bind = b.expr(Expr::Name("Meta.bind_res".into()));
        cont = b.expr(Expr::Apply {
            function: bind,
            arguments: vec![call, lam],
        });
    }
    Ok(cont)
}

#[allow(clippy::too_many_lines)]
pub fn derive(
    target: &DeriveTarget,
    args: Option<&Block>,
    _module: &[DeriveTarget],
) -> Result<Arena, Vec<Diagnostic>> {
    let whole = target.decl.whole();
    let aliases = parse_aliases(args);
    let mut b = Builder::new(whole);
    let mut help = String::new();

    // Type header + doc comment
    let _ = write!(help, "{}", target.name);
    if !target.decl.docs.is_empty() {
        let _ = write!(help, ": {}", target.decl.docs);
    }

    // Case 1: record type alias or single-constructor nominal record form
    let flat_record = if let Some(alias) = &target.alias
        && let WrittenKind::Record {
            fields,
            open: false,
        } = &alias.kind
    {
        Some((fields.as_slice(), None))
    } else if let [case] = target.cases.as_slice()
        && case.name == target.name
        && let [payload] = case.fields.as_slice()
        && let WrittenKind::Record {
            fields,
            open: false,
        } = &payload.kind
    {
        Some((fields.as_slice(), Some(case.name.as_str())))
    } else {
        None
    };

    let parse_body = if let Some((fields, wrapper_con)) = flat_record {
        let ordered = ordered_fields(fields);
        for field in &ordered {
            let Some((_, ty_label)) = classify(&field.ty) else {
                return Err(vec![
                    Diagnostic::error(
                        format!("unsupported CLI/form field type for `{}`", field.name),
                        field.ty.span,
                    )
                    .with_code("M005"),
                ]);
            };
            let flag_name = field_flag_name(field)?;
            let _ = write!(help, "\n  --{flag_name} <{ty_label}>");
            if !field.docs.is_empty() {
                let _ = write!(help, " - {}", field.docs);
            }
        }
        parse_record_body(&mut b, &ordered, wrapper_con, "args")?
    } else if !target.cases.is_empty() {
        // Case 2: ADT with variants (subcommands)
        let mut branch_expr = {
            let err = b.expr(Expr::Name("Err".into()));
            let prefix = b.expr(Expr::Text("unknown command: ".into()));
            let cmd = b.expr(Expr::Name("cmd".into()));
            let msg = b.expr(Expr::Concat(vec![prefix, cmd]));
            b.expr(Expr::Apply {
                function: err,
                arguments: vec![msg],
            })
        };

        for case in &target.cases {
            let cmd_name = case.name.to_ascii_lowercase();
            let alias = variant_alias(case, &aliases)?;
            let _ = write!(help, "\n  {cmd_name}");
            if let Some(a) = &alias {
                let _ = write!(help, " (alias: {a})");
            }
            if !case.constraints.is_empty() {
                let classes: Vec<&str> =
                    case.constraints.iter().map(|c| c.class.as_str()).collect();
                let _ = write!(help, " [requires: {}]", classes.join(", "));
            }
            if !case.docs.is_empty() {
                let _ = write!(help, ": {}", case.docs);
            }
            if let [payload] = case.fields.as_slice()
                && let WrittenKind::Record {
                    fields,
                    open: false,
                } = &payload.kind
            {
                let ordered = ordered_fields(fields);
                for field in &ordered {
                    let Some((_, ty_label)) = classify(&field.ty) else {
                        return Err(vec![
                            Diagnostic::error(
                                format!("unsupported CLI/form field type for `{}`", field.name),
                                field.ty.span,
                            )
                            .with_code("M005"),
                        ]);
                    };
                    let flag_name = field_flag_name(field)?;
                    let _ = write!(help, "\n    --{flag_name} <{ty_label}>");
                    if !field.docs.is_empty() {
                        let _ = write!(help, " - {}", field.docs);
                    }
                }
            }
        }

        for case in target.cases.iter().rev() {
            let cmd_name = case.name.to_ascii_lowercase();
            let mut names = vec![b.expr(Expr::Text(cmd_name))];
            if let Some(a) = variant_alias(case, &aliases)? {
                names.push(b.expr(Expr::Text(a)));
            }
            let names_list = b.expr(Expr::Items(names));
            let matcher = b.expr(Expr::Name("Meta.matches_cmd".into()));
            let cmd_ref = b.expr(Expr::Name("cmd".into()));
            let cond = b.expr(Expr::Apply {
                function: matcher,
                arguments: vec![cmd_ref, names_list],
            });
            let then_branch = if case.fields.is_empty() {
                let ok = b.expr(Expr::Name("Ok".into()));
                let con = b.expr(Expr::Name(case.name.clone()));
                b.expr(Expr::Apply {
                    function: ok,
                    arguments: vec![con],
                })
            } else if let [payload] = case.fields.as_slice()
                && let WrittenKind::Record {
                    fields,
                    open: false,
                } = &payload.kind
            {
                let ordered = ordered_fields(fields);
                parse_record_body(&mut b, &ordered, Some(&case.name), "rest")?
            } else {
                return Err(vec![
                    Diagnostic::error(
                        format!(
                            "variant `{}` must be nullary or carry a closed record for `Meta.CliForm`",
                            case.name
                        ),
                        case.span,
                    )
                    .with_code("M005"),
                ]);
            };
            branch_expr = b.expr(Expr::If {
                condition: cond,
                then: then_branch,
                otherwise: branch_expr,
            });
        }

        let head_fn = b.expr(Expr::Name("Meta.cmd_head".into()));
        let args_ref = b.expr(Expr::Name("args".into()));
        let scrutinee = b.expr(Expr::Apply {
            function: head_fn,
            arguments: vec![args_ref],
        });
        let cmd_pat = b.pat(Pat::Var("cmd".into()));
        let rest_pat = b.pat(Pat::Var("rest".into()));
        let pair_pat = b.pat(Pat::Tuple(vec![cmd_pat, rest_pat]));
        let some_pat = b.pat(Pat::Con {
            name: "Some".into(),
            arguments: vec![pair_pat],
        });
        let none_pat = b.pat(Pat::Con {
            name: "None".into(),
            arguments: Vec::new(),
        });
        let err_con = b.expr(Expr::Name("Err".into()));
        let empty_msg = b.expr(Expr::Text("expected a command".into()));
        let none_body = b.expr(Expr::Apply {
            function: err_con,
            arguments: vec![empty_msg],
        });
        b.expr(Expr::Match {
            scrutinee,
            arms: vec![
                Arm {
                    pattern: some_pat,
                    guard: None,
                    body: branch_expr,
                },
                Arm {
                    pattern: none_pat,
                    guard: None,
                    body: none_body,
                },
            ],
        })
    } else {
        return Err(vec![
            Diagnostic::error(
                format!(
                    "`Meta.CliForm` requires a record alias or ADT, got `{}`",
                    target.name
                ),
                target.name_span,
            )
            .with_code("M005"),
        ]);
    };

    let wild = b.pat(Pat::Wildcard);
    let help_text_expr = b.expr(Expr::Text(help));
    let help_let = b.decl(Decl::Let {
        name: "help_text".into(),
        params: vec![wild],
        ty: None,
        body: help_text_expr,
        rec: false,
    });

    let args_pat = b.pat(Pat::Var("args".into()));
    let parse_let = b.decl(Decl::Let {
        name: "parse_form".into(),
        params: vec![args_pat],
        ty: None,
        body: parse_body,
        rec: false,
    });

    let target_ty = b.ty(Ty::Name(target.name.clone()));
    let class_ty = b.ty(Ty::Name("Meta.CliForm".into()));
    let head = b.ty(Ty::Apply {
        function: class_ty,
        arguments: vec![target_ty],
    });
    let instance = b.decl(Decl::Instance {
        context: Vec::new(),
        head,
        members: vec![help_let, parse_let],
    });
    Ok(b.finish(instance))
}
