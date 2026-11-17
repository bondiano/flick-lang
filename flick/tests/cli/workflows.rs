//! The workflows are bounded, they report what failed, and a package tag does
//! not take the compiler's `latest` release.
//!
//! A job without `timeout-minutes` runs for GitHub's six hours; a nightly
//! that fails quietly catches nothing; and a `std-v*` release that takes
//! `latest` moves the release the installer resolves, so the compiler archive
//! it expects is not there. These are read as text: a workflow has no schema
//! here, and the assertions are about the shape of the file.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The workflows this test holds. `release.yml` belongs to the distribution
/// agent and is only read, below, to pin what the compiler release must do.
const OWNED: &[&str] = &[
    "ci.yml",
    "docs.yml",
    "nightly.yml",
    "packages.yml",
    "perf.yml",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate is in the repository")
        .to_path_buf()
}

fn workflow(name: &str) -> String {
    fs::read_to_string(root().join(".github/workflows").join(name))
        .unwrap_or_else(|error| panic!("a readable `.github/workflows/{name}`: {error}"))
}

/// The jobs of a workflow, each with the lines of its body.
fn jobs(text: &str) -> BTreeMap<String, Vec<String>> {
    let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    // `on:` holds `  push:` and `  pull_request:` at the depth of a job, and
    // only what comes after `jobs:` is one
    let mut in_jobs = false;
    for line in text.lines() {
        if line == "jobs:" {
            in_jobs = true;
            continue;
        }
        if !in_jobs {
            continue;
        }
        let header = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && line
                .trim()
                .trim_end_matches(':')
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        if header {
            let name = line.trim().trim_end_matches(':').to_owned();
            found.entry(name.clone()).or_default();
            current = Some(name);
        } else if let Some(name) = current.as_ref()
            && let Some(body) = found.get_mut(name)
        {
            body.push(line.to_owned());
        }
    }
    found
}

/// The `gh release create` of a workflow, its continued lines joined.
fn release_create(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.contains("gh release create"))
        .expect("a `gh release create`");
    let mut command = String::new();
    for line in &lines[at..] {
        command.push_str(line);
        command.push(' ');
        if !line.trim_end().ends_with('\\') {
            break;
        }
    }
    command
}

#[test]
fn every_job_of_every_workflow_we_run_has_a_timeout() {
    for file in OWNED {
        for (job, body) in jobs(&workflow(file)) {
            // a job that calls a reusable workflow cannot hold one: the jobs
            // of the called workflow hold their own
            let reusable = body.iter().any(|line| {
                line.strip_prefix("    ")
                    .is_some_and(|rest| rest.starts_with("uses:"))
            });
            if reusable {
                continue;
            }
            assert!(
                body.iter().any(|line| {
                    line.strip_prefix("    ")
                        .is_some_and(|rest| rest.starts_with("timeout-minutes:"))
                }),
                "{file}: job `{job}` has no `timeout-minutes`, so a hang holds a runner for the \
                 six hours GitHub allows"
            );
        }
    }
}

#[test]
fn a_package_tag_does_not_take_the_latest_release() {
    let packages = release_create(&workflow("packages.yml"));
    assert!(
        packages.contains("--latest=false"),
        "a `*-v*` tag releases a package, not the compiler: without `--latest=false` it takes \
         the `latest` release the installer resolves, and the compiler archive is not there: \
         {packages}"
    );
    let release = release_create(&workflow("release.yml"));
    assert!(
        !release.contains("--latest=false"),
        "a `v<version>` tag is the compiler release, the only release `latest` may name: {release}"
    );
}

#[test]
fn the_installer_is_tested_where_every_pull_request_sees_it() {
    let text = workflow("ci.yml");
    let job = jobs(&text)
        .into_iter()
        .find(|(_, body)| {
            body.iter()
                .any(|line| line.contains("node --test .github/actions/setup-flick"))
        })
        .map(|(job, _)| job)
        .expect(
            "the setup Action resolves the compiler release the installer downloads, so a job \
             must run `node --test .github/actions/setup-flick/install.test.mjs`",
        );
    assert!(
        text.contains("  pull_request:"),
        "`ci.yml` must run on a pull request, or job `{job}` tests the installer on main only"
    );
}

#[test]
fn the_nightly_cancels_an_older_run_and_reports_what_failed() {
    let text = workflow("nightly.yml");
    assert!(
        text.contains("cancel-in-progress: true"),
        "a newer nightly must replace the older one, as a pull request replaces its run"
    );
    let jobs = jobs(&text);
    let report = jobs
        .get("report")
        .expect("a `report` job: the nightly must say which of its jobs failed")
        .join("\n");
    let needs = report
        .lines()
        .find(|line| line.trim_start().starts_with("needs:"))
        .expect("the report depends on what it reports");
    for needed in ["ci", "fuzz", "perf"] {
        assert!(
            needs.contains(needed),
            "the report must depend on `{needed}`: {needs}"
        );
    }
    assert!(
        report.contains("always()") && report.contains("!cancelled()"),
        "the report runs after a failure, and not for a cancelled run"
    );
    assert!(
        report.contains("GITHUB_STEP_SUMMARY"),
        "the report writes the run's step summary"
    );
    for write in ["gh issue", "webhook", "slack", "curl "] {
        assert!(
            !report.contains(write),
            "the report writes to the run only, not through `{write}`"
        );
    }
    let fuzz = jobs.get("fuzz").expect("a `fuzz` job").join("\n");
    assert!(
        fuzz.contains("upload-artifact") && fuzz.contains("fuzz.log"),
        "a failing fuzz run must leave its log as an artifact"
    );
}
