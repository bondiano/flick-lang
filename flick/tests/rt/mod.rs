//! The cases of tests/run on Flick's own runtime.

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use super::{assert_go, jobs, listed_cases, parallel, portable, run_case, run_case_with, trace};

/// The cases of `tests/run/rt.txt` on Flick's own runtime, in both profiles,
/// each trace checked by the monitor. Its own test: no other run builds for it.
#[test]
#[ignore = "needs Node and `pnpm install`; run `just conformance-rt`"]
fn rt_cases() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let root = repo.join("tests/run");
    let only = env::var("FLICK_E2E_ONLY").ok();
    let cases: Vec<PathBuf> = listed_cases(&root, "rt.txt")
        .into_iter()
        .filter(|c| {
            only.as_ref()
                .is_none_or(|o| o.split(',').any(|p| c.contains(p)))
        })
        .map(|c| root.join(format!("{c}.fl")))
        .collect();
    let out_root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("e2e-rt");
    let mut report = Vec::new();
    // what the debug run of each case projects its trace to
    let mut projections: HashMap<PathBuf, String> = HashMap::new();
    for profile in ["debug", "release"] {
        let results = parallel(&cases, jobs(), |case| {
            let rel = case.strip_prefix(&root).expect("case under root");
            let out_dir = out_root.join(profile).join(rel.with_extension(""));
            let ran = run_case_with(case, rel, &out_dir, "js", profile, &["--rt"]);
            (
                ran.problem
                    .map(|p| format!("{} ({profile}):\n{p}", rel.display())),
                ran.projection,
            )
        });
        for (case, (problem, projection)) in cases.iter().zip(results) {
            report.extend(problem);
            if let ("debug", Some(p)) = (profile, projection) {
                projections.insert(case.clone(), p);
            }
        }
    }
    // the same laws on the same programs: what Go's trace projects to agrees
    let portable = portable(&root);
    let on_go: Vec<PathBuf> = cases
        .iter()
        .filter(|case| projections.contains_key(*case))
        .filter(|case| {
            let rel = case.strip_prefix(&root).expect("case under root");
            portable.contains(&rel.with_extension("").to_string_lossy().replace('\\', "/"))
        })
        .cloned()
        .collect();
    if !on_go.is_empty() {
        assert_go();
    }
    let differing = parallel(&on_go, jobs(), |case| {
        let rel = case.strip_prefix(&root).expect("case under root");
        let out_dir = out_root.join("go").join(rel.with_extension(""));
        let ran = run_case(case, rel, &out_dir, "go", "debug");
        match (projections.get(case), &ran.projection) {
            (Some(rt), Some(go)) => trace::differ(rt, go),
            _ => ran.problem,
        }
        .map(|p| format!("{} (Go against rt):\n{p}", rel.display()))
    });
    report.extend(differing.into_iter().flatten());
    println!(
        "rt target: {} cases, both profiles; {} projections against Go",
        cases.len(),
        on_go.len()
    );
    assert!(report.is_empty(), "e2e mismatches:\n{}", report.join("\n"));
}
