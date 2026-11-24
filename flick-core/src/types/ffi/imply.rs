//! Finite environment tag worlds, implication over `#663` tag vocabularies,
//! and `@native` override selection.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::spec::ForeignBackendSpec;
use crate::syntax::when;

/// One tag axis in a backend's finite environment vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AxisTagModel {
    pub tags: Vec<String>,
    pub default: Option<String>,
    pub exclusive: bool,
}

/// Tag vocabulary and foreign adapter spec of one known backend.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BackendTagModel {
    pub id: String,
    pub short: String,
    pub tags: Vec<String>,
    pub axes: Vec<AxisTagModel>,
    pub builtin_hosts: Vec<String>,
    pub foreign_spec: ForeignBackendSpec,
}

impl BackendTagModel {
    fn matches_name(&self, name: &str) -> bool {
        self.short == name || self.id == name || self.tags.iter().any(|t| t == name)
    }
}

/// A declared `[host]` package's tag contribution and backend requirement.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HostTagModel {
    pub name: String,
    pub backend: String,
    pub requires: Option<String>,
    pub tags: Vec<String>,
}

/// Environment vocabulary, world model, and active selection state for `@native`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

impl NativeOverrideEnv {
    /// Default standalone environment (`flick/js` + `flick/go` vocabulary, active `js/rt/node`).
    pub fn default_js() -> &'static Self {
        static DEFAULT: OnceLock<NativeOverrideEnv> = OnceLock::new();
        DEFAULT.get_or_init(Self::official_js)
    }

    /// Builds an environment with the official `flick/js` and `flick/go` models
    /// and default active tags `{js, rt, node}`.
    pub fn official_js() -> Self {
        let backends = vec![
            BackendTagModel {
                id: "flick/js".into(),
                short: "js".into(),
                tags: vec!["js".into()],
                axes: vec![
                    AxisTagModel {
                        tags: vec!["rt".into(), "effect".into()],
                        default: Some("rt".into()),
                        exclusive: true,
                    },
                    AxisTagModel {
                        tags: vec!["effect-adapter".into()],
                        default: None,
                        exclusive: false,
                    },
                ],
                builtin_hosts: vec![
                    "node".into(),
                    "browser".into(),
                    "bun".into(),
                    "cloudflare".into(),
                ],
                foreign_spec: ForeignBackendSpec::js(),
            },
            BackendTagModel {
                id: "flick/go".into(),
                short: "go".into(),
                tags: vec!["go".into()],
                axes: vec![AxisTagModel {
                    tags: vec!["cgo".into()],
                    default: None,
                    exclusive: false,
                }],
                builtin_hosts: vec!["native".into()],
                foreign_spec: ForeignBackendSpec::go(),
            },
        ];
        let mut vocabulary = BTreeSet::new();
        for b in &backends {
            vocabulary.extend(b.tags.iter().cloned());
            for axis in &b.axes {
                vocabulary.extend(axis.tags.iter().cloned());
            }
            vocabulary.extend(b.builtin_hosts.iter().cloned());
        }
        let active_tags = ["js", "rt", "node"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        Self {
            vocabulary,
            validate_tags: true,
            backends,
            hosts: Vec::new(),
            active_tags: Some(active_tags),
            active_backend_id: Some("flick/js".into()),
            disable_native_overrides: false,
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
        let mut relevant = BTreeSet::new();
        for (tag, _) in a.tags().into_iter().chain(b.tags()) {
            relevant.insert(tag.to_owned());
        }
        let mut holds = true;
        self.for_each_world(&relevant, &mut |world| {
            if a.eval(&|t| world.contains(t)) && !b.eval(&|t| world.contains(t)) {
                holds = false;
                return false;
            }
            true
        });
        holds
    }

    /// Selects the unique strictly most-specific matching override among `conds`,
    /// or reports the conflicting pair when no unique strict winner exists.
    pub fn select(&self, conds: &[when::Expr]) -> OverrideSelection {
        let Some(active) = &self.active_tags else {
            return OverrideSelection::None;
        };
        let matching: Vec<usize> = conds
            .iter()
            .enumerate()
            .filter_map(|(i, expr)| expr.eval(&|t| active.contains(t)).then_some(i))
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
        let mut relevant = BTreeSet::new();
        for (tag, _) in cond.tags() {
            relevant.insert(tag.to_owned());
        }
        let mut found: Option<&ForeignBackendSpec> = None;
        for backend in &self.backends {
            let mut matched = false;
            self.for_each_backend_world(backend, &relevant, &mut |world| {
                if cond.eval(&|t| world.contains(t)) {
                    matched = true;
                    return false;
                }
                true
            });
            if matched {
                if found.is_some() {
                    return None;
                }
                found = Some(&backend.foreign_spec);
            }
        }
        found
    }

    fn for_each_world(
        &self,
        relevant: &BTreeSet<String>,
        visit: &mut impl FnMut(&BTreeSet<String>) -> bool,
    ) {
        for backend in &self.backends {
            if !self.for_each_backend_world(backend, relevant, visit) {
                return;
            }
        }
    }

    fn for_each_backend_world(
        &self,
        backend: &BackendTagModel,
        relevant: &BTreeSet<String>,
        visit: &mut impl FnMut(&BTreeSet<String>) -> bool,
    ) -> bool {
        // Collect all hosts targeting this backend (declared hosts override built-in hosts of the same name).
        let mut host_map: BTreeMap<String, (Option<when::Expr>, Vec<String>)> = BTreeMap::new();
        for builtin in &backend.builtin_hosts {
            host_map.insert(builtin.clone(), (None, vec![builtin.clone()]));
        }
        for host in &self.hosts {
            if backend.matches_name(&host.backend) {
                let req = host.requires.as_deref().and_then(|s| when::parse(s).ok());
                let mut htags = Vec::with_capacity(1 + host.tags.len());
                htags.push(host.name.clone());
                htags.extend(host.tags.iter().cloned());
                host_map.insert(host.name.clone(), (req, htags));
            }
        }
        let hosts: Vec<(Option<when::Expr>, Vec<String>)> = host_map.into_values().collect();
        // Tags referenced in host `requires` are also relevant for non-exclusive axes.
        let mut axis_relevant = relevant.clone();
        for (req, _) in &hosts {
            if let Some(expr) = req {
                for (tag, _) in expr.tags() {
                    axis_relevant.insert(tag.to_owned());
                }
            }
        }

        // Build per-axis choices.
        let mut axis_choices: Vec<Vec<Vec<String>>> = Vec::with_capacity(backend.axes.len());
        for axis in &backend.axes {
            if axis.exclusive {
                let mut choices: Vec<Vec<String>> =
                    axis.tags.iter().map(|t| vec![t.clone()]).collect();
                if axis.default.is_none() || choices.is_empty() {
                    choices.push(Vec::new());
                }
                axis_choices.push(choices);
            } else {
                let active_tags: Vec<String> = axis
                    .tags
                    .iter()
                    .filter(|t| axis_relevant.contains(t.as_str()))
                    .cloned()
                    .collect();
                let subsets = non_exclusive_subsets(&active_tags, axis.default.as_deref());
                axis_choices.push(subsets);
            }
        }

        let mut base = BTreeSet::new();
        base.extend(backend.tags.iter().cloned());
        enumerate_axes(&axis_choices, 0, &mut base, &hosts, visit)
    }
}

fn non_exclusive_subsets(tags: &[String], default: Option<&str>) -> Vec<Vec<String>> {
    // Cap exponential fan-out defensively while remaining exact for all practical axes.
    let count = tags.len().min(12);
    let total = 1usize << count;
    let mut out = Vec::with_capacity(total);
    for mask in 0..total {
        let mut subset = Vec::new();
        for (i, tag) in tags[..count].iter().enumerate() {
            if (mask >> i) & 1 == 1 {
                subset.push(tag.clone());
            }
        }
        if subset.is_empty()
            && let Some(def) = default
        {
            subset.push(def.to_owned());
        }
        if !out.contains(&subset) {
            out.push(subset);
        }
    }
    out
}

fn enumerate_axes(
    axes: &[Vec<Vec<String>>],
    index: usize,
    current: &mut BTreeSet<String>,
    hosts: &[(Option<when::Expr>, Vec<String>)],
    visit: &mut impl FnMut(&BTreeSet<String>) -> bool,
) -> bool {
    if index == axes.len() {
        if hosts.is_empty() {
            return visit(current);
        }
        let mut any_host = false;
        for (req, host_tags) in hosts {
            if req
                .as_ref()
                .is_some_and(|expr| !expr.eval(&|t| current.contains(t)))
            {
                continue;
            }
            any_host = true;
            let mut world = current.clone();
            world.extend(host_tags.iter().cloned());
            if !visit(&world) {
                return false;
            }
        }
        if !any_host {
            return visit(current);
        }
        return true;
    }
    for choice in &axes[index] {
        let mut added = Vec::new();
        for tag in choice {
            if current.insert(tag.clone()) {
                added.push(tag.clone());
            }
        }
        let cont = enumerate_axes(axes, index + 1, current, hosts, visit);
        for tag in added {
            current.remove(&tag);
        }
        if !cont {
            return false;
        }
    }
    true
}

#[cfg(test)]
#[path = "imply_tests.rs"]
mod tests;
