//! The cases of tests/run on Flick's own runtime.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};
use std::{env, thread};

use super::{assert_go, jobs, listed_cases, parallel, portable, run_case, run_case_with, trace};

/// How long a case not in `rt.txt` may run: some never end on this runtime.
const UNLISTED_LIMIT: Duration = Duration::from_secs(30);

/// What `command` printed and how it ended; after `limit` it is killed and
/// what it printed is dropped, as a pipe a child of it holds may never close.
pub fn output_within(command: &mut Command, limit: Option<Duration>) -> Output {
    const NODE: &str = "the program runs: `node` (Node.js >= 24.18.0) is on PATH for JS";
    let Some(limit) = limit else {
        return command.output().expect(NODE);
    };
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect(NODE);
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = read(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = read(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait().expect("the program is awaited") {
            let joined = |t: thread::JoinHandle<Vec<u8>>| t.join().expect("a pipe is read");
            return Output {
                status,
                stdout: joined(stdout),
                stderr: joined(stderr),
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let status = child.wait().expect("the program is awaited");
            return Output {
                status,
                stdout: Vec::new(),
                stderr: b"stopped: it ran out of time".to_vec(),
            };
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// The cases of `tests/run/rt.txt` on Flick's own runtime, in both profiles,
/// each trace checked by the monitor, and every other case of tests/run in
/// debug, which must fail: one that passes is reported, so it joins the list,
/// as `known_gaps.txt` reports a gap that passes. Its own test: no other run
/// builds for it.
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
            // `../rt/x` stays under the profile's directory, which may not exist yet
            let under = rel.strip_prefix("..").unwrap_or(rel);
            let out_dir = out_root.join(profile).join(under.with_extension(""));
            let ran = run_case_with(case, rel, &out_dir, "js", profile, &["--rt"], None);
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
    // the cases of tests/rt run on Go too, against their own expectations: the
    // depth of a tail call and a chain of wakes is no matter of one runtime
    let own: Vec<PathBuf> = cases
        .iter()
        .filter(|case| {
            case.strip_prefix(&root)
                .is_ok_and(|r| r.starts_with("../rt"))
        })
        .cloned()
        .collect();
    if !on_go.is_empty() || !own.is_empty() {
        assert_go();
    }
    let on_own = parallel(&own, jobs(), |case| {
        let rel = case.strip_prefix(&root).expect("case under root");
        let name = case.file_stem().expect("a case has a name");
        let out_dir = out_root.join("go-own").join(name);
        run_case(case, rel, &out_dir, "go", "debug")
            .problem
            .map(|p| format!("{} (Go):\n{p}", rel.display()))
    });
    report.extend(on_own.into_iter().flatten());
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
        "rt target: {} cases, both profiles; {} projections against Go; {} of tests/rt on Go",
        cases.len(),
        on_go.len(),
        own.len()
    );
    report.extend(unlisted_passing(&root, &cases, only.as_deref(), &out_root));
    assert!(report.is_empty(), "e2e mismatches:\n{}", report.join("\n"));
}

/// The cases of tests/run under `root` that `listed` lacks, run in debug: a
/// report of each that passes, which then holds the runtime from now on.
fn unlisted_passing(
    root: &Path,
    listed: &[PathBuf],
    only: Option<&str>,
    out_root: &Path,
) -> Vec<String> {
    let started = Instant::now();
    let mut all = Vec::new();
    super::cases(root, &mut all);
    all.sort();
    let unlisted: Vec<PathBuf> = all
        .into_iter()
        .filter(|c| !listed.contains(c))
        .filter(|c| only.is_none_or(|o| o.split(',').any(|p| c.to_string_lossy().contains(p))))
        .collect();
    let passing = parallel(&unlisted, jobs(), |case| {
        let rel = case.strip_prefix(root).expect("case under root");
        let out_dir = out_root.join("unlisted").join(rel.with_extension(""));
        let limit = Some(UNLISTED_LIMIT);
        let ran = run_case_with(case, rel, &out_dir, "js", "debug", &["--rt"], limit);
        ran.problem.is_none().then(|| {
            let rel = rel.with_extension("");
            let rel = rel.display();
            format!("{rel}: passes on Flick's own runtime: add it to tests/run/rt.txt")
        })
    });
    println!(
        "rt target: {} other cases in debug, in {:.0?}",
        unlisted.len(),
        started.elapsed()
    );
    passing.into_iter().flatten().collect()
}
