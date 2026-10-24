//! Top-level definitions, emitted in an order where each value is defined
//! before the code that runs at load time uses it.

use std::collections::{BTreeSet, HashMap};

use super::expr::arrow_body;
use super::schema::{plan_uses, schema_names};
use super::{Gen, Target, indent, names};
use crate::ir::{Expr, ExprKind, Foreign, ForeignMode, Item, ItemKind};
use crate::types::{Constructor, Dict, InstanceInfo, Rep, companion_name, default_method};

impl Gen<'_> {
    pub(super) fn items(&mut self, items: &[Item]) -> String {
        let order = self.order(items);
        let mut out = String::new();
        for i in order {
            let item = &items[i];
            let mark = self.mark(item.span);
            let code = self.item(item);
            if !code.is_empty() {
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
                let target = Target::Global(format!("{}.{name}", self.module));
                let (mut stmts, worker) =
                    self.block(|g| g.lambda(params, body, Some(target), true));
                let id = names::ident(name);
                let worker_id = format!("{id}${}", params.len());
                stmts.push(self.constant(&worker_id, &worker));
                if !self.workers.get(name).is_some_and(|&(_, curried)| curried) {
                    return stmts.join("\n");
                }
                let heads: Vec<String> = (0..params.len()).map(|i| format!("$a{i}")).collect();
                let mut curried = format!("{worker_id}({})", heads.join(", "));
                for head in heads.iter().rev() {
                    curried = format!("{} {curried}", self.arrow(head));
                }
                stmts.push(self.export(&id, &curried));
                stmts.join("\n")
            }
            ItemKind::Value { name, dicts, body } => {
                let target = Target::Global(format!("{}.{name}", self.module));
                let (stmts, value) = self.block(|g| g.function(dicts, body, Some(target)));
                let value = self.scoped(&stmts, value);
                self.export(&names::ident(name), &value)
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
            ItemKind::Schema(item) => self.schema(item),
        }
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

    fn foreign(&mut self, f: &Foreign) -> String {
        let alias = format!("$f_{}", f.name);
        let target = match &f.module {
            Some(module) => {
                let spec = (self.options.foreign)(self.module, module);
                self.foreign.push((spec, f.export.clone(), alias.clone()));
                alias
            }
            None => format!("globalThis.{}", f.export),
        };
        let info = &f.info;
        let dicts: Vec<String> = (0..f.dicts).map(|i| format!("$d{i}")).collect();
        let params: Vec<String> = (0..info.stages_taken()).map(|i| format!("$a{i}")).collect();
        let runtime = params
            .len()
            .checked_sub(1)
            .and_then(|i| f.stages.get(i))
            .copied()
            .unwrap_or(false);
        if f.arity == 0 && params.is_empty() && f.dicts == 0 && f.mode == ForeignMode::Pure {
            return self.export(&names::ident(&f.name), &target);
        }
        let mut args: Vec<String> = dicts.clone();
        for (i, param) in params.iter().take(f.arity).enumerate() {
            let arg = match info.callbacks.get(i).copied().flatten() {
                _ if info.this && i == 0 => param.clone(),
                Some(1) => param.clone(),
                Some(n) => {
                    let xs: Vec<String> = (0..n).map(|k| format!("$x{k}")).collect();
                    let applied = xs.iter().fold(String::new(), |acc, x| acc + "(" + x + ")");
                    format!("{} {param}{applied}", self.arrow(&xs.join(", ")))
                }
                None if info.optional.contains(&i) => {
                    format!("{}({param})", self.rt("optionalArg"))
                }
                None if info.variadic && i + 1 == f.arity => format!("...{param}"),
                None => param.clone(),
            };
            args.push(arg);
        }
        if info.cancel {
            args.push("$signal".to_owned());
        }
        let call = if info.this && !args.is_empty() {
            let receiver = args.remove(dicts.len());
            let rest: Vec<String> = [receiver].into_iter().chain(args).collect();
            format!("{target}.call({})", rest.join(", "))
        } else {
            format!("{target}({})", args.join(", "))
        };
        let result = |g: &mut Self, v: &str| {
            let v = if info.result.nullable {
                format!("{}({v})", g.rt("fromNullable"))
            } else {
                v.to_owned()
            };
            if info.result.unit {
                format!("(void {v})")
            } else {
                v
            }
        };
        let catch = f.catch.as_ref().map(|c| self.expr(c));
        let mut body = match (f.mode, runtime) {
            (ForeignMode::Pure, _) | (_, false) => result(self, &call),
            (ForeignMode::Sync, true) => {
                let value = result(self, &call);
                match catch {
                    Some(c) => format!("{}(() => {value}, {c})", self.rt("foreignSync")),
                    None => format!("{}(() => {value})", self.effect("sync")),
                }
            }
            (ForeignMode::Async, true) => {
                let value = result(self, "$v");
                let start = format!(
                    "{} Promise.resolve({call}).then(($v) => {value})",
                    self.arrow("$signal")
                );
                let catch = catch.map_or(String::new(), |c| format!(", {c}"));
                format!("{}({start}{catch})", self.rt("foreignAsync"))
            }
            (ForeignMode::Effect, true) => {
                let effect = match catch {
                    Some(c) => format!("{}(() => {call}, {c})", self.rt("foreignEffect")),
                    None => format!("{}(() => {call})", self.effect("suspend")),
                };
                if info.result.nullable || info.result.unit {
                    let value = result(self, "$v");
                    format!("{}({effect}, ($v) => {value})", self.effect("map"))
                } else {
                    effect
                }
            }
        };
        for param in params.iter().rev() {
            body = format!("{} {body}", self.arrow(param));
        }
        if !dicts.is_empty() {
            body = format!("{} {body}", self.arrow(&dicts.join(", ")));
        }
        self.export(&names::ident(&f.name), &body)
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
                            methods_first.push(self.constant(&direct, &v));
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
        // `Cli.Value` of a refined type parses the base, then checks the predicate
        let refined = match (class.as_str(), constructors.as_str()) {
            ("Value", "null") => {
                let ty = info.name.rsplit(' ').next().unwrap_or_default();
                format!(", {}, {}", companion_name(ty, "check"), names::string(ty))
            }
            _ => String::new(),
        };
        format!(
            "{derive}({}, () => [{}], {constructors}, {supers}{refined})",
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
        ItemKind::Schema(item) => schema_names(item),
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
        ItemKind::Schema(item) => {
            let mut uses = Vec::new();
            plan_uses(&item.plan, module, &mut uses);
            out.extend(uses);
        }
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
