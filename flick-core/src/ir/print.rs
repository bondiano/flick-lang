//! A compact text form of the IR for tests: one item per line group, every
//! runtime stage marked with `!`, lifts with `lift`, dictionaries in `[…]`.

use std::fmt::Write;

use super::{Arm, Expr, ExprKind, ForeignMode, Handler, ItemKind, Lit, Module, Pat, Stage, Stmt};
use crate::types::{Dict, Lift};

pub fn print(module: &Module) -> String {
    let mut out = format!("module {}\n", module.name);
    for item in &module.items {
        let line = match &item.kind {
            ItemKind::Value { name, dicts, body } => {
                format!("let {name}{} = {}", given(dicts), expr(body))
            }
            ItemKind::Destructure { pattern, body } => {
                format!("let {} = {}", pat(pattern), expr(body))
            }
            ItemKind::Foreign(f) => {
                let mode = match f.mode {
                    ForeignMode::Pure => "pure",
                    ForeignMode::Sync => "sync",
                    ForeignMode::Async => "async",
                    ForeignMode::Effect => "effect",
                };
                format!(
                    "foreign {mode} {} = {}:{} arity {} dicts {} stages {}",
                    f.name,
                    f.binding.from.as_deref().unwrap_or("global"),
                    f.binding.name,
                    f.arity,
                    f.dicts,
                    stages(&f.stages)
                )
            }
            ItemKind::Constructor(c) => {
                format!("constructor {}.{}.{}/{}", c.module, c.ty, c.name, c.arity)
            }
            ItemKind::Service { name, key } => format!("service {name} = {key}"),
            ItemKind::Refined { name, check } => format!("refined {name} = {}", expr(check)),
            ItemKind::Instance { info, methods } => {
                let mut s = format!("instance {}{}", info.name, given(&info.context));
                for (class, d) in &info.supers {
                    let _ = write!(s, " super {class} = {}", dict(d));
                }
                if let Some((fields, _)) = &info.derived {
                    let fields: Vec<String> = fields.iter().map(dict).collect();
                    let _ = write!(s, " derived [{}]", fields.join(", "));
                }
                for (name, _, extra) in info.defaults.iter().filter(|(_, _, e)| !e.is_empty()) {
                    let extra: Vec<String> = extra.iter().map(dict).collect();
                    let _ = write!(s, " default {name} [{}]", extra.join(", "));
                }
                for (name, value) in methods {
                    let _ = write!(s, "\n  {name} = {}", expr(value));
                }
                s
            }
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn given(dicts: &[String]) -> String {
    if dicts.is_empty() {
        String::new()
    } else {
        format!(" [{}]", dicts.join(", "))
    }
}

fn stages(stages: &[bool]) -> String {
    stages.iter().map(|&r| if r { '!' } else { '.' }).collect()
}

pub fn dict(d: &Dict) -> String {
    match d {
        Dict::Instance { name, args, .. } if args.is_empty() => format!("<{name}>"),
        Dict::Instance { name, args, .. } => {
            let args: Vec<String> = args.iter().map(dict).collect();
            format!("<{name}>({})", args.join(", "))
        }
        Dict::Given(g) => format!("?{g}"),
        Dict::Field(local) => format!("@{local}"),
        Dict::Super { dict: d, class } => format!("{}.{class}", dict(d)),
        Dict::Structural { class, fields } => {
            let fields: Vec<String> = fields.iter().map(dict).collect();
            format!("{class}{{{}}}", fields.join(", "))
        }
        Dict::Primitive { constraint, .. } => format!("#{constraint}"),
        Dict::Site { place, .. } => format!("#CallSite {place}"),
        Dict::Error => "error".to_owned(),
    }
}

/// `!` after a runtime stage, `~` after a Sync one.
fn mark(stage: Stage) -> &'static str {
    match stage {
        Stage::Pure => "",
        Stage::Sync => "~",
        Stage::Runtime => "!",
    }
}

fn lift(l: &Lift) -> String {
    match l {
        Lift::Stage { from, to, result } => {
            let inner = result.as_deref().map_or(String::new(), lift);
            let here = if from == to { "." } else { mark(*to) };
            format!("({here}{inner})")
        }
        Lift::Eff(inner) => format!("eff{}", lift(inner)),
    }
}

fn exprs(items: &[Expr]) -> String {
    items.iter().map(expr).collect::<Vec<_>>().join(", ")
}

fn fields(fields: &[(String, Expr)]) -> String {
    fields
        .iter()
        .map(|(l, e)| format!("{l} = {}", expr(e)))
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn expr(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Lit(l) => lit(l),
        ExprKind::Local(n) => n.clone(),
        ExprKind::Global { module, name } => format!("{module}.{name}"),
        ExprKind::Method { dict: d, name } => format!("{}.{name}", dict(d)),
        ExprKind::Con(c) => c.name.clone(),
        ExprKind::Perform {
            effect: service,
            op: name,
            ..
        } => format!("{service}.{name}"),
        ExprKind::ServiceTag(key) => format!("tag {key}"),
        ExprKind::Label(l) => format!("label {l:?}"),
        ExprKind::Dicts(e, dicts) => {
            let dicts: Vec<String> = dicts.iter().map(dict).collect();
            format!("{}[{}]", expr(e), dicts.join(", "))
        }
        ExprKind::Abstract(dicts, e) => format!("(\\[{}] {})", dicts.join(", "), expr(e)),
        ExprKind::App {
            func,
            arg,
            stage,
            arg_first,
        } => {
            let bang = mark(*stage);
            if *arg_first {
                format!("({} |>{bang} {})", expr(arg), expr(func))
            } else {
                format!("({} {}){bang}", expr(func), expr(arg))
            }
        }
        ExprKind::Lambda { params, body } => {
            let params: Vec<String> = params
                .iter()
                .map(|p| format!("{}{}", pat(&p.pat), mark(p.stage)))
                .collect();
            format!("(fun {} -> {})", params.join(" "), expr(body))
        }
        ExprKind::Block(stmts, result) => {
            let mut parts: Vec<String> = stmts.iter().map(stmt).collect();
            parts.push(expr(result));
            format!("{{ {} }}", parts.join("; "))
        }
        ExprKind::If(c, t, f) => format!("(if {} then {} else {})", expr(c), expr(t), expr(f)),
        ExprKind::Match(s, a) => format!("(match {} {})", expr(s), arms(a)),
        ExprKind::Tuple(items) => format!("({})", exprs(items)),
        ExprKind::List(items) => format!("[{}]", exprs(items)),
        ExprKind::Record(f) => format!("{{{}}}", fields(f)),
        ExprKind::Update(r, f) => format!("{{{} with {}}}", expr(r), fields(f)),
        ExprKind::Field(r, l) => format!("{}.{l}", expr(r)),
        ExprKind::Binary(op, l, r) => format!("({} {} {})", expr(l), op.symbol(), expr(r)),
        ExprKind::Unwrap(e) => format!("{}?", expr(e)),
        ExprKind::Not(e) => format!("(not {})", expr(e)),
        ExprKind::Neg(_, e) => format!("(-{})", expr(e)),
        ExprKind::Ignore(e) => format!("(ignore {})", expr(e)),
        ExprKind::Await(e) => format!("(await {})", expr(e)),
        ExprKind::Task(e) => format!("(task {})", expr(e)),
        ExprKind::Generate {
            body,
            strict,
            sync,
            limit,
        } => {
            let kind = match (strict, sync) {
                (true, _) => "list",
                (false, false) => "seq",
                (false, true) => "seq~",
            };
            let limit = limit
                .as_deref()
                .map_or(String::new(), |l| format!(" take {}", expr(l)));
            format!("({kind}{limit} {})", expr(body))
        }
        ExprKind::Each {
            pat: p,
            source,
            body,
        } => {
            format!("(each {} <- {} {})", pat(p), expr(source), expr(body))
        }
        ExprKind::Yield(e) => format!("(yield {})", expr(e)),
        ExprKind::Scope { owner, outer, body } => format!(
            "(scope{}{} {})",
            owner.as_ref().map_or(String::new(), |o| format!(" {o}")),
            outer
                .as_ref()
                .map_or(String::new(), |o| format!(" outer {o}")),
            expr(body)
        ),
        ExprKind::Within(owner, e) => format!("(within {owner} {})", expr(e)),
        ExprKind::Handle(h) if h.is_try() => format!("(try {} {})", expr(&h.body), arms(&h.errors)),
        ExprKind::Handle(h) => handler(h),
        ExprKind::Lift(e, l) => format!("(lift{} {})", lift(l), expr(e)),
    }
}

fn stmt(s: &Stmt) -> String {
    match s {
        Stmt::Let(p, e) => format!("let {} = {}", pat(p), expr(e)),
        Stmt::Fun {
            name,
            rec,
            dicts,
            value,
        } => format!(
            "let {}{name}{} = {}",
            if *rec { "rec " } else { "" },
            given(dicts),
            expr(value)
        ),
        Stmt::Expr(e) => expr(e),
    }
}

fn arms(arms: &[Arm]) -> String {
    arms.iter()
        .map(|a| {
            let guard = a
                .guard
                .as_ref()
                .map_or(String::new(), |g| format!(" when {}", expr(g)));
            format!("| {}{guard} -> {}", pat(&a.pat), expr(&a.body))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn lit(l: &Lit) -> String {
    match l {
        Lit::Int(n) => n.to_string(),
        Lit::Float(x) => x.to_string(),
        Lit::BigInt(n) => format!("{n}n"),
        Lit::String(s) => format!("{s:?}"),
        Lit::Bool(b) => b.to_string(),
        Lit::Unit => "()".to_owned(),
    }
}

pub fn pat(p: &Pat) -> String {
    match p {
        Pat::Wild => "_".to_owned(),
        Pat::Bind(n) => n.clone(),
        Pat::Lit(l) => lit(l),
        Pat::Tuple(items) => format!("({})", pats(items, ", ")),
        Pat::List(items, None) => format!("[{}]", pats(items, ", ")),
        Pat::List(items, Some(rest)) if items.is_empty() => format!("[...{}]", pat(rest)),
        Pat::List(items, Some(rest)) => format!("[{}, ...{}]", pats(items, ", "), pat(rest)),
        Pat::Record(fields) => {
            let fields: Vec<String> = fields
                .iter()
                .map(|(l, p)| format!("{l} = {}", pat(p)))
                .collect();
            format!("{{{}}}", fields.join("; "))
        }
        Pat::Con(c, args) if args.is_empty() => c.name.clone(),
        Pat::Con(c, args) => format!("({} {})", c.name, pats(args, " ")),
        Pat::Or(alts) => format!("({})", pats(alts, " | ")),
    }
}

fn pats(items: &[Pat], sep: &str) -> String {
    items.iter().map(pat).collect::<Vec<_>>().join(sep)
}

/// `(handle [E.op class f …] body [return f] arms)`.
fn handler(h: &Handler) -> String {
    let effects: Vec<String> = h
        .effects
        .iter()
        .map(|e| {
            let ops = e.operations.iter();
            ops.map(|o| {
                format!(
                    "{}.{} {:?} {}",
                    e.effect,
                    o.name,
                    o.class,
                    expr(&o.function)
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
        })
        .collect();
    let ret = h
        .ret
        .as_ref()
        .map_or(String::new(), |r| format!(" return {}", expr(r)));
    format!(
        "(handle [{}] {}{ret} {})",
        effects.join(" "),
        expr(&h.body),
        arms(&h.errors)
    )
}
