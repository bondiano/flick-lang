use std::collections::BTreeSet;

use super::*;
use crate::syntax::when;

fn parse(cond: &str) -> when::Expr {
    when::parse(cond).unwrap_or_else(|e| panic!("failed to parse `{cond}`: {e}"))
}

fn set(tags: &[&str]) -> BTreeSet<String> {
    tags.iter().map(|s| (*s).to_owned()).collect()
}

fn custom_env(active: &[&str]) -> NativeOverrideEnv {
    let mut env = NativeOverrideEnv::official_js();
    env.backends.push(BackendTagModel {
        id: "test/literal".into(),
        short: "literal".into(),
        tags: vec!["literal".into()],
        axes: vec![
            AxisTagModel {
                tags: vec!["alpha".into(), "beta".into()],
                default: Some("alpha".into()),
                exclusive: true,
            },
            AxisTagModel {
                tags: vec!["jit".into(), "ffi".into()],
                default: None,
                exclusive: false,
            },
        ],
        builtin_hosts: vec!["default".into()],
        foreign_spec: ForeignBackendSpec {
            backend_id: "test/literal".into(),
            adapters: Vec::new(),
            type_form_attributes: Vec::new(),
        },
    });
    env.hosts.push(HostTagModel {
        name: "alpha-host".into(),
        backend: "literal".into(),
        requires: Some("alpha".into()),
        tags: vec!["fast-math".into()],
    });
    env.active_tags = Some(set(active));
    env
}

#[test]
fn host_implies_backend_and_conjunction_implies_component() {
    let env = NativeOverrideEnv::official_js();
    assert!(env.implies(&parse("bun"), &parse("js")));
    assert!(!env.implies(&parse("js"), &parse("bun")));
    assert!(env.implies(&parse("node"), &parse("js")));
    assert!(env.implies(&parse("js && rt"), &parse("js")));
    assert!(env.implies(&parse("cgo"), &parse("go")));
    assert!(!env.implies(&parse("go"), &parse("cgo")));
    assert!(env.implies(&parse("node && rt"), &parse("node")));
    assert!(env.implies(&parse("node && rt"), &parse("js")));
}

#[test]
fn select_picks_most_specific_override() {
    let mut env = NativeOverrideEnv::official_js();
    env.active_tags = Some(set(&["js", "rt", "bun"]));
    let conds = vec![parse("js"), parse("bun"), parse("go")];
    assert_eq!(env.select(&conds), OverrideSelection::Selected(1));

    let chain = vec![parse("js"), parse("bun && rt"), parse("bun")];
    assert_eq!(env.select(&chain), OverrideSelection::Selected(1));
}

#[test]
fn select_rejects_incomparable_and_equivalent_pairs() {
    let env = NativeOverrideEnv::official_js();
    // Active: js, rt, node. Neither `js && node` nor `js && rt` implies the other.
    let incomparable = vec![parse("js && node"), parse("js && rt")];
    assert_eq!(
        env.select(&incomparable),
        OverrideSelection::Ambiguous(0, 1)
    );

    // Equivalent conditions (`bun` <=> `js && bun`) are ambiguous.
    let mut bun_env = NativeOverrideEnv::official_js();
    bun_env.active_tags = Some(set(&["js", "rt", "bun"]));
    let equiv = vec![parse("bun"), parse("js && bun")];
    assert_eq!(bun_env.select(&equiv), OverrideSelection::Ambiguous(0, 1));

    // Exhaustive exclusive axis disjunction (`(js && rt) || (js && effect)` <=> `js`) is equivalent.
    let exhaustive = vec![parse("js"), parse("(js && rt) || (js && effect)")];
    assert_eq!(env.select(&exhaustive), OverrideSelection::Ambiguous(0, 1));
}

#[test]
fn custom_backend_axes_and_host_requires_implication() {
    let env = custom_env(&["literal", "alpha", "jit", "alpha-host", "fast-math"]);
    // `alpha-host` requires `alpha` on `literal`, so `alpha-host => literal && alpha => literal`.
    assert!(env.implies(&parse("alpha-host"), &parse("literal && alpha")));
    assert!(env.implies(&parse("alpha-host"), &parse("literal")));
    assert!(env.implies(&parse("alpha-host"), &parse("fast-math")));
    assert!(!env.implies(&parse("literal && alpha"), &parse("alpha-host")));

    let ordered = vec![
        parse("literal"),
        parse("literal && alpha"),
        parse("alpha-host"),
    ];
    assert_eq!(env.select(&ordered), OverrideSelection::Selected(2));

    let both_axes = custom_env(&["literal", "alpha", "jit", "ffi", "default"]);
    let ambig = vec![
        parse("literal && jit"),
        parse("literal && ffi"),
        parse("literal"),
    ];
    assert_eq!(both_axes.select(&ambig), OverrideSelection::Ambiguous(0, 1));
}

#[test]
fn select_returns_none_when_no_override_matches() {
    let env = custom_env(&["literal", "alpha", "default"]);
    let conds = vec![parse("js"), parse("go")];
    assert_eq!(env.select(&conds), OverrideSelection::None);
}

#[test]
fn unique_backend_identifies_target_for_inactive_clauses() {
    let env = NativeOverrideEnv::official_js();
    assert_eq!(
        env.unique_backend_for(&parse("go"))
            .map(|s| s.backend_id.as_str()),
        Some("flick/go")
    );
    assert_eq!(
        env.unique_backend_for(&parse("bun"))
            .map(|s| s.backend_id.as_str()),
        Some("flick/js")
    );
    // `!browser` is satisfiable on both `js/node` and `go/native`.
    assert_eq!(env.unique_backend_for(&parse("!browser")), None);
}
