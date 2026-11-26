//! Expression, pattern, literal, and constructor JS emission for `layout-specialize`,
//! operating on `flick_backend_support::specialize::SpecExpr`.

use std::fmt::Write as _;

use flick_backend_support::specialize::{SpecExpr, SpecExprKind, SpecPat, SpecStmt};
use flick_ir::{Arith, BinOp, Constructor, EnumLit, Lit, Num, Rep};

use super::emit::{dict_js_name, emit_dict, emit_value_body, local_js_name, sym_js_name};

fn emit_lit(lit: &Lit) -> String {
    match lit {
        Lit::Int(n) => n.to_string(),
        Lit::Int8(n) => n.to_string(),
        Lit::Int16(n) => n.to_string(),
        Lit::Int64(n) => format!("{n}n"),
        Lit::UInt8(n) => n.to_string(),
        Lit::UInt16(n) => n.to_string(),
        Lit::UInt32(n) => n.to_string(),
        Lit::UInt64(n) => format!("{n}n"),
        Lit::Float(f) => {
            if f.is_nan() {
                "NaN".to_owned()
            } else if f.is_infinite() {
                if f.is_sign_positive() {
                    "Infinity".to_owned()
                } else {
                    "(-Infinity)".to_owned()
                }
            } else {
                format!("{f:?}")
            }
        }
        Lit::Float32(f) => format!("Math.fround({f:?})"),
        Lit::BigInt(s) => format!("{s}n"),
        Lit::Bool(b) => b.to_string(),
        Lit::String(s) => serde_json::to_string(s).expect("string json"),
        Lit::Unit => "undefined".to_owned(),
    }
}

fn emit_constructor(con: &Constructor) -> String {
    match &con.rep {
        Rep::Literal(EnumLit::Int(n)) => return n.to_string(),
        Rep::Literal(EnumLit::String(s)) => return serde_json::to_string(s).expect("str json"),
        Rep::Unboxed | Rep::Typeof(_) if con.arity == 1 && con.dicts == 0 => {
            return "((x) => x)".to_owned();
        }
        _ => {}
    }
    let tag = serde_json::to_string(&format!("{}.{}.{}", con.module, con.ty, con.name))
        .expect("tag json");
    let mut fields = Vec::with_capacity(con.arity + con.dicts);
    for i in 0..con.arity {
        fields.push(format!("$f{i}"));
    }
    for i in 0..con.dicts {
        fields.push(format!("$d{i}"));
    }
    let mut body = format!(
        "({{ _tag: {tag}, _f: [{}], _o: {} }})",
        fields.join(", "),
        con.arity
    );
    for i in (0..con.arity).rev() {
        body = format!("(($f{i}) => {body})");
    }
    for i in (0..con.dicts).rev() {
        body = format!("(($d{i}) => {body})");
    }
    body
}

fn emit_pat_check_and_bind(
    pat: &SpecPat,
    target: &str,
    conds: &mut Vec<String>,
    binds: &mut Vec<String>,
) {
    match pat {
        SpecPat::Wild => {}
        SpecPat::Bind(name) => {
            binds.push(format!("const {} = {target};", local_js_name(name)));
        }
        SpecPat::Lit(lit) => {
            conds.push(format!("$eq({target}, {})", emit_lit(lit)));
        }
        SpecPat::Tuple(items) => {
            if items.is_empty() {
                conds.push(format!("({target} === undefined)"));
            } else {
                conds.push(format!(
                    "(Array.isArray({target}) && {target}.length === {})",
                    items.len()
                ));
                for (idx, sub) in items.iter().enumerate() {
                    emit_pat_check_and_bind(sub, &format!("{target}[{idx}]"), conds, binds);
                }
            }
        }
        SpecPat::List(items, rest) => {
            if let Some(rest_pat) = rest {
                conds.push(format!(
                    "(Array.isArray({target}) && {target}.length >= {})",
                    items.len()
                ));
                for (idx, sub) in items.iter().enumerate() {
                    emit_pat_check_and_bind(sub, &format!("{target}[{idx}]"), conds, binds);
                }
                emit_pat_check_and_bind(
                    rest_pat,
                    &format!("{target}.slice({})", items.len()),
                    conds,
                    binds,
                );
            } else {
                conds.push(format!(
                    "(Array.isArray({target}) && {target}.length === {})",
                    items.len()
                ));
                for (idx, sub) in items.iter().enumerate() {
                    emit_pat_check_and_bind(sub, &format!("{target}[{idx}]"), conds, binds);
                }
            }
        }
        SpecPat::Record { fields, .. } => {
            conds.push(format!(
                "({target} !== null && typeof {target} === \"object\")"
            ));
            for (label, _, sub) in fields {
                let key = serde_json::to_string(label).expect("label json");
                emit_pat_check_and_bind(sub, &format!("{target}[{key}]"), conds, binds);
            }
        }
        SpecPat::Con { ctor, args, .. } => match &ctor.rep {
            Rep::Literal(EnumLit::Int(n)) => conds.push(format!("({target} === {n})")),
            Rep::Literal(EnumLit::String(s)) => {
                let s_json = serde_json::to_string(s).expect("str json");
                conds.push(format!("({target} === {s_json})"));
            }
            Rep::Unboxed | Rep::Typeof(_) if ctor.arity == 1 && ctor.dicts == 0 => {
                if let Some(sub) = args.first() {
                    emit_pat_check_and_bind(sub, target, conds, binds);
                }
            }
            _ => {
                let tag =
                    serde_json::to_string(&format!("{}.{}.{}", ctor.module, ctor.ty, ctor.name))
                        .expect("tag json");
                conds.push(format!(
                    "({target} !== null && typeof {target} === \"object\" && {target}._tag === {tag})"
                ));
                for (idx, sub) in args.iter().enumerate() {
                    emit_pat_check_and_bind(sub, &format!("{target}._f[{idx}]"), conds, binds);
                }
            }
        },
        SpecPat::Or(alts) => {
            let mut alt_conds = Vec::new();
            for alt in alts {
                let mut sub_c = Vec::new();
                let mut sub_b = Vec::new();
                emit_pat_check_and_bind(alt, target, &mut sub_c, &mut sub_b);
                if sub_c.is_empty() {
                    alt_conds.push("true".to_owned());
                } else {
                    alt_conds.push(format!("({})", sub_c.join(" && ")));
                }
            }
            if !alt_conds.is_empty() {
                conds.push(format!("({})", alt_conds.join(" || ")));
            }
        }
    }
}

fn emit_binary(op: BinOp, left: &SpecExpr, right: &SpecExpr) -> String {
    let l = emit_expr(left);
    let r = emit_expr(right);
    match op {
        BinOp::Arith(Arith::Add, Num::Int) => format!("((({l}) + ({r})) | 0)"),
        BinOp::Arith(Arith::Sub, Num::Int) => format!("((({l}) - ({r})) | 0)"),
        BinOp::Arith(Arith::Mul, Num::Int) => format!("Math.imul({l}, {r})"),
        BinOp::Arith(Arith::Div, Num::Int) => {
            format!("((($a, $b) => $b === 0 ? 0 : (($a / $b) | 0))({l}, {r}))")
        }
        BinOp::Arith(Arith::Rem, Num::Int) => {
            format!("((($a, $b) => $b === 0 ? 0 : (($a % $b) | 0))({l}, {r}))")
        }
        BinOp::Arith(Arith::Add, _) => format!("(({l}) + ({r}))"),
        BinOp::Arith(Arith::Sub, _) => format!("(({l}) - ({r}))"),
        BinOp::Arith(Arith::Mul, _) => format!("(({l}) * ({r}))"),
        BinOp::Arith(Arith::Div, _) => format!("(({l}) / ({r}))"),
        BinOp::Arith(Arith::Rem, _) => format!("(({l}) % ({r}))"),
        BinOp::Arith(Arith::Pow, _) => format!("(({l}) ** ({r}))"),
        BinOp::Concat => format!("(String({l}) + String({r}))"),
        BinOp::ListConcat => format!("[...({l}), ...({r})]"),
        BinOp::Eq | BinOp::Same => format!("$eq({l}, {r})"),
        BinOp::NotEq | BinOp::NotSame => format!("(!$eq({l}, {r}))"),
        BinOp::Lt => format!("(({l}) < ({r}))"),
        BinOp::Le => format!("(({l}) <= ({r}))"),
        BinOp::Gt => format!("(({l}) > ({r}))"),
        BinOp::Ge => format!("(({l}) >= ({r}))"),
        BinOp::And => format!("(({l}) && ({r}))"),
        BinOp::Or => format!("(({l}) || ({r}))"),
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn emit_expr(expr: &SpecExpr) -> String {
    match &expr.kind {
        SpecExprKind::Box { expr: inner, .. }
        | SpecExprKind::Unbox { expr: inner, .. }
        | SpecExprKind::Lift(inner, _)
        | SpecExprKind::Task(inner)
        | SpecExprKind::Await(inner) => emit_expr(inner),
        SpecExprKind::CallDirect { target, dicts }
        | SpecExprKind::CallFallback { target, dicts }
        | SpecExprKind::CallForeign { target, dicts, .. } => {
            let mut s = sym_js_name(target);
            for d in dicts {
                let _ = write!(s, "({})", emit_dict(d));
            }
            s
        }
        SpecExprKind::MethodCall { dict, name } => {
            let key = serde_json::to_string(name).expect("method json");
            format!("({})[{key}]", emit_dict(dict))
        }
        SpecExprKind::Construct { ctor, dicts, .. } => {
            let mut s = emit_constructor(ctor);
            for d in dicts {
                let _ = write!(s, "({})", emit_dict(d));
            }
            s
        }
        SpecExprKind::Lit(lit) => emit_lit(lit),
        SpecExprKind::Local(name) | SpecExprKind::Var(name) => local_js_name(name),
        SpecExprKind::Dicts(inner, dicts) => {
            let mut s = format!("({})", emit_expr(inner));
            for d in dicts {
                let _ = write!(s, "({})", emit_dict(d));
            }
            s
        }
        SpecExprKind::Abstract(names, inner) => {
            let mut s = emit_expr(inner);
            for n in names.iter().rev() {
                s = format!("(({}) => {s})", dict_js_name(n));
            }
            s
        }
        SpecExprKind::App {
            func,
            arg,
            arg_first,
            ..
        } => {
            if *arg_first {
                format!(
                    "((() => {{ const $a = {}; return ({})($a); }})())",
                    emit_expr(arg),
                    emit_expr(func)
                )
            } else {
                format!("(({})({}))", emit_expr(func), emit_expr(arg))
            }
        }
        SpecExprKind::Lambda { params, body } => {
            let mut body_js = emit_expr(body);
            for (idx, param) in params.iter().enumerate().rev() {
                match &param.pat {
                    SpecPat::Wild => {
                        body_js = format!("((_$p{idx}) => {body_js})");
                    }
                    SpecPat::Bind(name) => {
                        body_js = format!("(({}) => {body_js})", local_js_name(name));
                    }
                    other => {
                        let arg_var = format!("$p{idx}");
                        let mut conds = Vec::new();
                        let mut binds = Vec::new();
                        emit_pat_check_and_bind(other, &arg_var, &mut conds, &mut binds);
                        body_js = format!(
                            "(({arg_var}) => {{ {} return {body_js}; }})",
                            binds.join(" ")
                        );
                    }
                }
            }
            body_js
        }
        SpecExprKind::Tuple(items) => {
            if items.is_empty() {
                "undefined".to_owned()
            } else {
                let parts: Vec<String> = items.iter().map(emit_expr).collect();
                format!("[{}]", parts.join(", "))
            }
        }
        SpecExprKind::List(items) => {
            let parts: Vec<String> = items.iter().map(emit_expr).collect();
            format!("[{}]", parts.join(", "))
        }
        SpecExprKind::Record { fields, .. } | SpecExprKind::Dictionary(fields) => {
            let mut s = String::from("(() => { ");
            let mut entries = Vec::with_capacity(fields.len());
            for (i, (label, val)) in fields.iter().enumerate() {
                let _ = write!(s, "const $rf{i} = {}; ", emit_expr(val));
                let key = serde_json::to_string(label).expect("label json");
                entries.push(format!("{key}: $rf{i}"));
            }
            let _ = write!(s, "return {{ {} }}; }})()", entries.join(", "));
            s
        }
        SpecExprKind::FieldOffset {
            record,
            label,
            index,
            offset,
            ..
        } => {
            let key = serde_json::to_string(label).expect("field json");
            format!(
                "(/* off={offset}, idx={index} */ ({})[{key}])",
                emit_expr(record)
            )
        }
        SpecExprKind::FieldByLabel { record, label } => {
            let key = serde_json::to_string(label).expect("field json");
            format!("({})[{key}]", emit_expr(record))
        }
        SpecExprKind::UpdateOffset { record, fields } => {
            let mut s = format!("(() => {{ const $base = {}; ", emit_expr(record));
            let mut entries = Vec::with_capacity(fields.len());
            for (i, (label, _, _, val)) in fields.iter().enumerate() {
                let _ = write!(s, "const $uf{i} = {}; ", emit_expr(val));
                let key = serde_json::to_string(label).expect("label json");
                entries.push(format!("{key}: $uf{i}"));
            }
            let _ = write!(s, "return {{ ...$base, {} }}; }})()", entries.join(", "));
            s
        }
        SpecExprKind::UpdateByLabel { record, fields } => {
            let mut s = format!("(() => {{ const $base = {}; ", emit_expr(record));
            let mut entries = Vec::with_capacity(fields.len());
            for (i, (label, val)) in fields.iter().enumerate() {
                let _ = write!(s, "const $uf{i} = {}; ", emit_expr(val));
                let key = serde_json::to_string(label).expect("label json");
                entries.push(format!("{key}: $uf{i}"));
            }
            let _ = write!(s, "return {{ ...$base, {} }}; }})()", entries.join(", "));
            s
        }
        SpecExprKind::Binary(op, l, r) => emit_binary(*op, l, r),
        SpecExprKind::Neg(Num::Int, inner) => format!("((-({})) | 0)", emit_expr(inner)),
        SpecExprKind::Neg(_, inner) => format!("(-({}))", emit_expr(inner)),
        SpecExprKind::Not(inner) => format!("(!({}))", emit_expr(inner)),
        SpecExprKind::If(c, t, e) => {
            format!(
                "(({}) ? ({}) : ({}))",
                emit_expr(c),
                emit_expr(t),
                emit_expr(e)
            )
        }
        SpecExprKind::Block(stmts, tail) => {
            let mut s = String::from("(() => { ");
            for stmt in stmts {
                match stmt {
                    SpecStmt::Let { pat, value, .. } => {
                        let v = emit_expr(value);
                        match pat {
                            SpecPat::Wild => {
                                let _ = write!(s, "{v}; ");
                            }
                            SpecPat::Bind(name) => {
                                let _ = write!(s, "const {} = {v}; ", local_js_name(name));
                            }
                            other => {
                                let mut conds = Vec::new();
                                let mut binds = Vec::new();
                                emit_pat_check_and_bind(other, "$lv", &mut conds, &mut binds);
                                let _ = write!(s, "const $lv = {v}; {} ", binds.join(" "));
                            }
                        }
                    }
                    SpecStmt::Var { name, value, .. } => {
                        let _ = write!(s, "let {} = {}; ", local_js_name(name), emit_expr(value));
                    }
                    SpecStmt::Assign(name, value) => {
                        let _ = write!(s, "{} = {}; ", local_js_name(name), emit_expr(value));
                    }
                    SpecStmt::Fun {
                        name, dicts, value, ..
                    } => {
                        let var = local_js_name(name);
                        let rhs = emit_value_body(dicts, value);
                        let _ = write!(s, "let {var}; {var} = {rhs}; ");
                    }
                    SpecStmt::Expr(e) => {
                        let _ = write!(s, "{}; ", emit_expr(e));
                    }
                }
            }
            let _ = write!(s, "return {}; }})()", emit_expr(tail));
            s
        }
        SpecExprKind::Match(scrut, arms) => {
            let mut s = format!("(() => {{ const $s = {}; ", emit_expr(scrut));
            for arm in arms {
                let mut conds = Vec::new();
                let mut binds = Vec::new();
                emit_pat_check_and_bind(&arm.pat, "$s", &mut conds, &mut binds);
                let check = if conds.is_empty() {
                    "true".to_owned()
                } else {
                    conds.join(" && ")
                };
                if let Some(guard) = &arm.guard {
                    let _ = write!(
                        s,
                        "if ({check}) {{ {} if ({}) return {}; }} ",
                        binds.join(" "),
                        emit_expr(guard),
                        emit_expr(&arm.body),
                    );
                } else {
                    let _ = write!(
                        s,
                        "if ({check}) {{ {} return {}; }} ",
                        binds.join(" "),
                        emit_expr(&arm.body),
                    );
                }
            }
            s.push_str("throw new Error(\"non-exhaustive match\"); })()");
            s
        }
        SpecExprKind::Ignore(inner) => {
            format!("((() => {{ {}; return undefined; }})())", emit_expr(inner))
        }
        SpecExprKind::Perform { op, .. } => match op.as_str() {
            "stdout_line" | "write_line" | "println" => "((s) => { console.log(s); })".to_owned(),
            _ => "((x) => x)".to_owned(),
        },
        _ => "undefined".to_owned(),
    }
}
