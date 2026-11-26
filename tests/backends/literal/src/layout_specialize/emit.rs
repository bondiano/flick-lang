//! Whole-program JavaScript bundle emission for `layout-specialize` mode,
//! emitting directly from `flick_backend_support::specialize::SpecializedProgram`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use flick_backend_support::specialize::{
    SpecDict, SpecExpr, SpecExprKind, SpecStmt, SpecializedDef, SpecializedInstance,
    SpecializedProgram, SymbolId,
};
use flick_ir::{Dict, InstanceInfo, ItemKind, Program};

use super::expr::emit_expr;
use super::runtime::{RUNTIME_PRELUDE_JS, emit_foreign};

pub(super) fn mangle_id(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 4);
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            let _ = write!(out, "_x{:x}_", u32::from(ch));
        }
    }
    out
}

pub(super) fn sym_js_name(sym: &SymbolId) -> String {
    format!("$s_{}", mangle_id(sym.as_str()))
}

pub(super) fn local_js_name(name: &str) -> String {
    format!("$l_{}", mangle_id(name))
}

pub(super) fn dict_js_name(name: &str) -> String {
    format!("$d_{}", mangle_id(name))
}

fn is_lazy_def(def: &SpecializedDef) -> bool {
    !def.runtime_dicts.is_empty()
        || matches!(
            def.body.kind,
            SpecExprKind::Lambda { .. } | SpecExprKind::Abstract(_, _)
        )
}

#[allow(clippy::too_many_lines)]
fn collect_def_deps(expr: &SpecExpr, out: &mut BTreeSet<SymbolId>) {
    match &expr.kind {
        SpecExprKind::CallDirect { target, dicts }
        | SpecExprKind::CallFallback { target, dicts } => {
            out.insert(target.clone());
            for d in dicts {
                collect_dict_deps(d, out);
            }
        }
        SpecExprKind::CallForeign { dicts, .. }
        | SpecExprKind::Construct { dicts, .. }
        | SpecExprKind::Perform { dicts, .. } => {
            for d in dicts {
                collect_dict_deps(d, out);
            }
        }
        SpecExprKind::MethodCall { dict, .. } => collect_dict_deps(dict, out),
        SpecExprKind::Dicts(inner, dicts) => {
            for d in dicts {
                collect_dict_deps(d, out);
            }
            collect_def_deps(inner, out);
        }
        SpecExprKind::Box { expr: inner, .. }
        | SpecExprKind::Unbox { expr: inner, .. }
        | SpecExprKind::FieldOffset { record: inner, .. }
        | SpecExprKind::FieldByLabel { record: inner, .. }
        | SpecExprKind::Not(inner)
        | SpecExprKind::Unwrap(inner)
        | SpecExprKind::Neg(_, inner)
        | SpecExprKind::Ignore(inner)
        | SpecExprKind::Await(inner)
        | SpecExprKind::Task(inner)
        | SpecExprKind::Within(_, inner)
        | SpecExprKind::Lift(inner, _)
        | SpecExprKind::Yield(inner) => collect_def_deps(inner, out),
        SpecExprKind::UpdateOffset { record, fields } => {
            collect_def_deps(record, out);
            for (_, _, _, v) in fields {
                collect_def_deps(v, out);
            }
        }
        SpecExprKind::UpdateByLabel { record, fields } => {
            collect_def_deps(record, out);
            for (_, v) in fields {
                collect_def_deps(v, out);
            }
        }
        SpecExprKind::App { func, arg, .. }
        | SpecExprKind::Binary(_, func, arg)
        | SpecExprKind::WithOwnerEvidence(func, arg) => {
            collect_def_deps(func, out);
            collect_def_deps(arg, out);
        }
        SpecExprKind::Block(stmts, tail) => {
            for stmt in stmts {
                match stmt {
                    SpecStmt::Let { value, .. }
                    | SpecStmt::Var { value, .. }
                    | SpecStmt::Assign(_, value)
                    | SpecStmt::Fun { value, .. }
                    | SpecStmt::Expr(value) => collect_def_deps(value, out),
                }
            }
            collect_def_deps(tail, out);
        }
        SpecExprKind::If(c, t, e) => {
            collect_def_deps(c, out);
            collect_def_deps(t, out);
            collect_def_deps(e, out);
        }
        SpecExprKind::Match(s, arms) => {
            collect_def_deps(s, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    collect_def_deps(g, out);
                }
                collect_def_deps(&arm.body, out);
            }
        }
        SpecExprKind::Tuple(items) | SpecExprKind::List(items) => {
            for item in items {
                collect_def_deps(item, out);
            }
        }
        SpecExprKind::Record { fields, .. } | SpecExprKind::Dictionary(fields) => {
            for (_, v) in fields {
                collect_def_deps(v, out);
            }
        }
        SpecExprKind::Lambda { .. }
        | SpecExprKind::Abstract(_, _)
        | SpecExprKind::Generate { .. }
        | SpecExprKind::Each { .. }
        | SpecExprKind::Loop { .. }
        | SpecExprKind::ControlLoop { .. }
        | SpecExprKind::LoopExit { .. }
        | SpecExprKind::Handle(_)
        | SpecExprKind::Lit(_)
        | SpecExprKind::Local(_)
        | SpecExprKind::Var(_)
        | SpecExprKind::ServiceTag(_)
        | SpecExprKind::Label(_)
        | SpecExprKind::ReadOwnerEvidence => {}
    }
}

fn collect_dict_deps(dict: &SpecDict, out: &mut BTreeSet<SymbolId>) {
    match dict {
        SpecDict::Value { target } => {
            out.insert(target.clone());
        }
        SpecDict::Instance { args, .. } => {
            for a in args {
                collect_dict_deps(a, out);
            }
        }
        SpecDict::Super { dict, .. } => collect_dict_deps(dict, out),
        SpecDict::Structural { fields, .. } => {
            for f in fields {
                collect_dict_deps(f, out);
            }
        }
        SpecDict::Given(_)
        | SpecDict::Opened { .. }
        | SpecDict::Field(_)
        | SpecDict::Primitive { .. }
        | SpecDict::Site { .. }
        | SpecDict::Error => {}
    }
}

fn order_eager_defs(specialized: &SpecializedProgram, eager: &[SymbolId]) -> Vec<SymbolId> {
    fn visit(
        sym: &SymbolId,
        specialized: &SpecializedProgram,
        eager_set: &BTreeSet<SymbolId>,
        visited: &mut BTreeSet<SymbolId>,
        ordered: &mut Vec<SymbolId>,
    ) {
        if !visited.insert(sym.clone()) {
            return;
        }
        if let Some(def) = specialized.definitions.get(sym) {
            let mut deps = BTreeSet::new();
            collect_def_deps(&def.body, &mut deps);
            for dep in deps {
                if eager_set.contains(&dep) {
                    visit(&dep, specialized, eager_set, visited, ordered);
                }
            }
        }
        ordered.push(sym.clone());
    }

    let eager_set: BTreeSet<SymbolId> = eager.iter().cloned().collect();
    let mut visited = BTreeSet::new();
    let mut ordered = Vec::with_capacity(eager.len());

    for sym in eager {
        visit(sym, specialized, &eager_set, &mut visited, &mut ordered);
    }
    ordered
}

pub(super) fn emit_whole_program_js(
    program: &Program,
    specialized: &SpecializedProgram,
    entry_module: &str,
) -> String {
    let mut orig_instances: BTreeMap<(String, String), &InstanceInfo> = BTreeMap::new();
    for module in &program.modules {
        for item in &module.items {
            if let ItemKind::Instance { info, .. } = &item.kind {
                orig_instances.insert((module.name.clone(), info.name.clone()), info.as_ref());
            }
        }
    }

    let mut out = String::from(RUNTIME_PRELUDE_JS);

    for (sym, foreign) in &specialized.foreigns {
        let _ = writeln!(
            out,
            "const {} = {};",
            sym_js_name(sym),
            emit_foreign(&foreign.module, &foreign.foreign)
        );
    }

    for sym in specialized.instances.keys() {
        let _ = writeln!(out, "let {};", sym_js_name(sym));
    }
    for sym in specialized.definitions.keys() {
        let _ = writeln!(out, "let {};", sym_js_name(sym));
    }

    for (sym, inst) in &specialized.instances {
        let orig = orig_instances
            .get(&(inst.module.clone(), inst.name.clone()))
            .copied();
        let _ = writeln!(out, "{} = {};", sym_js_name(sym), emit_instance(inst, orig));
    }

    let mut lazy_defs = Vec::new();
    let mut eager_defs = Vec::new();
    for (sym, def) in &specialized.definitions {
        if is_lazy_def(def) {
            lazy_defs.push(sym.clone());
        } else {
            eager_defs.push(sym.clone());
        }
    }

    for sym in &lazy_defs {
        let def = &specialized.definitions[sym];
        let _ = writeln!(
            out,
            "{} = {};",
            sym_js_name(sym),
            emit_value_body(&def.runtime_dicts, &def.body)
        );
    }

    for sym in order_eager_defs(specialized, &eager_defs) {
        let def = &specialized.definitions[&sym];
        let _ = writeln!(
            out,
            "{} = {};",
            sym_js_name(&sym),
            emit_value_body(&def.runtime_dicts, &def.body)
        );
    }

    if let Some(entry_sym) = specialized
        .roots
        .get(&(entry_module.to_owned(), "main".to_owned()))
    {
        let entry_name = sym_js_name(entry_sym);
        let _ = writeln!(out, "export const main = {entry_name};");
    }

    out
}

fn emit_instance(inst: &SpecializedInstance, orig: Option<&InstanceInfo>) -> String {
    let mut body = String::from("(() => { const $d = { ");
    for (m_name, m_expr) in &inst.methods {
        let key = serde_json::to_string(m_name).expect("json key");
        let _ = write!(body, "{key}: {}, ", emit_expr(m_expr));
    }
    for (sup_name, sup_dict) in &inst.supers {
        let key = serde_json::to_string(&format!("${sup_name}")).expect("json key");
        let _ = write!(body, "get {key}() {{ return {}; }}, ", emit_dict(sup_dict));
    }
    if let Some(info) = orig
        && let Some((field_dicts, Some(ctors))) = &info.derived
        && info.class.1 == "Show"
        && !inst.methods.contains_key("show")
    {
        let mut arms = String::new();
        let mut dict_idx = 0usize;
        for con in ctors {
            let tag = format!("{}.{}.{}", con.module, con.ty, con.name);
            let tag_json = serde_json::to_string(&tag).expect("tag json");
            let name_json = serde_json::to_string(&con.name).expect("name json");
            if con.arity == 0 {
                let _ = write!(arms, "if (v._tag === {tag_json}) return {name_json}; ");
            } else {
                let mut parts = Vec::with_capacity(con.arity);
                for f_i in 0..con.arity {
                    let d_expr = field_dicts
                        .get(dict_idx)
                        .map_or_else(|| "({ show: $show })".to_owned(), emit_ir_dict);
                    dict_idx += 1;
                    parts.push(format!("$showParen(({d_expr}).show(v._f[{f_i}]))"));
                }
                let _ = write!(
                    arms,
                    "if (v._tag === {tag_json}) return {name_json} + \" \" + [{}].join(\" \"); ",
                    parts.join(", ")
                );
            }
        }
        let _ = write!(body, "show: (v) => {{ {arms} return $show(v); }}, ");
    }
    body.push_str("}; return $d; })()");
    for ctx_dict in inst.context.iter().rev() {
        body = format!("(({}) => {body})", dict_js_name(ctx_dict));
    }
    body
}

pub(super) fn emit_value_body(dicts: &[String], body: &SpecExpr) -> String {
    let mut s = emit_expr(body);
    for d in dicts.iter().rev() {
        s = format!("(({}) => {s})", dict_js_name(d));
    }
    s
}

pub(super) fn emit_dict(dict: &SpecDict) -> String {
    match dict {
        SpecDict::Given(name) => dict_js_name(name),
        SpecDict::Field(name) => local_js_name(name),
        SpecDict::Opened { binder, field } => {
            let f = serde_json::to_string(field).expect("field json");
            format!("({})[{f}]", local_js_name(binder))
        }
        SpecDict::Value { target } => sym_js_name(target),
        SpecDict::Instance { target, args, .. } => {
            let mut s = sym_js_name(target);
            for a in args {
                let _ = write!(s, "({})", emit_dict(a));
            }
            s
        }
        SpecDict::Super { dict, class } => {
            let key = serde_json::to_string(&format!("${class}")).expect("class json");
            format!("({})[{key}]", emit_dict(dict))
        }
        SpecDict::Structural { class, fields } => {
            if class == "Show" {
                let ds: Vec<String> = fields.iter().map(emit_dict).collect();
                format!("($structShow([{}]))", ds.join(", "))
            } else {
                "({ equals: (a) => (b) => $eq(a, b), show: $show })".to_owned()
            }
        }
        SpecDict::Primitive { labels, .. } => {
            let ls: Vec<String> = labels
                .iter()
                .map(|l| serde_json::to_string(l).expect("label json"))
                .collect();
            format!("[{}]", ls.join(", "))
        }
        SpecDict::Site { place, definition } => {
            let p = serde_json::to_string(place).expect("place json");
            let d = serde_json::to_string(definition).expect("def json");
            format!("({{ place: {p}, definition: {d} }})")
        }
        SpecDict::Error => "({})".to_owned(),
    }
}

fn emit_ir_dict(dict: &Dict) -> String {
    match dict {
        Dict::Given(name) => dict_js_name(name),
        Dict::Field(name) => local_js_name(name),
        Dict::Opened { binder, field } => {
            let f = serde_json::to_string(field).expect("field json");
            format!("({})[{f}]", local_js_name(binder))
        }
        _ => "({ show: $show })".to_owned(),
    }
}
