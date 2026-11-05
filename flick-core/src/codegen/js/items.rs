//! Top-level definitions, emitted in an order where each value is defined
//! before the code that runs at load time uses it.

use std::collections::{BTreeSet, HashMap};
use std::mem;

use super::expr::arrow_body;
use super::{Callee, Gen, Target, indent, names};
use crate::ir::{Expr, ExprKind, Item, ItemKind};
use crate::types::{Constructor, Dict, InstanceInfo, Rep, companion_name, default_method};

impl Gen<'_> {
    pub(super) fn items(&mut self, items: &[Item]) -> String {
        let order = self.order(items);
        let mut out = String::new();
        let mut grouped = vec![false; self.groups.len()];
        for i in order {
            let item = &items[i];
            // a group runs its members: it is defined before the first of them
            if let Some(g) = self.groups.iter().position(|g| g.items.contains(&i))
                && !mem::replace(&mut grouped[g], true)
            {
                out.push_str(&self.group(g, items));
                out.push_str("\n\n");
            }
            #[cfg(feature = "source-maps")]
            let mark = self.mark(item.span);
            let code = self.item(item);
            if !code.is_empty() {
                #[cfg(feature = "source-maps")]
                out.push_str(&mark);
                out.push_str(&code);
                out.push_str("\n\n");
            }
        }
        out.truncate(out.trim_end().len());
        out.push('\n');
        out
    }

    fn item(&mut self, item: &Item) -> String {
        match &item.kind {
            ItemKind::Value {
                name,
                dicts,
                body:
                    Expr {
                        kind: ExprKind::Lambda { params, body },
                        ..
                    },
            } if dicts.is_empty() && self.workers.contains_key(name) => {
                let target = Target {
                    callee: Callee::Global(format!("{}.{name}", self.module)),
                    dicts: Vec::new(),
                };
                let (mut stmts, worker) =
                    self.block(|g| g.lambda(params, body, Some(target), true));
                let id = names::ident(name);
                let worker_id = format!("{id}${}", params.len());
                match (self.options.workers)(self.module, name) {
                    Some(n) if n == params.len() => stmts.push(self.export(&worker_id, &worker)),
                    Some(n) => {
                        stmts.push(self.constant(&worker_id, &worker));
                        stmts.push(self.applying_worker(&id, n));
                    }
                    None => stmts.push(self.constant(&worker_id, &worker)),
                }
                if self.workers.get(name).is_some_and(|&(_, curried)| curried) {
                    stmts.push(self.export(&id, &self.curried(&worker_id, params.len())));
                }
                stmts.join("\n")
            }
            ItemKind::Value { name, dicts, body } => {
                let target = Target {
                    callee: Callee::Global(format!("{}.{name}", self.module)),
                    dicts: dicts.clone(),
                };
                let (stmts, value) = self.block(|g| g.function(dicts, body, Some(target)));
                let value = self.scoped(&stmts, value);
                let value = self.labelled(name, value);
                let id = names::ident(name);
                let code = self.export(&id, &value);
                #[cfg(feature = "source-maps")]
                let code = self.named(code, &id, name, item.span);
                match (self.options.workers)(self.module, name) {
                    Some(n) => format!("{code}\n{}", self.applying_worker(&id, n)),
                    None => code,
                }
            }
            ItemKind::Destructure { pattern, body } => {
                let (stmts, value) = self.block(|g| g.expr(body));
                let value = self.scoped(&stmts, value);
                let t = self.temp();
                let mut out = vec![self.constant(&t, &value)];
                let (binds, ()) = self.block(|g| g.destructure(pattern, &t));
                out.extend(binds.into_iter().map(|b| format!("export {b}")));
                out.join("\n")
            }
            ItemKind::Foreign(f) => self.foreign(f),
            ItemKind::Constructor(c) => self.constructor_item(c),
            ItemKind::Service { name, key } => {
                let service = self.rt("service");
                format!(
                    "export const {} = {service}({});",
                    names::service(name),
                    names::string(key)
                )
            }
            ItemKind::Instance { info, methods } => self.instance(info, methods),
            ItemKind::Refined { name, check } => self.refined(name, check),
        }
    }

    /// The exported worker `id$n` that applies the curried `id`: the value
    /// was optimized into another shape than importers expect.
    fn applying_worker(&self, id: &str, n: usize) -> String {
        let heads: Vec<String> = (0..n).map(|i| format!("$a{i}")).collect();
        let worker = format!(
            "{} {id}({})",
            self.arrow(&heads.join(", ")),
            heads.join(")(")
        );
        self.export(&format!("{id}${n}"), &worker)
    }

    /// The curried form of the worker `worker` of `n` parameters.
    pub(super) fn curried(&self, worker: &str, n: usize) -> String {
        let heads: Vec<String> = (0..n).map(|i| format!("$a{i}")).collect();
        let mut curried = format!("{worker}({})", heads.join(", "));
        for head in heads.iter().rev() {
            curried = format!("{} {curried}", self.arrow(head));
        }
        curried
    }

    /// `value` after the statements it needs, run in a scope of their own:
    /// the locals of two top-level values may share a name.
    fn scoped(&self, stmts: &[String], value: String) -> String {
        if stmts.is_empty() {
            value
        } else {
            format!("({} {})()", self.arrow(""), arrow_body(stmts, &value))
        }
    }

    fn constructor_item(&mut self, c: &Constructor) -> String {
        let name = names::ident(&c.name);
        if names::native(c).is_some() {
            let native = self.constructor(c);
            return self.export(&name, &native);
        }
        if c.rep != Rep::Tagged {
            // `unboxed` and `untagged`: a value is its one field
            return self.export(&name, &format!("{} _0", self.arrow("_0")));
        }
        let package = (self.options.package)(&c.module);
        let tag = names::string(&names::tag(c, package.as_deref()));
        let fields: Vec<String> = (0..c.arity).map(|i| format!("_{i}")).collect();
        let mut value = if fields.is_empty() {
            format!("{{ _tag: {tag} }}")
        } else {
            format!("({{ _tag: {tag}, {} }})", fields.join(", "))
        };
        for field in fields.iter().rev() {
            value = format!("{} {value}", self.arrow(field));
        }
        self.export(&name, &value)
    }

    fn instance(&mut self, info: &InstanceInfo, methods: &[(String, Expr)]) -> String {
        let mark = self.givens.len();
        // release: the methods a canonical use calls directly, before the dictionary
        let mut methods_first = Vec::new();
        let params = self.bind_givens(&info.context);
        let mut entries = Vec::new();
        for (class, dict) in &info.supers {
            let d = self.dict(dict);
            entries.push(format!("get ${class}() {{\n  return {d};\n}}"));
        }
        let body = if let Some((fields, constructors)) = &info.derived {
            self.derived(info, fields, constructors.as_deref(), &entries)
        } else {
            {
                for (name, value) in methods {
                    let (stmts, v) = self.block(|g| g.function(&[], value, None));
                    let v = self.scoped(&stmts, v);
                    match self.direct.get(&(info.name.clone(), name.clone())).cloned() {
                        Some(direct) => {
                            methods_first.push(self.export(&direct, &v));
                            entries.push(format!("{name}: {direct}"));
                        }
                        None => entries.push(format!("{name}: {v}")),
                    }
                }
                for (name, own) in &info.defaults {
                    let (module, class) = &info.class;
                    let value = self.global(module, &default_method(class, name));
                    let own: Vec<String> = (0..*own).map(|i| format!("$o{i}")).collect();
                    let call = if own.is_empty() {
                        format!("{value}(this)")
                    } else {
                        let list = own.join(", ");
                        format!("{} {value}(this, {list})", self.arrow(&list))
                    };
                    entries.push(format!("get {name}() {{\n  return {call};\n}}"));
                }
                let entries: Vec<String> = entries.into_iter().map(|e| format!("{e},")).collect();
                format!("({{\n{}}})", indent(&entries))
            }
        };
        self.givens.truncate(mark);
        let id = names::instance(&info.name);
        if info.context.is_empty() {
            methods_first.push(format!(
                "export {}",
                self.constant(&id, &super::pure(&body))
            ));
            methods_first.join("\n")
        } else {
            let value = format!("{} {body}", self.arrow(&params));
            format!("export {}", self.constant(&id, &value))
        }
    }

    /// A derived instance; `supers` are the getters of its superclasses.
    fn derived(
        &mut self,
        info: &InstanceInfo,
        fields: &[Dict],
        constructors: Option<&[Constructor]>,
        supers: &[String],
    ) -> String {
        let fields: Vec<String> = fields.iter().map(|d| self.dict(d)).collect();
        let constructors = constructors.map_or("null".to_owned(), |cs| {
            let cs: Vec<String> = cs
                .iter()
                .map(|c| {
                    let tag = self.tag_of(c);
                    let rep = match c.rep {
                        Rep::Tagged => String::new(),
                        Rep::Unboxed => ", \"unboxed\"".to_owned(),
                        Rep::Typeof(class) => format!(", {}", names::string(class.name())),
                    };
                    format!(
                        "[{}, {}, {}{rep}]",
                        names::string(&tag),
                        names::string(&c.name),
                        c.arity
                    )
                })
                .collect();
            format!("[{}]", cs.join(", "))
        });
        let class = &info.class.1;
        let derive = self.rt("derive");
        let supers = if supers.is_empty() {
            "{}".to_owned()
        } else {
            let entries: Vec<String> = supers.iter().map(|e| format!("{e},")).collect();
            format!("{{\n{}}}", indent(&entries))
        };
        format!(
            "{derive}({}, () => [{}], {constructors}, {supers})",
            names::string(class),
            fields.join(", ")
        )
    }

    fn tag_of(&self, c: &Constructor) -> String {
        let package = (self.options.package)(&c.module);
        names::tag(c, package.as_deref())
    }

    /// Items in an order where everything an item uses is defined first.
    fn order(&self, items: &[Item]) -> Vec<usize> {
        let mut by_name: HashMap<String, usize> = HashMap::new();
        for (i, item) in items.iter().enumerate() {
            for name in defined(item) {
                by_name.insert(name, i);
            }
        }
        let uses: Vec<Vec<usize>> = items
            .iter()
            .map(|item| {
                let mut names = BTreeSet::new();
                item_uses(item, self.module, &mut names);
                names
                    .iter()
                    .filter_map(|n| by_name.get(n).copied())
                    .collect()
            })
            .collect();
        let mut state = vec![0u8; items.len()];
        let mut out = Vec::new();
        for start in 0..items.len() {
            visit(start, &uses, &mut state, &mut out);
        }
        out
    }
}

fn visit(i: usize, uses: &[Vec<usize>], state: &mut [u8], out: &mut Vec<usize>) {
    if state[i] != 0 {
        return;
    }
    state[i] = 1;
    for &j in &uses[i] {
        visit(j, uses, state, out);
    }
    state[i] = 2;
    out.push(i);
}

/// The JS names an item defines.
fn defined(item: &Item) -> Vec<String> {
    match &item.kind {
        ItemKind::Value { name, .. } => vec![names::ident(name)],
        ItemKind::Destructure { pattern, .. } => {
            let mut out = Vec::new();
            pattern.binders(&mut out);
            out.into_iter().map(names::ident).collect()
        }
        ItemKind::Foreign(f) => vec![names::ident(&f.name)],
        ItemKind::Constructor(c) => vec![names::ident(&c.name)],
        ItemKind::Service { name, .. } => vec![names::service(name)],
        ItemKind::Instance { info, .. } => vec![names::instance(&info.name)],
        ItemKind::Refined { name, .. } => ["check", "parse", "value"]
            .iter()
            .map(|m| companion_name(name, m))
            .collect(),
    }
}

fn item_uses(item: &Item, module: &str, out: &mut BTreeSet<String>) {
    match &item.kind {
        ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => {
            expr_uses(body, module, out);
        }
        ItemKind::Instance { info, methods } => {
            for (_, d) in &info.supers {
                dict_uses(d, module, out);
            }
            if let Some((fields, _)) = &info.derived {
                for d in fields {
                    dict_uses(d, module, out);
                }
            }
            for (_, m) in methods {
                expr_uses(m, module, out);
            }
        }
        ItemKind::Refined { check, .. } => expr_uses(check, module, out),
        ItemKind::Foreign(_) | ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
    }
}

fn dict_uses(d: &Dict, module: &str, out: &mut BTreeSet<String>) {
    match d {
        Dict::Instance {
            module: m,
            name,
            args,
        } => {
            if m == module {
                out.insert(names::instance(name));
            }
            for a in args {
                dict_uses(a, module, out);
            }
        }
        Dict::Super { dict, .. } => dict_uses(dict, module, out),
        Dict::Structural { fields, .. } => {
            for f in fields {
                dict_uses(f, module, out);
            }
        }
        Dict::Given(_) | Dict::Primitive { .. } | Dict::Error => {}
    }
}

fn expr_uses(e: &Expr, module: &str, out: &mut BTreeSet<String>) {
    match &e.kind {
        ExprKind::Global { module: m, name } if m == module => {
            out.insert(names::ident(name));
        }
        ExprKind::Con(c) if c.module == module => {
            out.insert(names::ident(&c.name));
        }
        ExprKind::Operation { service: key, .. } | ExprKind::ServiceTag(key) => {
            if let Some((m, name)) = key.rsplit_once('.')
                && m == module
            {
                out.insert(names::service(name));
            }
        }
        ExprKind::Method { dict, .. } => dict_uses(dict, module, out),
        ExprKind::Dicts(_, dicts) => {
            for d in dicts {
                dict_uses(d, module, out);
            }
        }
        _ => {}
    }
    e.each_child(&mut |c| expr_uses(c, module, out));
}
