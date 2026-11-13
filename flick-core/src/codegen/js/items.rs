//! Top-level definitions, emitted in an order where each value is defined
//! before the code that runs at load time uses it.

use std::collections::{BTreeSet, HashMap};
use std::mem;

use flick_pretty::{Document, docvec};

use super::doc::{After, Before, Head, Js, MULTILINE, Stmt, array, block, call, lines, object};
use super::{Callee, Gen, Target, names};
use crate::codegen::order::post_order;
use crate::ir::{Expr, ExprKind, Item, ItemKind};
use flick_ir::{Constructor, Dict, InstanceInfo, Rep, default_method};

impl<'d> Gen<'_, 'd> {
    /// The code of each item, in order.
    pub(super) fn items(&mut self, items: &[Item]) -> Vec<Document<'d>> {
        let a = self.a;
        let order = self.order(items);
        let mut out = Vec::new();
        let mut grouped = vec![false; self.groups.len()];
        for i in order {
            let item = &items[i];
            // a group runs its members: it is defined before the first of them
            if let Some(g) = self.groups.iter().position(|g| g.items.contains(&i))
                && !mem::replace(&mut grouped[g], true)
            {
                out.push(self.group(g, items).doc);
            }
            let mark = mark!(self, item.span);
            let code = self.item(item);
            if !code.is_empty() {
                let code = lines(a, code.iter().map(|s| s.doc));
                out.push(docvec![a, mark.unwrap_or_else(|| a.empty()), code]);
            }
        }
        out
    }

    fn item(&mut self, item: &Item) -> Vec<Stmt<'d>> {
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
                match self.options.worker(self.module, name) {
                    Some(n) if n == params.len() => stmts.push(self.export(&worker_id, worker)),
                    Some(n) => {
                        stmts.push(self.constant(&worker_id, worker));
                        stmts.push(self.applying_worker(&id, n));
                    }
                    None => stmts.push(self.constant(&worker_id, worker)),
                }
                if self.workers.get(name).is_some_and(|&(_, curried)| curried) {
                    let curried = self.curried(&worker_id, params.len());
                    stmts.push(self.export(&id, curried));
                }
                stmts
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
                // the source map names `id` by `name` where it is defined
                let mark = mark!(self, item.span);
                #[cfg(feature = "native-project")]
                self.names.insert(self.spans.len() - 1, name.clone());
                let code = self.export_marked(&id, mark, value);
                match self.options.worker(self.module, name) {
                    Some(n) => vec![code, self.applying_worker(&id, n)],
                    None => vec![code],
                }
            }
            ItemKind::Destructure { pattern, body } => {
                let (stmts, value) = self.block(|g| g.expr(body));
                let value = self.scoped(&stmts, value);
                let t = self.temp();
                let mut out = vec![self.constant(&t, value)];
                let (binds, ()) = self.block(|g| g.destructure(pattern, &t));
                out.extend(binds.into_iter().map(|b| Stmt {
                    doc: docvec![self.a, "export ", b.doc],
                    ..b
                }));
                out
            }
            ItemKind::Foreign(f) => self.foreign(f),
            ItemKind::Constructor(c) => vec![self.constructor_item(c)],
            ItemKind::Service { name, key } => {
                let a = self.a;
                let service = self.rt("service");
                let value = call(a, service, &[self.string(key)]);
                let doc = docvec![
                    a,
                    "export const ",
                    names::service(name),
                    " = ",
                    value.at(a, Before::Assign, After::Semicolon),
                    ";"
                ];
                vec![Stmt::new(doc, value.flags)]
            }
            ItemKind::Instance { info, methods } => self.instance(info, methods),
        }
    }

    /// The exported worker `id$n` that applies the curried `id`: the value
    /// was optimized into another shape than importers expect.
    fn applying_worker(&self, id: &str, n: usize) -> Stmt<'d> {
        let heads: Vec<String> = (0..n).map(|i| format!("$a{i}")).collect();
        let applied = heads
            .iter()
            .fold(self.leaf(id), |f, head| call(self.a, f, &[self.leaf(head)]));
        let worker = self.lambda_of(&heads.join(", "), applied);
        self.export(&format!("{id}${n}"), worker)
    }

    /// The curried form of the worker `worker` of `n` parameters.
    pub(super) fn curried(&self, worker: &str, n: usize) -> Js<'d> {
        let heads: Vec<String> = (0..n).map(|i| format!("$a{i}")).collect();
        let args: Vec<Js<'d>> = heads.iter().map(|h| self.leaf(h)).collect();
        let mut curried = call(self.a, self.leaf(worker), &args);
        for head in heads.iter().rev() {
            curried = self.lambda_of(head, curried);
        }
        curried
    }

    /// `value` after the statements it needs, run in a scope of their own:
    /// the locals of two top-level values may share a name.
    fn scoped(&self, stmts: &[Stmt<'d>], value: Js<'d>) -> Js<'d> {
        if stmts.is_empty() {
            return value;
        }
        let a = self.a;
        let run = self.lambda_of("", self.arrow_body(stmts, value));
        let doc = docvec![
            a,
            "(",
            run.at(a, Before::Delimiter, After::Delimiter),
            ")()"
        ];
        Js::code(doc, run.after_paren(), &[run]).call()
    }

    fn constructor_item(&mut self, c: &Constructor) -> Stmt<'d> {
        let a = self.a;
        let name = names::ident(&c.name);
        if names::native(self.options.lang, c).is_some() {
            let native = self.constructor(c);
            return self.export(&name, native);
        }
        if let Rep::Literal(literal) = &c.rep {
            return self.export(&name, self.leaf(&names::enum_literal(literal)));
        }
        if c.rep != Rep::Tagged {
            // `unboxed` and `untagged`: a value is its one field
            let identity = self.lambda_of("_0", self.leaf("_0"));
            return self.export(&name, identity);
        }
        let package = self.options.package(&c.module);
        let tag = self.string(&names::tag(c, package));
        let fields: Vec<String> = (0..c.arity).map(|i| format!("_{i}")).collect();
        let mut entries = vec![docvec![a, "_tag: ", tag.doc]];
        entries.extend(fields.iter().map(|f| a.text(a.alloc_str(f))));
        let dicts: Vec<String> = (0..c.dicts).map(|i| format!("$d{i}")).collect();
        entries.extend(
            dicts
                .iter()
                .enumerate()
                .map(|(i, d)| docvec![a, "$", i, ": ", a.alloc_str(d)]),
        );
        let object = object(a, &entries, &[]);
        let mut value = if fields.is_empty() && dicts.is_empty() {
            object
        } else {
            // an object, not a block, as the body of the arrows
            let doc = docvec![a, "(", object.doc, ")"];
            Js::code(doc, Head::Paren, &[object])
        };
        for field in fields.iter().rev() {
            value = self.lambda_of(field, value);
        }
        if !dicts.is_empty() {
            value = self.lambda_of(&dicts.join(", "), value);
        }
        self.export(&name, value)
    }

    /// `{` the entries, each followed by a comma, one per line `}`.
    fn entries(&self, entries: &[Stmt<'d>]) -> Js<'d> {
        let a = self.a;
        let entries: Vec<Stmt<'d>> = entries
            .iter()
            .map(|e| Stmt::new(docvec![a, e.doc, ","], e.flags))
            .collect();
        let flags = entries.iter().fold(MULTILINE, |all, e| all | e.flags);
        Js::code(block(a, &entries), Head::Brace, &[]).with(flags)
    }

    /// `get name() { return value; }`.
    fn getter(&self, name: &str, value: Js<'d>) -> Stmt<'d> {
        let a = self.a;
        let body = block(a, &[Stmt::ret(a, None, value)]);
        Stmt::new(
            docvec![a, "get ", a.alloc_str(name), "() ", body],
            value.flags | MULTILINE,
        )
    }

    fn instance(&mut self, info: &InstanceInfo, methods: &[(String, Expr)]) -> Vec<Stmt<'d>> {
        let a = self.a;
        let mark = self.givens.len();
        // release: the methods a canonical use calls directly, before the dictionary
        let mut methods_first = Vec::new();
        let params = self.bind_givens(&info.context);
        let mut entries = Vec::new();
        for (class, dict) in &info.supers {
            let d = self.dict(dict);
            entries.push(self.getter(&format!("${class}"), d));
        }
        // a derived instance without methods written out is the runtime's to derive
        let body = if methods.is_empty()
            && let Some((fields, constructors)) = &info.derived
        {
            self.derived(info, fields, constructors.as_deref(), &entries)
        } else {
            for (name, value) in methods {
                let (stmts, v) = self.block(|g| g.function(&[], value, None));
                let v = self.scoped(&stmts, v);
                let entry = |v: Js<'d>| {
                    let doc = docvec![
                        a,
                        a.alloc_str(name),
                        ": ",
                        v.at(a, Before::Delimiter, After::Delimiter)
                    ];
                    Stmt::new(doc, v.flags)
                };
                match self.direct.get(&(info.name.clone(), name.clone())).cloned() {
                    Some(direct) => {
                        methods_first.push(self.export(&direct, v));
                        entries.push(entry(self.leaf(&direct)));
                    }
                    None => entries.push(entry(v)),
                }
            }
            for (name, own, extra) in &info.defaults {
                let (module, class) = &info.class;
                let value = self.global(module, &default_method(class, name));
                let own: Vec<String> = (0..*own).map(|i| format!("$o{i}")).collect();
                let dicts: Vec<Js<'d>> = [self.leaf("this")]
                    .into_iter()
                    .chain(extra.iter().map(|d| self.dict(d)))
                    .collect();
                let applied = if own.is_empty() {
                    call(a, value, &dicts)
                } else {
                    let args: Vec<Js<'d>> = dicts
                        .into_iter()
                        .chain(own.iter().map(|o| self.leaf(o)))
                        .collect();
                    self.lambda_of(&own.join(", "), call(a, value, &args))
                };
                entries.push(self.getter(name, applied));
            }
            self.entries(&entries).in_parens()
        };
        self.givens.truncate(mark);
        let id = names::instance(&info.name);
        if info.context.is_empty() {
            let value = self.pure(body);
            methods_first.push(self.exported(self.constant(&id, value)));
            methods_first
        } else {
            let value = self.lambda_of(&params, body);
            vec![self.exported(self.constant(&id, value))]
        }
    }

    /// `export stmt`.
    fn exported(&self, stmt: Stmt<'d>) -> Stmt<'d> {
        Stmt {
            doc: docvec![self.a, "export ", stmt.doc],
            ..stmt
        }
    }

    /// A derived instance; `supers` are the getters of its superclasses.
    fn derived(
        &mut self,
        info: &InstanceInfo,
        fields: &[Dict],
        constructors: Option<&[Constructor]>,
        supers: &[Stmt<'d>],
    ) -> Js<'d> {
        let a = self.a;
        let fields: Vec<Js<'d>> = fields.iter().map(|d| self.dict(d)).collect();
        let constructors = constructors.map(|cs| {
            let cs: Vec<Js<'d>> = cs
                .iter()
                .map(|c| {
                    let tag = self.tag_of(c);
                    let mut shape = vec![
                        self.string(&tag),
                        self.string(&c.name),
                        self.leaf(&c.arity.to_string()),
                    ];
                    match &c.rep {
                        Rep::Tagged => {}
                        Rep::Unboxed => shape.push(self.string("unboxed")),
                        Rep::Typeof(class) => shape.push(self.string(class.name())),
                        Rep::Literal(l) => {
                            shape.push(self.string("literal"));
                            shape.push(self.leaf(&names::enum_literal(l)));
                        }
                    }
                    array(a, &shape)
                })
                .collect();
            array(a, &cs)
        });
        let class = &info.class.1;
        let derive = self.rt("derive");
        let supers = if supers.is_empty() {
            Js::code(a.text("{}"), Head::Brace, &[])
        } else {
            self.entries(supers)
        };
        let args = vec![
            self.string(class),
            self.thunk(array(a, &fields)),
            constructors.unwrap_or_else(|| self.leaf("null")),
            supers,
        ];
        call(a, derive, &args)
    }

    fn tag_of(&self, c: &Constructor) -> String {
        let package = self.options.package(&c.module);
        names::tag(c, package)
    }

    /// Items in an order where everything an item uses is defined first.
    fn order(&self, items: &[Item]) -> Vec<usize> {
        let mut by_name: HashMap<String, usize> = HashMap::new();
        for (i, item) in items.iter().enumerate() {
            for name in defined(item) {
                by_name.insert(name, i);
            }
        }
        let mut uses = |i: usize| {
            let mut names = BTreeSet::new();
            item_uses(&items[i], self.module, &mut names);
            (names.iter())
                .filter_map(|n| by_name.get(n).copied())
                .collect()
        };
        post_order(items.len(), 0..items.len(), &mut uses)
    }
}

/// The JS names an item defines.
pub(super) fn defined(item: &Item) -> Vec<String> {
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
    }
}

fn item_uses(item: &Item, module: &str, out: &mut BTreeSet<String>) {
    // the mapping of `catch` is read when a call throws, after the module
    // has started, so it does not order the foreign function after it
    if !matches!(item.kind, ItemKind::Foreign(_)) {
        item.each_body(&mut |e| expr_uses(e, module, out));
    }
    if let ItemKind::Instance { info, .. } = &item.kind {
        let derived = info.derived.iter().flat_map(|(fields, _)| fields);
        let extra = info.defaults.iter().flat_map(|(_, _, extra)| extra);
        for d in (info.supers.iter().map(|(_, d)| d))
            .chain(derived)
            .chain(extra)
        {
            dict_uses(d, module, out);
        }
    }
}

fn dict_uses(d: &Dict, module: &str, out: &mut BTreeSet<String>) {
    d.each_global(&mut |instance, m, name| {
        if m == module {
            out.insert(if instance {
                names::instance(name)
            } else {
                names::ident(name)
            });
        }
    });
}

fn expr_uses(e: &Expr, module: &str, out: &mut BTreeSet<String>) {
    match &e.kind {
        ExprKind::Global { module: m, name } if m == module => {
            out.insert(names::ident(name));
        }
        ExprKind::Con(c) if c.module == module => {
            out.insert(names::ident(&c.name));
        }
        ExprKind::Perform { effect: key, .. } | ExprKind::ServiceTag(key) => {
            if let Some((m, name)) = key.rsplit_once('.')
                && m == module
            {
                out.insert(names::service(name));
            }
        }
        ExprKind::Handle(h) => {
            for handled in &h.effects {
                if let Some((m, name)) = handled.effect.rsplit_once('.')
                    && m == module
                {
                    out.insert(names::service(name));
                }
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
