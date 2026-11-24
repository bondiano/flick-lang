//! Minimal Flick-IR fallback evaluator/emitter for the `test/literal` fixture
//! backend (`--case fallback`), verifying that functions with non-matching
//! `@native` overrides arrive as `ItemKind::Value` and run their Flick body.

use std::fmt::Write as _;

use flick_backend::{BuildRequest, Diagnostic, ModuleRole, Span};
use flick_ir::{Arith, BinOp, Expr, ExprKind, ItemKind, Lit, Module, Pat, Program, Stmt};

pub(super) fn check_target(
    program: &Program,
    request: &BuildRequest,
) -> Result<(), Vec<Diagnostic>> {
    for module in &program.modules {
        if !request
            .modules
            .iter()
            .any(|m| m.name == module.name && m.role == ModuleRole::Own)
        {
            continue;
        }
        for item in &module.items {
            if let ItemKind::Foreign(f) = &item.kind {
                return Err(super::diagnostic(
                    &format!(
                        "fallback case expected Flick body, found foreign `{}`",
                        f.name
                    ),
                    Some(Span {
                        file: item.span.file.index(),
                        start: item.span.start,
                        end: item.span.end,
                    }),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn source(
    request: &BuildRequest,
    module_name: &str,
) -> Result<Vec<u8>, Vec<Diagnostic>> {
    let program = Program::from_cbor(&request.program)
        .map_err(|e| super::diagnostic(&e.to_string(), None))?;
    let Some(module) = program.modules.iter().find(|m| m.name == module_name) else {
        return Ok(b"export const main = (_) => undefined;\n".to_vec());
    };
    Ok(emit_module(module).into_bytes())
}

fn emit_module(module: &Module) -> String {
    let mut out = String::new();
    for item in &module.items {
        if let ItemKind::Value { name, body, .. } = &item.kind {
            let expr = emit_expr(body, &module.name);
            let _ = writeln!(out, "export const {name} = {expr};");
        }
    }
    out
}

fn pat_name(pat: &Pat) -> String {
    match pat {
        Pat::Bind(name) => name.clone(),
        _ => "_".to_owned(),
    }
}

fn emit_lit(lit: &Lit) -> String {
    match lit {
        Lit::Int(n) => n.to_string(),
        Lit::Int8(n) => n.to_string(),
        Lit::Int16(n) => n.to_string(),
        Lit::Int64(n) => n.to_string(),
        Lit::UInt8(n) => n.to_string(),
        Lit::UInt16(n) => n.to_string(),
        Lit::UInt32(n) => n.to_string(),
        Lit::UInt64(n) => n.to_string(),
        Lit::Float(f) => format!("{f:?}"),
        Lit::Float32(f) => format!("{f:?}"),
        Lit::BigInt(s) => format!("{s}n"),
        Lit::Bool(b) => b.to_string(),
        Lit::String(s) => serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into()),
        Lit::Unit => "undefined".to_owned(),
    }
}

fn binop_sym(op: BinOp) -> &'static str {
    match op {
        BinOp::Arith(Arith::Sub, _) => "-",
        BinOp::Arith(Arith::Mul, _) => "*",
        BinOp::Arith(Arith::Div, _) => "/",
        BinOp::Arith(Arith::Rem, _) => "%",
        BinOp::Arith(Arith::Pow, _) => "**",
        BinOp::Eq | BinOp::Same => "===",
        BinOp::NotEq | BinOp::NotSame => "!==",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
        // Add, Concat, and any other fallback operator
        _ => "+",
    }
}

fn emit_block(stmts: &[Stmt], tail: &Expr, current_module: &str) -> String {
    let mut s = String::from("(() => { ");
    for stmt in stmts {
        match stmt {
            Stmt::Let { pat, value, .. } => {
                let _ = write!(
                    s,
                    "const {} = {}; ",
                    pat_name(pat),
                    emit_expr(value, current_module)
                );
            }
            Stmt::Var { name, value, .. } => {
                let _ = write!(s, "let {name} = {}; ", emit_expr(value, current_module));
            }
            Stmt::Assign(name, value) => {
                let _ = write!(s, "{name} = {}; ", emit_expr(value, current_module));
            }
            Stmt::Fun { name, value, .. } => {
                let _ = write!(s, "const {name} = {}; ", emit_expr(value, current_module));
            }
            Stmt::Expr(e) => {
                let _ = write!(s, "{}; ", emit_expr(e, current_module));
            }
        }
    }
    let _ = write!(s, "return {}; }})()", emit_expr(tail, current_module));
    s
}

fn emit_expr(expr: &Expr, current_module: &str) -> String {
    match &expr.kind {
        ExprKind::Lit(lit) => emit_lit(lit),
        ExprKind::Local(name) | ExprKind::Var(name) => name.clone(),
        ExprKind::Global { module, name, .. } => {
            if module == current_module {
                name.clone()
            } else {
                match (module.as_str(), name.as_str()) {
                    (_, "sqrt") => "Math.sqrt".to_owned(),
                    (_, "abs") => "Math.abs".to_owned(),
                    (_, "println" | "write_line" | "stdout_line") => {
                        "((s) => { console.log(s); })".to_owned()
                    }
                    (_, "to_string" | "show") => "((v) => String(v))".to_owned(),
                    _ => format!("/* {module}.{name} */ ((x) => x)"),
                }
            }
        }
        ExprKind::Perform { op, .. } => match op.as_str() {
            "write_line" | "println" => "((s) => { console.log(s); })".to_owned(),
            _ => "((x) => x)".to_owned(),
        },
        ExprKind::Method { name, .. } => match name.as_str() {
            "show" => "((v) => String(v))".to_owned(),
            _ => "((x) => x)".to_owned(),
        },
        ExprKind::Dicts(inner, _)
        | ExprKind::Lift(inner, _)
        | ExprKind::Ignore(inner)
        | ExprKind::Task(inner)
        | ExprKind::Await(inner) => emit_expr(inner, current_module),
        ExprKind::Lambda { params, body } => {
            let mut s = emit_expr(body, current_module);
            for p in params.iter().rev() {
                s = format!("(({}) => {s})", pat_name(&p.pat));
            }
            s
        }
        ExprKind::App { func, arg, .. } => format!(
            "({})({})",
            emit_expr(func, current_module),
            emit_expr(arg, current_module)
        ),
        ExprKind::Binary(op, left, right) => format!(
            "({} {} {})",
            emit_expr(left, current_module),
            binop_sym(*op),
            emit_expr(right, current_module),
        ),
        ExprKind::Neg(_, inner) => format!("(-({}))", emit_expr(inner, current_module)),
        ExprKind::Not(inner) => format!("(!({}))", emit_expr(inner, current_module)),
        ExprKind::If(c, t, e) => format!(
            "({} ? {} : {})",
            emit_expr(c, current_module),
            emit_expr(t, current_module),
            emit_expr(e, current_module)
        ),
        ExprKind::Block(stmts, tail) => emit_block(stmts, tail, current_module),
        _ => "undefined".to_owned(),
    }
}
