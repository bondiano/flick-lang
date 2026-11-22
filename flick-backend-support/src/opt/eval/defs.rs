//! Bounded, deterministic definition table for the portable-IR evaluator.
//!
//! Snapshots own-module pure definitions and transitively preloads reachable
//! external pure definitions and instance methods with deterministic `BTreeSet`
//! ordering, lazy polymorphic candidate generation, and hard caps on both
//! loaded definitions ([`MAX_EXTERNAL_DEFS`]) and attempted lookup probes
//! ([`MAX_EXTERNAL_PROBES`]).

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
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
#[derive(Debug, Default)]
pub struct DefTable {
    values: HashMap<(String, String), (Vec<String>, Expr)>,
    methods: HashMap<(String, String, String), (Vec<String>, Expr)>,
}

struct RefQueues {
    values: VecDeque<(String, String)>,
    methods: VecDeque<(String, String, String)>,
    seen_values: HashSet<(String, String)>,
    seen_methods: HashSet<(String, String, String)>,
    queued_values: usize,
    queued_methods: usize,
    ext_instances: BTreeSet<(String, String)>,
    polymorphic_methods: BTreeSet<String>,
}

impl RefQueues {
    fn new() -> Self {
        RefQueues {
            values: VecDeque::new(),
            methods: VecDeque::new(),
            seen_values: HashSet::new(),
            seen_methods: HashSet::new(),
            queued_values: 0,
            queued_methods: 0,
            ext_instances: BTreeSet::new(),
            polymorphic_methods: BTreeSet::new(),
        }
    }

    fn push_value(&mut self, owner: &str, name: &str) {
        if self.queued_values >= MAX_EXTERNAL_PROBES {
            return;
        }
        let key = (owner.to_owned(), name.to_owned());
        if self.seen_values.insert(key.clone()) {
            self.queued_values += 1;
            self.values.push_back(key);
        }
    }

    fn push_method(&mut self, owner: &str, instance: &str, method: &str) {
        if self.queued_methods >= MAX_EXTERNAL_PROBES {
            return;
        }
        let key = (owner.to_owned(), instance.to_owned(), method.to_owned());
        if self.seen_methods.insert(key.clone()) {
            self.queued_methods += 1;
            self.methods.push_back(key);
        }
    }

    fn insert_ext_instance(&mut self, owner: &str, instance: &str) {
        insert_bounded_set(
            &mut self.ext_instances,
            (owner.to_owned(), instance.to_owned()),
            MAX_EXTERNAL_PROBES,
        );
    }

    fn insert_poly_method(&mut self, method: &str) {
        insert_bounded_set(
            &mut self.polymorphic_methods,
            method.to_owned(),
            MAX_EXTERNAL_PROBES,
        );
    }
}

/// Lazy lexicographical iterator over `ext_instances × polymorphic_methods`.
///
/// Materializes only the two bounded factor vectors (`O(|I| + |M|)`) instead of
/// the full Cartesian product (`O(|I| × |M|)`), yielding candidates on demand.
struct PolyCursor {
    instances: Vec<(String, String)>,
    methods: Vec<String>,
    inst_idx: usize,
    method_idx: usize,
}

impl PolyCursor {
    fn from_queues(queues: &mut RefQueues) -> Self {
        PolyCursor {
            instances: mem::take(&mut queues.ext_instances).into_iter().collect(),
            methods: mem::take(&mut queues.polymorphic_methods)
                .into_iter()
                .collect(),
            inst_idx: 0,
            method_idx: 0,
        }
    }

    fn next(&mut self) -> Option<(String, String, String)> {
        if self.methods.is_empty() {
            return None;
        }
        while self.inst_idx < self.instances.len() {
            if self.method_idx < self.methods.len() {
                let (owner, instance) = &self.instances[self.inst_idx];
                let method = &self.methods[self.method_idx];
                self.method_idx += 1;
                return Some((owner.clone(), instance.clone(), method.clone()));
            }
            self.inst_idx += 1;
            self.method_idx = 0;
        }
        None
    }
}

impl DefTable {
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
        let mut queues = RefQueues::new();

        // Pass 1: register all own-module definitions before scanning bodies so
        // forward references within `module` never consume external probe slots.
        for item in &module.items {
            match &item.kind {
                ItemKind::Value { name, dicts, body } => {
                    let key = (module.name.clone(), name.clone());
                    queues.seen_values.insert(key.clone());
                    table.values.insert(key, (dicts.clone(), body.clone()));
                }
                ItemKind::Instance { info, methods } => {
                    for (m, code) in methods {
                        let key = (module.name.clone(), info.name.clone(), m.clone());
                        queues.seen_methods.insert(key.clone());
                        table
                            .methods
                            .insert(key, (info.context.clone(), code.clone()));
                    }
                    for (m, n, _) in &info.defaults {
                        if *n == 0 {
                            let key = (module.name.clone(), info.name.clone(), m.clone());
                            if queues.seen_methods.insert(key.clone())
                                && let Some(found) = lookup.raw_method(&module.name, &info.name, m)
                            {
                                table.methods.insert(key, found);
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
            match &item.kind {
                ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => {
                    enqueue_refs(body, &module.name, &mut queues);
                }
                ItemKind::Instance { info, methods } => {
                    for (_, code) in methods {
                        enqueue_refs(code, &module.name, &mut queues);
                    }
                    for (m, n, _) in &info.defaults {
                        if *n == 0
                            && let Some((_, body)) = table.method(&module.name, &info.name, m)
                        {
                            enqueue_refs(body, &module.name, &mut queues);
                        }
                    }
                }
                ItemKind::Foreign(_) | ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
            }
        }

        let mut external_loaded = 0_usize;
        let mut external_probes = 0_usize;
        let mut poly: Option<PolyCursor> = None;
        while external_loaded < MAX_EXTERNAL_DEFS && external_probes < MAX_EXTERNAL_PROBES {
            if let Some(key) = queues.values.pop_front() {
                external_probes += 1;
                if let Some((dicts, body)) = generics(&key.0, &key.1) {
                    enqueue_refs(&body, &module.name, &mut queues);
                    table.values.insert(key, (dicts, body));
                    external_loaded += 1;
                }
                continue;
            }
            if let Some(key) = queues.methods.pop_front() {
                external_probes += 1;
                if let Some((context, body)) = lookup.raw_method(&key.0, &key.1, &key.2) {
                    enqueue_refs(&body, &module.name, &mut queues);
                    table.methods.insert(key, (context, body));
                    external_loaded += 1;
                }
                continue;
            }
            let cursor = poly.get_or_insert_with(|| PolyCursor::from_queues(&mut queues));
            let Some(key) = cursor.next() else {
                break;
            };
            if !queues.seen_methods.insert(key.clone()) {
                continue;
            }
            external_probes += 1;
            if let Some((context, body)) = lookup.raw_method(&key.0, &key.1, &key.2) {
                enqueue_refs(&body, &module.name, &mut queues);
                table.methods.insert(key, (context, body));
                external_loaded += 1;
            }
        }
        table
    }

    /// Inserts a single value definition (useful for unit tests).
    pub fn insert_value(&mut self, module: String, name: String, dicts: Vec<String>, body: Expr) {
        self.values.insert((module, name), (dicts, body));
    }

    /// Inserts a single instance method definition (useful for unit tests).
    pub fn insert_method(
        &mut self,
        module: String,
        instance: String,
        method: String,
        context: Vec<String>,
        body: Expr,
    ) {
        self.methods
            .insert((module, instance, method), (context, body));
    }

    pub(super) fn value(&self, module: &str, name: &str) -> Option<(&[String], &Expr)> {
        self.values
            .get(&(module.to_owned(), name.to_owned()))
            .map(|(d, b)| (d.as_slice(), b))
    }

    pub(super) fn method(
        &self,
        module: &str,
        instance: &str,
        method: &str,
    ) -> Option<(&[String], &Expr)> {
        self.methods
            .get(&(module.to_owned(), instance.to_owned(), method.to_owned()))
            .map(|(c, b)| (c.as_slice(), b))
    }
}

/// Keeps the lexicographically smallest `cap` distinct values in `set`,
/// producing identical results regardless of insertion order.
fn insert_bounded_set<T: Ord>(set: &mut BTreeSet<T>, value: T, cap: usize) {
    if set.contains(&value) {
        return;
    }
    if set.len() < cap {
        set.insert(value);
    } else if let Some(last) = set.last()
        && &value < last
    {
        set.pop_last();
        set.insert(value);
    }
}

fn enqueue_refs(root: &Expr, current_module: &str, queues: &mut RefQueues) {
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        match &e.kind {
            ExprKind::Global { module, name } if module != current_module => {
                queues.push_value(module, name);
            }
            ExprKind::Method { dict, name } => {
                enqueue_dict_methods(dict, name, current_module, queues);
            }
            ExprKind::Dicts(_, dicts) => {
                for dict in dicts {
                    collect_external_instances(dict, current_module, queues);
                }
            }
            _ => {}
        }
        e.each_child(&mut |child| stack.push(child));
    }
}

fn enqueue_dict_methods(dict: &Dict, method: &str, current_module: &str, queues: &mut RefQueues) {
    let mut stack = vec![dict];
    while let Some(d) = stack.pop() {
        match d {
            Dict::Instance { module, name, args } => {
                queues.push_method(module, name, method);
                for arg in args {
                    collect_external_instances(arg, current_module, queues);
                    stack.push(arg);
                }
            }
            Dict::Super { dict, .. } => stack.push(dict),
            Dict::Structural { fields, .. } => {
                for field in fields {
                    stack.push(field);
                }
            }
            Dict::Given(_) | Dict::Field(_) => {
                queues.insert_poly_method(method);
            }
            Dict::Value { .. }
            | Dict::Opened { .. }
            | Dict::Primitive { .. }
            | Dict::Site { .. }
            | Dict::Error => {}
        }
    }
}

fn collect_external_instances(dict: &Dict, current_module: &str, queues: &mut RefQueues) {
    let mut stack = vec![dict];
    while let Some(d) = stack.pop() {
        match d {
            Dict::Instance { module, name, args } => {
                if module != current_module {
                    queues.insert_ext_instance(module, name);
                }
                for arg in args {
                    stack.push(arg);
                }
            }
            Dict::Super { dict, .. } => stack.push(dict),
            Dict::Structural { fields, .. } => {
                for field in fields {
                    stack.push(field);
                }
            }
            Dict::Given(_)
            | Dict::Value { .. }
            | Dict::Opened { .. }
            | Dict::Field(_)
            | Dict::Primitive { .. }
            | Dict::Site { .. }
            | Dict::Error => {}
        }
    }
}
