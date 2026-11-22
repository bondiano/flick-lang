//! Bounded, deterministic definition table for the portable-IR evaluator.
//!
//! Snapshots own-module pure definitions and transitively preloads reachable
//! external pure definitions and instance methods with deterministic
//! lexicographical ordering, lazy polymorphic candidate generation, and hard
//! caps on both loaded definitions ([`MAX_EXTERNAL_DEFS`]) and attempted lookup
//! probes ([`MAX_EXTERNAL_PROBES`]).

use std::collections::{HashMap, HashSet};
use std::mem;

use flick_ir::{Dict, Expr, ExprKind, ItemKind, Module};

use crate::Known;
use crate::opt::{Generics, devirt};

/// Maximum number of external definitions pre-loaded into a [`DefTable`] per round.
const MAX_EXTERNAL_DEFS: usize = 256;
/// Maximum number of external lookup probes (and distinct queued candidates)
/// attempted while building a [`DefTable`] per round.
pub(super) const MAX_EXTERNAL_PROBES: usize = 1_024;

/// Immutable snapshot of pure definitions and instance methods available to
/// the evaluator during an optimization round.
///
/// Shares a single `HashMap<String, (Vec<String>, Expr)>` (already
/// monomorphized by [`crate::opt::specialize`]) with disjoint NUL-delimited
/// keys for values (`"v\0<module>\0<name>"`) and methods
/// (`"m\0<module>\0<instance>\0<method>"`).
#[derive(Debug, Default)]
pub struct DefTable {
    defs: HashMap<String, (Vec<String>, Expr)>,
}

/// Joins `parts` with `'\0'` into a compact composite key.
pub(super) fn join_nul(parts: &[&str]) -> String {
    let mut cap = parts.len().saturating_sub(1);
    for part in parts {
        cap += part.len();
    }
    let mut k = String::with_capacity(cap);
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            k.push('\0');
        }
        k.push_str(part);
    }
    k
}

fn value_key(module: &str, name: &str) -> String {
    join_nul(&["v", module, name])
}

fn method_key(module: &str, instance: &str, method: &str) -> String {
    join_nul(&["m", module, instance, method])
}

#[inline(never)]
fn compact_sorted(vec: &mut Vec<String>, cap: usize) {
    vec.sort();
    vec.dedup();
    vec.truncate(cap);
}

fn push_bounded_sorted(vec: &mut Vec<String>, item: String, cap: usize) {
    vec.push(item);
    if vec.len() >= cap * 2 {
        compact_sorted(vec, cap);
    }
}

#[derive(Default)]
struct StrQueue {
    items: Vec<String>,
    head: usize,
}

impl StrQueue {
    fn push(&mut self, seen: &mut HashSet<String>, key: String) {
        if self.items.len() >= MAX_EXTERNAL_PROBES {
            return;
        }
        if seen.insert(key.clone()) {
            self.items.push(key);
        }
    }

    fn pop(&mut self) -> Option<String> {
        let slot = self.items.get_mut(self.head)?;
        self.head += 1;
        Some(mem::take(slot))
    }
}

#[derive(Default)]
struct RefQueues {
    values: StrQueue,
    methods: StrQueue,
    seen: HashSet<String>,
    ext_instances: Vec<String>,
    polymorphic_methods: Vec<String>,
}

/// Lazy lexicographical iterator over `ext_instances × polymorphic_methods`.
///
/// Materializes only the two bounded sorted factor vectors (`O(|I| + |M|)`)
/// instead of the full Cartesian product (`O(|I| × |M|)`).
struct PolyCursor {
    instances: Vec<String>,
    methods: Vec<String>,
    inst_idx: usize,
    method_idx: usize,
}

impl PolyCursor {
    fn from_queues(queues: &mut RefQueues) -> Self {
        let mut instances = mem::take(&mut queues.ext_instances);
        compact_sorted(&mut instances, MAX_EXTERNAL_PROBES);
        let mut methods = mem::take(&mut queues.polymorphic_methods);
        compact_sorted(&mut methods, MAX_EXTERNAL_PROBES);
        PolyCursor {
            instances,
            methods,
            inst_idx: 0,
            method_idx: 0,
        }
    }

    fn next(&mut self) -> Option<(&str, &str, &str)> {
        if self.methods.is_empty() {
            return None;
        }
        while let Some(inst) = self.instances.get(self.inst_idx) {
            if let Some(method) = self.methods.get(self.method_idx) {
                let Some((owner, instance)) = inst.split_once('\0') else {
                    self.inst_idx += 1;
                    self.method_idx = 0;
                    continue;
                };
                self.method_idx += 1;
                return Some((owner, instance, method.as_str()));
            }
            self.inst_idx += 1;
            self.method_idx = 0;
        }
        None
    }
}

fn split_value_key(key: &str) -> Option<(&str, &str)> {
    key.strip_prefix("v\0")?.split_once('\0')
}

fn split_method_key(key: &str) -> Option<(&str, &str, &str)> {
    let rest = key.strip_prefix("m\0")?;
    let (owner, rest) = rest.split_once('\0')?;
    let (instance, method) = rest.split_once('\0')?;
    Some((owner, instance, method))
}

impl DefTable {
    #[inline(never)]
    fn record_own(&mut self, seen: &mut HashSet<String>, key: String, val: (Vec<String>, Expr)) {
        seen.insert(key.clone());
        self.defs.insert(key, val);
    }

    /// Builds a [`DefTable`] for `module` by snapshotting own items and
    /// transitively pre-loading reachable external pure definitions via `generics`
    /// and `lookup` (bounded by [`MAX_EXTERNAL_DEFS`] and [`MAX_EXTERNAL_PROBES`]).
    pub fn build(
        module: &Module,
        lookup: &devirt::Lookup<'_>,
        generics: Generics<'_>,
        _lang: &Known<'_>,
    ) -> Self {
        let mut table = DefTable::default();
        let mut queues = RefQueues::default();

        // Pass 1: register all own-module definitions before scanning bodies so
        // forward references within `module` never consume external probe slots.
        for item in &module.items {
            match &item.kind {
                ItemKind::Value { name, dicts, body } => {
                    let key = value_key(&module.name, name);
                    table.record_own(&mut queues.seen, key, (dicts.clone(), body.clone()));
                }
                ItemKind::Instance { info, methods } => {
                    for (m, code) in methods {
                        let key = method_key(&module.name, &info.name, m);
                        table.record_own(
                            &mut queues.seen,
                            key,
                            (info.context.clone(), code.clone()),
                        );
                    }
                    for (m, n, _) in &info.defaults {
                        if *n == 0 {
                            let key = method_key(&module.name, &info.name, m);
                            if queues.seen.insert(key.clone())
                                && let Some(found) = lookup.raw_method(&module.name, &info.name, m)
                            {
                                table.defs.insert(key, found);
                            }
                        }
                    }
                }
                ItemKind::Destructure { .. }
                | ItemKind::Foreign(_)
                | ItemKind::Constructor(_)
                | ItemKind::Service { .. } => {}
            }
        }

        // Pass 2: collect external references from own items and own default methods.
        for item in &module.items {
            item.each_body(&mut |body| enqueue_refs(body, &module.name, &mut queues));
            if let ItemKind::Instance { info, .. } = &item.kind {
                for (m, n, _) in &info.defaults {
                    if *n == 0
                        && let Some((_, body)) = table.method(&module.name, &info.name, m)
                    {
                        enqueue_refs(body, &module.name, &mut queues);
                    }
                }
            }
        }

        let mut external_loaded = 0_usize;
        let mut external_probes = 0_usize;
        let mut poly: Option<PolyCursor> = None;
        while external_loaded < MAX_EXTERNAL_DEFS && external_probes < MAX_EXTERNAL_PROBES {
            let (key, found) = if let Some(key) = queues.values.pop() {
                let found = split_value_key(&key).and_then(|(owner, name)| {
                    external_probes += 1;
                    generics(owner, name)
                });
                (key, found)
            } else {
                let key = if let Some(key) = queues.methods.pop() {
                    key
                } else {
                    let cursor = poly.get_or_insert_with(|| PolyCursor::from_queues(&mut queues));
                    let Some((owner, instance, method)) = cursor.next() else {
                        break;
                    };
                    let key = method_key(owner, instance, method);
                    if !queues.seen.insert(key.clone()) {
                        continue;
                    }
                    key
                };
                let found = split_method_key(&key).and_then(|(owner, instance, method)| {
                    external_probes += 1;
                    lookup.raw_method(owner, instance, method)
                });
                (key, found)
            };
            if let Some((ctx, body)) = found {
                enqueue_refs(&body, &module.name, &mut queues);
                table.defs.insert(key, (ctx, body));
                external_loaded += 1;
            }
        }
        table
    }

    /// Inserts a single value definition (useful for unit tests).
    pub fn insert_value(&mut self, module: &str, name: &str, dicts: Vec<String>, body: Expr) {
        self.defs.insert(value_key(module, name), (dicts, body));
    }

    /// Inserts a single instance method definition (useful for unit tests).
    pub fn insert_method(
        &mut self,
        module: &str,
        instance: &str,
        method: &str,
        context: Vec<String>,
        body: Expr,
    ) {
        self.defs
            .insert(method_key(module, instance, method), (context, body));
    }

    pub(super) fn value(&self, module: &str, name: &str) -> Option<(&[String], &Expr)> {
        self.defs
            .get(value_key(module, name).as_str())
            .map(|(d, b)| (d.as_slice(), b))
    }

    pub(super) fn method(
        &self,
        module: &str,
        instance: &str,
        method: &str,
    ) -> Option<(&[String], &Expr)> {
        self.defs
            .get(method_key(module, instance, method).as_str())
            .map(|(c, b)| (c.as_slice(), b))
    }
}

fn enqueue_refs(root: &Expr, current_module: &str, queues: &mut RefQueues) {
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        match &e.kind {
            ExprKind::Global { module, name } if module != current_module => {
                queues
                    .values
                    .push(&mut queues.seen, value_key(module, name));
            }
            ExprKind::Method { dict, name } => {
                walk_dict(dict, Some(name), current_module, queues);
            }
            ExprKind::Dicts(_, dicts) => {
                for dict in dicts {
                    walk_dict(dict, None, current_module, queues);
                }
            }
            _ => {}
        }
        e.each_child(&mut |child| stack.push(child));
    }
}

fn walk_dict(root: &Dict, method: Option<&str>, current_module: &str, queues: &mut RefQueues) {
    let mut inst_stack: Vec<&Dict> = Vec::new();
    if let Some(method_name) = method {
        let mut stack = vec![root];
        while let Some(d) = stack.pop() {
            match d {
                Dict::Instance { module, name, args } => {
                    queues
                        .methods
                        .push(&mut queues.seen, method_key(module, name, method_name));
                    for arg in args {
                        stack.push(arg);
                        inst_stack.push(arg);
                    }
                }
                Dict::Super { dict, .. } => stack.push(dict),
                Dict::Structural { fields, .. } => {
                    for field in fields {
                        stack.push(field);
                    }
                }
                Dict::Given(_) | Dict::Field(_) => {
                    push_bounded_sorted(
                        &mut queues.polymorphic_methods,
                        method_name.to_owned(),
                        MAX_EXTERNAL_PROBES,
                    );
                }
                _ => {}
            }
        }
    } else {
        inst_stack.push(root);
    }

    while let Some(d) = inst_stack.pop() {
        match d {
            Dict::Instance { module, name, args } => {
                if module != current_module {
                    push_bounded_sorted(
                        &mut queues.ext_instances,
                        join_nul(&[module, name]),
                        MAX_EXTERNAL_PROBES,
                    );
                }
                for arg in args {
                    inst_stack.push(arg);
                }
            }
            Dict::Super { dict, .. } => inst_stack.push(dict),
            Dict::Structural { fields, .. } => {
                for field in fields {
                    inst_stack.push(field);
                }
            }
            _ => {}
        }
    }
}
