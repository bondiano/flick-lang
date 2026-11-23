//! Satisfiability of `@when` boolean conditions against a backend's valid
//! tag assignments (always-on base tags, selectable axes, built-in
//! environments, and any loaded `[host]` packages).

use std::collections::BTreeSet;

use super::{BackendAxis, BackendHost, BackendTags};
use crate::syntax::when::{self, Expr};

/// Upper bound on branch evaluations per backend so pathological expressions
/// with many free axis tags cannot cause exponential blowup.
const MAX_EVALS: usize = 4096;

enum AxisPlan<'a> {
    /// Choose one option (`None` represents selecting no tracked tag).
    Exclusive(Vec<Option<&'a str>>),
    /// Choose any subset of `tags`; when `must_be_non_empty` is true, the
    /// empty subset is excluded because `axis.default` is tracked and no
    /// unmentioned tag exists to prevent default fallback.
    Subset {
        tags: Vec<&'a str>,
        must_be_non_empty: bool,
    },
}

struct SearchCtx<'a, 'b> {
    backend: &'a BackendTags,
    parsed_hosts: &'b [(&'a [String], Option<Expr>)],
    relevant: &'b BTreeSet<&'a str>,
    expr: &'b Expr,
}

/// Whether `backend` has at least one valid tag assignment (always-on base
/// tags, valid selections on its axes, and one built-in or loaded `[host]`
/// environment) under which `expr` evaluates to `true`.
pub(super) fn backend_can_satisfy(
    backend: &BackendTags,
    extra_hosts: &[BackendHost],
    expr: &Expr,
) -> bool {
    let base: BTreeSet<&str> = backend.tags.iter().map(String::as_str).collect();
    let parsed_hosts: Vec<(&[String], Option<Expr>)> = extra_hosts
        .iter()
        .map(|h| {
            let req = h.requires.as_deref().and_then(|c| when::parse(c).ok());
            (h.tags.as_slice(), req)
        })
        .collect();

    let mut relevant: BTreeSet<&str> = expr.tags().into_iter().map(|(t, _)| t).collect();
    for (_, req) in &parsed_hosts {
        if let Some(req_expr) = req {
            relevant.extend(req_expr.tags().into_iter().map(|(t, _)| t));
        }
    }

    let plans: Vec<AxisPlan<'_>> = backend
        .axes
        .iter()
        .map(|axis| plan_axis(axis, &base, &relevant))
        .collect();
    let ctx = SearchCtx {
        backend,
        parsed_hosts: &parsed_hosts,
        relevant: &relevant,
        expr,
    };

    let mut active = base;
    let mut evals = 0usize;
    search_axes(0, &plans, &ctx, &mut active, &mut evals)
}

fn plan_axis<'a>(
    axis: &'a BackendAxis,
    base: &BTreeSet<&str>,
    relevant: &BTreeSet<&str>,
) -> AxisPlan<'a> {
    let mut tags: Vec<&'a str> = Vec::new();
    let mut has_other = false;
    for tag in &axis.tags {
        let t = tag.as_str();
        if relevant.contains(t) && !base.contains(t) {
            if !tags.contains(&t) {
                tags.push(t);
            }
        } else {
            has_other = true;
        }
    }
    if axis.exclusive {
        let mut choices: Vec<Option<&'a str>> = tags.into_iter().map(Some).collect();
        if axis.default.is_none() || has_other || axis.tags.is_empty() {
            choices.push(None);
        }
        AxisPlan::Exclusive(choices)
    } else {
        let default_is_tracked = axis.default.as_deref().is_some_and(|d| tags.contains(&d));
        AxisPlan::Subset {
            tags,
            must_be_non_empty: default_is_tracked && !has_other,
        }
    }
}

fn search_axes<'a>(
    axis_idx: usize,
    plans: &[AxisPlan<'a>],
    ctx: &SearchCtx<'a, '_>,
    active: &mut BTreeSet<&'a str>,
    evals: &mut usize,
) -> bool {
    if *evals >= MAX_EVALS {
        return false;
    }
    let Some(plan) = plans.get(axis_idx) else {
        return try_hosts(ctx, active, evals);
    };
    match plan {
        AxisPlan::Exclusive(choices) => {
            for choice in choices {
                let inserted = choice.is_some_and(|tag| active.insert(tag));
                let ok = search_axes(axis_idx + 1, plans, ctx, active, evals);
                if inserted && let Some(tag) = choice {
                    active.remove(tag);
                }
                if ok {
                    return true;
                }
            }
            false
        }
        AxisPlan::Subset {
            tags,
            must_be_non_empty,
        } => search_subset(
            axis_idx,
            0,
            tags,
            *must_be_non_empty,
            false,
            plans,
            ctx,
            active,
            evals,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn search_subset<'a>(
    axis_idx: usize,
    tag_idx: usize,
    tags: &[&'a str],
    must_be_non_empty: bool,
    picked_any: bool,
    plans: &[AxisPlan<'a>],
    ctx: &SearchCtx<'a, '_>,
    active: &mut BTreeSet<&'a str>,
    evals: &mut usize,
) -> bool {
    if *evals >= MAX_EVALS {
        return false;
    }
    if tag_idx == tags.len() {
        if must_be_non_empty && !picked_any {
            return false;
        }
        return search_axes(axis_idx + 1, plans, ctx, active, evals);
    }
    if search_subset(
        axis_idx,
        tag_idx + 1,
        tags,
        must_be_non_empty,
        picked_any,
        plans,
        ctx,
        active,
        evals,
    ) {
        return true;
    }
    let inserted = active.insert(tags[tag_idx]);
    let ok = search_subset(
        axis_idx,
        tag_idx + 1,
        tags,
        must_be_non_empty,
        true,
        plans,
        ctx,
        active,
        evals,
    );
    if inserted {
        active.remove(tags[tag_idx]);
    }
    ok
}

fn try_hosts<'a>(
    ctx: &SearchCtx<'a, '_>,
    active: &mut BTreeSet<&'a str>,
    evals: &mut usize,
) -> bool {
    let mut host_sets: Vec<Vec<&'a str>> = Vec::new();
    for (_, platform) in &ctx.backend.environments {
        let mut set = Vec::new();
        let p = platform.as_str();
        if ctx.relevant.contains(p) {
            set.push(p);
        }
        if !host_sets.contains(&set) {
            host_sets.push(set);
        }
    }
    for (tags, req) in ctx.parsed_hosts {
        if let Some(req_expr) = req
            && !req_expr.eval(&|t| active.contains(t))
        {
            continue;
        }
        let mut set: Vec<&'a str> = tags
            .iter()
            .map(String::as_str)
            .filter(|t| ctx.relevant.contains(t))
            .collect();
        set.sort_unstable();
        set.dedup();
        if !host_sets.contains(&set) {
            host_sets.push(set);
        }
    }
    if host_sets.is_empty() {
        host_sets.push(Vec::new());
    }
    for host_tags in host_sets {
        *evals += 1;
        let mut added = Vec::new();
        for tag in host_tags {
            if active.insert(tag) {
                added.push(tag);
            }
        }
        let ok = ctx.expr.eval(&|t| active.contains(t));
        for tag in added {
            active.remove(tag);
        }
        if ok {
            return true;
        }
        if *evals >= MAX_EVALS {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::project::environment::{EnvironmentContext, ResolvedEnvironment, official_backends};
    use crate::syntax::foreign_meta::ForeignMeta;

    fn sat(backend: &BackendTags, hosts: &[BackendHost], cond: &str) -> bool {
        let expr = when::parse(cond).expect("valid cond");
        backend_can_satisfy(backend, hosts, &expr)
    }

    #[test]
    fn official_backends_satisfy_negated_platform_and_axis_conditions() {
        let backends = official_backends();
        let js = backends.iter().find(|b| b.id == "flick/js").expect("js");
        let go = backends.iter().find(|b| b.id == "flick/go").expect("go");

        assert!(sat(js, &[], "js && !browser"));
        assert!(!sat(go, &[], "js && !browser"));

        assert!(sat(js, &[], "js && !rt"));
        assert!(sat(js, &[], "js && !effect"));
        assert!(!sat(js, &[], "js && !rt && !effect"));
        assert!(!sat(go, &[], "js && !rt"));

        assert!(sat(js, &[], "js && !effect-adapter"));
        assert!(sat(js, &[], "js && effect-adapter"));

        assert!(sat(go, &[], "go && !cgo"));
        assert!(sat(go, &[], "go && cgo"));
        assert!(!sat(js, &[], "go && !cgo"));

        // `native` is Go's sole built-in host unless a custom host package is loaded.
        assert!(!sat(go, &[], "go && !native"));
        let custom_host = vec![BackendHost {
            tags: vec!["wasm".into()],
            requires: None,
        }];
        assert!(sat(go, &custom_host, "go && !native"));

        // Both backends can satisfy `!browser` alone.
        assert!(sat(js, &[], "!browser"));
        assert!(sat(go, &[], "!browser"));
    }

    #[test]
    fn custom_host_requires_is_respected_during_satisfiability() {
        let backends = official_backends();
        let js = backends.iter().find(|b| b.id == "flick/js").expect("js");
        let hosts = vec![BackendHost {
            tags: vec!["deno".into(), "server".into()],
            requires: Some("effect".into()),
        }];
        assert!(sat(js, &hosts, "js && deno && effect"));
        assert!(!sat(js, &hosts, "js && deno && rt"));
    }

    #[test]
    fn foreign_meta_for_selects_js_for_negated_js_conditions_in_go_and_standalone() {
        let backends = official_backends();
        let go_ctx = EnvironmentContext {
            vocabulary: BTreeSet::new(),
            validate_tags: true,
            resolved: Some(ResolvedEnvironment {
                backend_id: "flick/go".into(),
                backend: "go".into(),
                platform: "native".into(),
                host_package: None,
                tags: BTreeSet::from(["go".into(), "native".into()]),
                explicit_environment: false,
                ui: None,
            }),
            foreign_meta: ForeignMeta::go(),
            validate_foreign_meta: true,
            backends: backends.clone(),
            hosts: BTreeMap::new(),
        };
        assert_eq!(
            go_ctx
                .foreign_meta_for(None, Some("js && !browser"))
                .backend_id,
            "flick/js"
        );
        assert_eq!(
            go_ctx.foreign_meta_for(None, Some("js && !rt")).backend_id,
            "flick/js"
        );
        assert_eq!(
            go_ctx.foreign_meta_for(None, Some("!browser")).backend_id,
            "flick/go"
        );

        let standalone = EnvironmentContext::standalone_js();
        assert_eq!(
            standalone
                .foreign_meta_for(None, Some("js && !browser"))
                .backend_id,
            "flick/js"
        );
        assert_eq!(
            standalone
                .foreign_meta_for(None, Some("go && !cgo"))
                .backend_id,
            "flick/go"
        );
    }
}
