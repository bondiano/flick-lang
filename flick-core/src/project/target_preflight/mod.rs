//! Unified environment preflight: reachable-use checking for module `@when`,
//! `[package] when`, Effect-adapter bindings on JS `rt` (`FLICK-P064`), and
//! reachable gated services without a root handler (`FLICK-E020`).
//!
//! Unused imports and unreachable functions are never rejected. Apps check
//! reachability from their entries, runners, and loaded initializers; libraries
//! check their active public exports; test runs check their active test suites.

mod builtin;
mod facades;
mod gated;
mod locals;
mod prune;
mod refs;
mod roots;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use facades::{FacadeUse, eager_facade_uses, item_facade_uses, module_facade_uses};
pub(crate) use gated::needs_adapter;
use gated::scan_item_gated_services;
pub use gated::{
    gated_service, handled_services, handles, std_root_handlers, std_root_handlers_for,
};
pub(crate) use prune::{prune_compilation, prune_compilation_for_env};
use refs::{Reference, collect_item_references, initialization_refs, module_reexports};

pub use builtin::BuiltinPreflight;

use super::builtins::builtin_preflight_of;
use super::environment::ResolvedEnvironment;
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::effects::bindings;
use crate::ir::{ItemKind, Module, reach};
use crate::project::Compilation;
use crate::source::{SourceMap, Span};
use crate::types::builtins_of;

/// A runner selected for an app entry, with its optional span in `flick.toml`.
#[derive(Debug, Clone)]
pub(crate) struct RunnerRef {
    pub entry: String,
    pub module: String,
    pub span: Option<Span>,
}

/// Host lookup returning whether the selected backend handles a gated service.
pub(crate) type HandlesService<'a> = &'a dyn Fn(&str) -> Option<bool>;

/// Options controlling which roots environment preflight starts from.
pub(crate) struct CheckOptions<'a> {
    pub entries: &'a [(String, String, Option<Span>)],
    pub runners: &'a [RunnerRef],
    pub by_class: bool,
    pub for_tests: bool,
    pub test_paths: &'a [PathBuf],
    pub handles: Option<HandlesService<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Unavailable {
    ModuleWhen {
        module: String,
        condition: String,
    },
    PackageWhen {
        module: String,
        package: String,
        condition: String,
    },
    EffectAdapter {
        module: String,
        item: String,
    },
}

struct PathNode {
    label: String,
    parent: Option<usize>,
}

struct Preflight<'a> {
    compilation: &'a Compilation,
    sources: &'a SourceMap,
    env: &'a ResolvedEnvironment,
    handles: Option<HandlesService<'a>>,
    modules: Vec<&'a Module>,
    index: HashMap<reach::Key, (usize, usize)>,
    /// The built-in modules, read apart from their IR.
    builtins: &'a BuiltinPreflight,
    seen_builtins: HashSet<usize>,
    module_conditions: HashMap<String, Option<String>>,
    /// Package name and `[package] when` condition for each module in `compilation`.
    package_conditions: HashMap<String, (String, String)>,
    by_class: bool,
    check_adapter: bool,
    facade_cache: HashMap<String, Vec<FacadeUse>>,
    nodes: Vec<PathNode>,
    queue: VecDeque<WorkItem>,
    seen: HashSet<(usize, usize)>,
    seen_e020: HashSet<((usize, usize), u8)>,
    loaded: HashSet<String>,
    reported_p064: HashSet<(Unavailable, Option<Span>)>,
    reported_e020: HashSet<(String, Span)>,
    diagnostics: &'a mut Vec<Diagnostic>,
}

struct WorkItem {
    key: reach::Key,
    origin: Option<Span>,
    node: usize,
    covered: u8,
}

/// Reject reachable uses of modules, packages, foreign bindings (`FLICK-P064`),
/// and gated services (`FLICK-E020`) unavailable in `env`.
pub(crate) fn check(
    compilation: &Compilation,
    options: &CheckOptions<'_>,
    sources: &SourceMap,
    env: &ResolvedEnvironment,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let modules: Vec<&Module> = compilation.modules.iter().map(|m| &*m.ir).collect();
    let index = reach::index(&modules);

    let mut module_conditions = HashMap::new();
    let mut package_conditions = HashMap::new();
    for builtin in builtins_of(options.by_class) {
        module_conditions.insert(builtin.module.clone(), builtin.when.clone());
    }
    for compiled in &compilation.modules {
        module_conditions.insert(compiled.name.clone(), compiled.interface.when.clone());
        if let Some(pkg) = &compiled.package {
            if let Some(cond) = compilation.package_when.get(pkg) {
                package_conditions.insert(compiled.name.clone(), (pkg.clone(), cond.clone()));
            }
        } else if let Some((pkg, cond)) = &compilation.own_package_when {
            package_conditions.insert(compiled.name.clone(), (pkg.clone(), cond.clone()));
        }
    }

    let check_adapter = env.tags.contains("js")
        && !env.tags.contains("effect")
        && !env.tags.contains("effect-adapter");

    let mut state = Preflight {
        compilation,
        sources,
        env,
        handles: options.handles,
        modules,
        index,
        builtins: builtin_preflight_of(options.by_class),
        seen_builtins: HashSet::new(),
        module_conditions,
        package_conditions,
        by_class: options.by_class,
        check_adapter,
        facade_cache: HashMap::new(),
        nodes: Vec::new(),
        queue: VecDeque::new(),
        seen: HashSet::new(),
        seen_e020: HashSet::new(),
        loaded: HashSet::new(),
        reported_p064: HashSet::new(),
        reported_e020: HashSet::new(),
        diagnostics,
    };

    state.seed_roots(options);
    state.run();
}

impl Preflight<'_> {
    fn add_node(&mut self, label: String, parent: Option<usize>) -> usize {
        let id = self.nodes.len();
        self.nodes.push(PathNode { label, parent });
        id
    }

    fn format_path(&self, mut node: usize, leaf: Option<&str>) -> String {
        let mut parts = Vec::new();
        if let Some(leaf) = leaf {
            parts.push(leaf.to_owned());
        }
        let mut guard = 0;
        loop {
            let current = &self.nodes[node];
            if parts.last().is_none_or(|prev| prev != &current.label) {
                parts.push(current.label.clone());
            }
            match current.parent {
                Some(p) if guard < 64 => {
                    node = p;
                    guard += 1;
                }
                _ => break,
            }
        }
        parts.reverse();
        parts
            .into_iter()
            .map(|p| format!("`{p}`"))
            .collect::<Vec<_>>()
            .join(" → ")
    }

    fn module_unavailable(&self, module: &str) -> Option<Unavailable> {
        if let Some(Some(cond)) = self.module_conditions.get(module)
            && !self.env.matches(cond)
        {
            return Some(Unavailable::ModuleWhen {
                module: module.to_owned(),
                condition: cond.clone(),
            });
        }
        if let Some((pkg, cond)) = self.package_conditions.get(module)
            && !self.env.matches(cond)
        {
            return Some(Unavailable::PackageWhen {
                module: module.to_owned(),
                package: pkg.clone(),
                condition: cond.clone(),
            });
        }
        None
    }

    fn report_unavailable(
        &mut self,
        reason: &Unavailable,
        span: Option<Span>,
        from_node: usize,
        leaf: Option<&str>,
    ) {
        if !self.reported_p064.insert((reason.clone(), span)) {
            return;
        }
        let path = self.format_path(from_node, leaf);
        let active = self.env.format_tags();
        let env_name = format!("{}/{}", self.env.backend, self.env.platform);
        let message = match reason {
            Unavailable::ModuleWhen { module, condition } => format!(
                "`{module}` is not available in environment `{env_name}` (requires `{condition}`; active tags: {active}; path: {path})"
            ),
            Unavailable::PackageWhen {
                module,
                package,
                condition,
            } => format!(
                "`{module}` (from package `{package}`) is not available in environment `{env_name}` (requires `{condition}`; active tags: {active}; path: {path})"
            ),
            Unavailable::EffectAdapter { module, item } => format!(
                "`{module}.{item}` requires `effect || effect-adapter` (the optional `{}` adapter on JS `rt`, or `[target.js] runtime = \"effect\"`; active tags: {active}; path: {path})",
                bindings::ADAPTER_PREFIX
            ),
        };
        let diag = match span {
            Some(at) => Diagnostic::error(codes::P064, message, at),
            None => Diagnostic::new(codes::P064, Severity::Error, message),
        };
        self.diagnostics.push(diag);
    }

    fn facades_of(&mut self, compiled_idx: usize) -> Vec<FacadeUse> {
        let name = &self.compilation.modules[compiled_idx].name;
        if let Some(cached) = self.facade_cache.get(name) {
            return cached.clone();
        }
        let computed = module_facade_uses(
            &self.compilation.modules[compiled_idx],
            self.compilation,
            self.sources,
            self.by_class,
        );
        self.facade_cache.insert(name.clone(), computed.clone());
        computed
    }

    fn check_via_chain(
        &mut self,
        via: &[String],
        symbol_name: &str,
        span: Option<Span>,
        from_node: usize,
    ) {
        for facade in via {
            self.enqueue_reference(
                Reference::ModuleOnly {
                    module: facade.clone(),
                    symbol: format!("{facade}.{symbol_name}"),
                    span,
                },
                from_node,
            );
        }
    }

    fn mark_loaded(&mut self, module_name: &str, from_node: usize) {
        if !self.loaded.insert(module_name.to_owned()) {
            return;
        }
        if self.module_unavailable(module_name).is_some() {
            return;
        }
        let Some(compiled_idx) = self
            .compilation
            .modules
            .iter()
            .position(|m| m.name == module_name)
        else {
            return;
        };

        // Re-exported values load their declaring modules and any intermediate
        // facades (contributing eager initializers and gating checks).
        let reexports = module_reexports(&self.compilation.modules[compiled_idx], self.sources);
        for (key, via, span) in reexports {
            self.check_via_chain(&via, &key.1, Some(span), from_node);
            if let Some(reason) = self.module_unavailable(&key.0) {
                self.report_unavailable(
                    &reason,
                    Some(span),
                    from_node,
                    Some(&format!("{}.{}", key.0, key.1)),
                );
            } else {
                self.mark_loaded(&key.0, from_node);
            }
        }

        // Eager top-level initializers of this loaded module.
        let mut init_refs = Vec::new();
        let mod_facades = self.facades_of(compiled_idx);
        for item in &self.compilation.modules[compiled_idx].ir.items {
            initialization_refs(item, &mut init_refs);
            for facade_use in eager_facade_uses(&mod_facades, item) {
                for facade in &facade_use.facades {
                    init_refs.push(Reference::ModuleOnly {
                        module: facade.clone(),
                        symbol: format!("{facade}.{}", facade_use.name),
                        span: Some(facade_use.span),
                    });
                }
            }
        }
        for r in init_refs {
            self.enqueue_reference(r, from_node);
        }
    }

    fn enqueue_reference(&mut self, r: Reference, from_node: usize) {
        match r {
            Reference::Value { key, span, covered } => {
                if let Some(reason) = self.module_unavailable(&key.0) {
                    self.report_unavailable(
                        &reason,
                        span,
                        from_node,
                        Some(&format!("{}.{}", key.0, key.1)),
                    );
                    return;
                }
                self.mark_loaded(&key.0, from_node);
                let label = self.display_key(&key);
                let next_node = match label {
                    Some(label) => self.add_node(label, Some(from_node)),
                    None => from_node,
                };
                self.queue.push_back(WorkItem {
                    key,
                    origin: span,
                    node: next_node,
                    covered,
                });
            }
            Reference::ModuleOnly {
                module,
                symbol,
                span,
            } => {
                if let Some(reason) = self.module_unavailable(&module) {
                    self.report_unavailable(&reason, span, from_node, Some(&symbol));
                } else {
                    self.mark_loaded(&module, from_node);
                }
            }
        }
    }

    fn display_key(&self, key: &reach::Key) -> Option<String> {
        // a built-in item of a key comes before a user one, as in one index
        if let Some(at) = self.builtins.find(key) {
            if let Some(name) = self.builtins.foreign_name(at) {
                return Some(format!("{}.{name}", key.0));
            }
        } else if let Some(&at) = self.index.get(key)
            && let ItemKind::Foreign(foreign) = &self.modules[at.0].items[at.1].kind
        {
            let name = foreign.source_name.as_deref().unwrap_or(&foreign.name);
            return Some(format!("{}.{name}", key.0));
        }
        if key.1.starts_with("$raw_") {
            return None;
        }
        Some(format!("{}.{}", key.0, key.1))
    }

    /// A reached built-in item: its uses, unless it is a foreign binding only
    /// the Effect adapter implements.
    fn visit_builtin(&mut self, at: usize, work: &WorkItem) {
        if !self.seen_builtins.insert(at) {
            return;
        }
        if self.check_adapter && self.builtins.needs_adapter(at, self.env.ui.as_deref()) {
            let item = self
                .builtins
                .foreign_name(at)
                .unwrap_or_default()
                .to_owned();
            let leaf = format!("{}.{item}", work.key.0);
            let reason = Unavailable::EffectAdapter {
                module: work.key.0.clone(),
                item,
            };
            self.report_unavailable(&reason, work.origin, work.node, Some(&leaf));
            return;
        }
        for r in self.builtins.uses(at, work.origin, work.covered) {
            self.enqueue_reference(r, work.node);
        }
    }

    fn run(&mut self) {
        while let Some(work) = self.queue.pop_front() {
            if let Some(at) = self.builtins.find(&work.key) {
                self.visit_builtin(at, &work);
                continue;
            }
            let Some(&at) = self.index.get(&work.key) else {
                continue;
            };
            let first_visit = self.seen.insert(at);
            let check_e020 =
                self.env.explicit_environment && self.seen_e020.insert((at, work.covered));
            if !first_visit && !check_e020 {
                continue;
            }
            let item = &self.modules[at.0].items[at.1];

            if first_visit
                && self.check_adapter
                && let ItemKind::Foreign(foreign) = &item.kind
                && needs_adapter(foreign, self.env.ui.as_deref())
            {
                let item_name = foreign
                    .source_name
                    .as_deref()
                    .unwrap_or(&foreign.name)
                    .to_owned();
                let leaf = format!("{}.{item_name}", work.key.0);
                self.report_unavailable(
                    &Unavailable::EffectAdapter {
                        module: work.key.0.clone(),
                        item: item_name,
                    },
                    work.origin,
                    work.node,
                    Some(&leaf),
                );
                continue;
            }

            if check_e020 {
                scan_item_gated_services(
                    item,
                    self.env,
                    self.handles,
                    work.covered,
                    &mut self.reported_e020,
                    self.diagnostics,
                );
            }

            let mut refs = Vec::new();
            collect_item_references(item, true, work.origin, work.covered, &mut refs);
            if first_visit {
                let item_ref = &self.compilation.modules[at.0].ir.items[at.1];
                let mod_facades = self.facades_of(at.0);
                for facade_use in item_facade_uses(&mod_facades, item_ref) {
                    for facade in &facade_use.facades {
                        refs.push(Reference::ModuleOnly {
                            module: facade.clone(),
                            symbol: format!("{facade}.{}", facade_use.name),
                            span: Some(facade_use.span),
                        });
                    }
                }
            }
            for r in refs {
                if first_visit || matches!(r, Reference::Value { .. }) {
                    self.enqueue_reference(r, work.node);
                }
            }
        }
    }
}
