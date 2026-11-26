//! Finite environment tag worlds, implication over `#663` tag vocabularies,
//! and `@native` override selection.

use std::collections::BTreeSet;
use std::sync::OnceLock;

#[cfg(feature = "native-project")]
use serde::{Deserialize, Serialize};

use super::spec::ForeignBackendSpec;
use crate::syntax::when;

/// One tag axis in a backend's finite environment vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(Serialize, Deserialize))]
pub struct AxisTagModel {
    pub tags: Vec<String>,
    pub default: Option<String>,
    pub exclusive: bool,
}

/// Tag vocabulary and foreign adapter spec of one known backend.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(Serialize, Deserialize))]
pub struct BackendTagModel {
    pub id: String,
    pub short: String,
    pub tags: Vec<String>,
    pub axes: Vec<AxisTagModel>,
    pub builtin_hosts: Vec<String>,
    pub foreign_spec: ForeignBackendSpec,
}

#[cfg(not(target_arch = "wasm32"))]
impl BackendTagModel {
    fn matches_name(&self, name: &str) -> bool {
        self.short == name || self.id == name || self.tags.iter().any(|t| t == name)
    }
}

/// A declared `[host]` package's tag contribution and backend requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(Serialize, Deserialize))]
pub struct HostTagModel {
    pub name: String,
    pub backend: String,
    pub requires: Option<String>,
    pub tags: Vec<String>,
}

/// Environment vocabulary, world model, and active selection state for `@native`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(Serialize, Deserialize))]
pub struct NativeOverrideEnv {
    /// All known environment tags (backends, axes, built-in hosts, host packages).
    pub vocabulary: BTreeSet<String>,
    /// False only when an uncached custom backend prevented loading the full vocabulary.
    pub validate_tags: bool,
    /// Known backends with their axes, built-in hosts, and foreign adapter specs.
    pub backends: Vec<BackendTagModel>,
    /// Declared `[host]` packages contributing host tags and `requires` constraints.
    pub hosts: Vec<HostTagModel>,
    /// Concrete active environment tags (`None` when unresolved / uncached custom backend).
    pub active_tags: Option<BTreeSet<String>>,
    /// Active backend id, when known.
    pub active_backend_id: Option<String>,
    /// When true (`--no-native-overrides`), checking still validates overrides, but
    /// lowering emits the Flick fallback body instead of the native override.
    pub disable_native_overrides: bool,
    /// Whether checking records every clause, not only the selected one: the
    /// built-in modules are checked and lowered once for every environment,
    /// and each build selects among their clauses again.
    #[cfg_attr(feature = "native-project", serde(default))]
    pub every_clause: bool,
}

/// Result of selecting among `@native` override conditions in the active environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideSelection {
    /// No override condition matches the active environment; use the Flick body.
    None,
    /// One matching override strictly implies every other matching override.
    Selected(usize),
    /// Two most-specific matching overrides where neither strictly implies the other.
    Ambiguous(usize, usize),
}

type BuiltinAxis = (&'static [&'static str], Option<&'static str>, bool);
type BuiltinBackend = (
    &'static str,
    &'static str,
    &'static [BuiltinAxis],
    &'static [&'static str],
);

const OFFICIAL_MODELS: &[BuiltinBackend] = &[
    (
        "flick/js",
        "js",
        &[
            (&["rt", "effect"], Some("rt"), true),
            (&["effect-adapter"], None, false),
        ],
        &["node", "browser", "bun", "cloudflare"],
    ),
    ("flick/go", "go", &[(&["cgo"], None, false)], &["native"]),
    ("flick/vm", "vm", &[], &["native"]),
];

fn owned(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn extend_vocab(vocab: &mut BTreeSet<String>, items: &[&str]) {
    for &t in items {
        vocab.insert(t.to_owned());
    }
}

fn eval_world(expr: &when::Expr, world: &[&str]) -> bool {
    expr.eval(&|t| world.contains(&t))
}

impl NativeOverrideEnv {
    /// Default standalone environment (`flick/js` + `flick/go` vocabulary, active `js/rt/node`).
    pub fn default_js() -> &'static Self {
        static DEFAULT: OnceLock<NativeOverrideEnv> = OnceLock::new();
        DEFAULT.get_or_init(Self::official_js)
    }

    /// The environment the built-in modules are checked in: the default one,
    /// whose selection their shared IR lowers, recording every clause.
    pub fn builtins() -> &'static Self {
        static BUILTINS: OnceLock<NativeOverrideEnv> = OnceLock::new();
        BUILTINS.get_or_init(|| Self {
            every_clause: true,
            ..Self::official_js()
        })
    }

    /// Builds an environment with the official `flick/js` and `flick/go` models
    /// and default active tags `{js, rt, node}`.
    pub fn official_js() -> Self {
        let mut backends = Vec::with_capacity(OFFICIAL_MODELS.len());
        let mut vocabulary = BTreeSet::new();
        for &(id, short, axes, builtin_hosts) in OFFICIAL_MODELS {
            extend_vocab(&mut vocabulary, &[short]);
            let mut axis_models = Vec::with_capacity(axes.len());
            for &(tags, default, exclusive) in axes {
                extend_vocab(&mut vocabulary, tags);
                axis_models.push(AxisTagModel {
                    tags: owned(tags),
                    default: default.map(str::to_owned),
                    exclusive,
                });
            }
            extend_vocab(&mut vocabulary, builtin_hosts);
            let foreign_spec = match short {
                "go" => ForeignBackendSpec::go(),
                "vm" => ForeignBackendSpec::vm(),
                _ => ForeignBackendSpec::js(),
            };
            backends.push(BackendTagModel {
                id: id.to_owned(),
                short: short.to_owned(),
                tags: vec![short.to_owned()],
                axes: axis_models,
                builtin_hosts: owned(builtin_hosts),
                foreign_spec,
            });
        }
        let mut active_tags = BTreeSet::new();
        extend_vocab(&mut active_tags, &["js", "rt", "node"]);
        Self {
            vocabulary,
            validate_tags: true,
            backends,
            hosts: Vec::new(),
            active_tags: Some(active_tags),
            active_backend_id: Some("flick/js".to_owned()),
            disable_native_overrides: false,
            every_clause: false,
        }
    }

    /// Returns the active backend's foreign adapter specification, falling back to `fallback`.
    pub fn active_backend_spec<'a>(
        &'a self,
        fallback: &'a ForeignBackendSpec,
    ) -> &'a ForeignBackendSpec {
        self.active_backend_id
            .as_deref()
            .and_then(|id| self.backends.iter().find(|b| b.id == id))
            .map_or(fallback, |b| &b.foreign_spec)
    }

    /// Whether `a` implies `b` over every valid environment world in this model.
    pub fn implies(&self, a: &when::Expr, b: &when::Expr) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        let relevant = {
            let mut relevant = Vec::new();
            for (tag, _) in a.tags().into_iter().chain(b.tags()) {
                if !relevant.contains(&tag) {
                    relevant.push(tag);
                }
            }
            relevant
        };
        #[cfg(target_arch = "wasm32")]
        let relevant: [&str; 0] = [];
        let mut holds = true;
        for backend in &self.backends {
            let mut check = |world: &[&str]| {
                if eval_world(a, world) && !eval_world(b, world) {
                    holds = false;
                    return false;
                }
                true
            };
            if !self.for_each_backend_world(backend, &relevant, &mut check) {
                break;
            }
        }
        holds
    }

    /// Selects the unique strictly most-specific matching override among `conds`,
    /// or reports the conflicting pair when no unique strict winner exists.
    pub fn select(&self, conds: &[when::Expr]) -> OverrideSelection {
        let Some(active) = &self.active_tags else {
            return OverrideSelection::None;
        };
        let active_slice: Vec<&str> = active.iter().map(String::as_str).collect();
        let matching: Vec<usize> = conds
            .iter()
            .enumerate()
            .filter_map(|(i, expr)| eval_world(expr, &active_slice).then_some(i))
            .collect();
        match matching.as_slice() {
            [] => OverrideSelection::None,
            [only] => OverrideSelection::Selected(*only),
            many => {
                // Strictly more specific: `i` implies `j` and `j` does not imply `i`.
                let mut minimal = Vec::new();
                for &cand in many {
                    let dominated = many.iter().any(|&other| {
                        other != cand
                            && self.implies(&conds[other], &conds[cand])
                            && !self.implies(&conds[cand], &conds[other])
                    });
                    if !dominated {
                        minimal.push(cand);
                    }
                }
                if let [winner] = minimal.as_slice() {
                    OverrideSelection::Selected(*winner)
                } else {
                    OverrideSelection::Ambiguous(minimal[0], minimal[1])
                }
            }
        }
    }

    /// If `cond` is satisfiable in valid worlds of exactly one known backend,
    /// returns that backend's foreign adapter specification.
    pub fn unique_backend_for(&self, cond: &when::Expr) -> Option<&ForeignBackendSpec> {
        #[cfg(not(target_arch = "wasm32"))]
        let relevant = {
            let mut relevant = Vec::new();
            for (tag, _) in cond.tags() {
                if !relevant.contains(&tag) {
                    relevant.push(tag);
                }
            }
            relevant
        };
        #[cfg(target_arch = "wasm32")]
        let relevant: [&str; 0] = [];
        let mut found: Option<&ForeignBackendSpec> = None;
        for backend in &self.backends {
            let mut matched = false;
            let mut probe = |world: &[&str]| {
                if eval_world(cond, world) {
                    matched = true;
                    return false;
                }
                true
            };
            self.for_each_backend_world(backend, &relevant, &mut probe);
            if matched {
                if found.is_some() {
                    return None;
                }
                found = Some(&backend.foreign_spec);
            }
        }
        found
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
    fn for_each_backend_world(
        &self,
        backend: &BackendTagModel,
        relevant: &[&str],
        visit: &mut dyn FnMut(&[&str]) -> bool,
    ) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = relevant;
            if backend.short == "js" {
                for rt in ["rt", "effect"] {
                    for host in ["node", "browser", "bun", "cloudflare"] {
                        if !visit(&["js", rt, host]) || !visit(&["js", rt, "effect-adapter", host])
                        {
                            return false;
                        }
                    }
                }
                true
            } else if backend.short == "vm" {
                visit(&["vm", "native"])
            } else {
                visit(&["go", "native"]) && visit(&["go", "cgo", "native"])
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Collect all hosts targeting this backend (declared hosts override built-ins of the same name).
            let mut hosts: Vec<(&str, Option<when::Expr>, Vec<&str>)> = backend
                .builtin_hosts
                .iter()
                .map(|b| (b.as_str(), None, vec![b.as_str()]))
                .collect();
            for host in &self.hosts {
                if backend.matches_name(&host.backend) {
                    let req = host.requires.as_deref().and_then(|s| when::parse(s).ok());
                    let mut htags = Vec::with_capacity(1 + host.tags.len());
                    htags.push(host.name.as_str());
                    htags.extend(host.tags.iter().map(String::as_str));
                    if let Some(slot) = hosts.iter_mut().find(|(n, _, _)| *n == host.name) {
                        *slot = (host.name.as_str(), req, htags);
                    } else {
                        hosts.push((host.name.as_str(), req, htags));
                    }
                }
            }
            // Tags referenced in host `requires` are also relevant for non-exclusive axes.
            let mut axis_relevant: Vec<&str> = relevant.to_vec();
            for (_, req, _) in &hosts {
                if let Some(expr) = req {
                    for (tag, _) in expr.tags() {
                        if !axis_relevant.contains(&tag) {
                            axis_relevant.push(tag);
                        }
                    }
                }
            }
            let mut world: Vec<&str> = backend.tags.iter().map(String::as_str).collect();
            enumerate_axes(&backend.axes, 0, &axis_relevant, &mut world, &hosts, visit)
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn enumerate_axes<'a>(
    axes: &'a [AxisTagModel],
    index: usize,
    relevant: &[&str],
    world: &mut Vec<&'a str>,
    hosts: &[(&'a str, Option<when::Expr>, Vec<&'a str>)],
    visit: &mut dyn FnMut(&[&str]) -> bool,
) -> bool {
    if index == axes.len() {
        if hosts.is_empty() {
            return visit(world);
        }
        let mut any_host = false;
        for (_, req, host_tags) in hosts {
            if req.as_ref().is_some_and(|expr| !eval_world(expr, world)) {
                continue;
            }
            any_host = true;
            let mark = world.len();
            world.extend_from_slice(host_tags);
            let keep_going = visit(world);
            world.truncate(mark);
            if !keep_going {
                return false;
            }
        }
        if !any_host {
            return visit(world);
        }
        return true;
    }
    let current = &axes[index];
    let mark = world.len();
    if current.exclusive {
        for tag in &current.tags {
            world.push(tag.as_str());
            let keep_going = enumerate_axes(axes, index + 1, relevant, world, hosts, visit);
            world.truncate(mark);
            if !keep_going {
                return false;
            }
        }
        if current.default.is_none() || current.tags.is_empty() {
            return enumerate_axes(axes, index + 1, relevant, world, hosts, visit);
        }
        return true;
    }
    let active: Vec<&'a str> = current
        .tags
        .iter()
        .map(String::as_str)
        .filter(|t| relevant.contains(t))
        .collect();
    // Cap exponential fan-out defensively while remaining exact for all practical axes.
    let bits = active.len().min(12);
    let total = 1usize << bits;
    for mask in 0..total {
        for (i, &tag) in active[..bits].iter().enumerate() {
            if (mask >> i) & 1 == 1 {
                world.push(tag);
            }
        }
        if world.len() == mark
            && let Some(def) = current.default.as_deref()
        {
            world.push(def);
        }
        let keep_going = enumerate_axes(axes, index + 1, relevant, world, hosts, visit);
        world.truncate(mark);
        if !keep_going {
            return false;
        }
    }
    true
}

#[cfg(test)]
#[path = "imply_tests.rs"]
mod tests;
