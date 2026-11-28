//! Instance codegen for `Meta.Codec` (`encode` and `decode` declarations).

use flick_plugin::{Arena, Builder, DeriveTarget, Expr, Field, NodeId, Pat, RecordField, Span, Ty};

use crate::codec::{DefaultVal, FieldConfig, SupportedTy};

#[allow(clippy::too_many_lines)]
pub(crate) fn emit_codec(
    target: &DeriveTarget,
    whole: Span,
    wrapper_con: Option<&str>,
    tag: Option<&str>,
    cfg: &FieldConfig,
    checked: &[(&RecordField, String, bool, Option<SupportedTy>)],
) -> Arena {
    let mut b = Builder::new(whole);

    // Build `encode`:
    //   let encode value = ...
    let mut encode_parts: Vec<NodeId> = Vec::new();
    for (field, wire, skipped, supported) in checked {
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
    let encode_body = if let Some(tag_name) = tag {
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

    let decode_body = if let Some(tag_name) = tag {
        let strip = b.expr(Expr::Name("Meta.strip_tag".into()));
        let tag_lit = b.expr(Expr::Text(tag_name.to_owned()));
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
    b.finish(instance)
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
