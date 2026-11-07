//! The known gaps of the e2e cases: `tests/run/known_gaps.txt` lists a case
//! a target does not pass yet. It must fail there, and one that passes is
//! reported, so its line goes and the case holds the target from then on.

use std::fs;
use std::path::Path;

/// The lines of the list: a target, `js` or `go`, then a case by path
/// without `.fl`.
pub fn load(root: &Path) -> Vec<(String, String)> {
    let text =
        fs::read_to_string(root.join("known_gaps.txt")).expect("readable tests/run/known_gaps.txt");
    let gaps: Vec<(String, String)> = text
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(|l| {
            let (target, case) = l.split_once(' ').expect("`<target> <case>`");
            (target.to_owned(), case.trim().to_owned())
        })
        .collect();
    for (target, case) in &gaps {
        assert!(
            matches!(target.as_str(), "js" | "go") && root.join(format!("{case}.fl")).exists(),
            "tests/run/known_gaps.txt lists `{target} {case}`: a target, then a case"
        );
    }
    gaps
}

/// Whether `case`, a path under `root`, is a known gap on `target`.
pub fn listed(gaps: &[(String, String)], target: &str, root: &Path, case: &Path) -> bool {
    let rel = case.strip_prefix(root).expect("case under root");
    let name = rel.with_extension("").to_string_lossy().replace('\\', "/");
    gaps.iter().any(|(t, c)| t == target && *c == name)
}

/// What a run of a case reports: its problem, and for a known gap on that
/// target only that it has none.
pub fn verdict(known_gap: bool, problem: Option<String>) -> Option<String> {
    match (known_gap, problem) {
        (true, Some(_)) => None,
        (true, None) => Some("  passes: remove its line from tests/run/known_gaps.txt".into()),
        (false, problem) => problem,
    }
}

#[test]
fn a_gap_that_passes_is_reported_and_one_that_fails_is_not() {
    assert_eq!(verdict(true, Some("differs".into())), None);
    assert!(verdict(true, None).is_some_and(|p| p.contains("known_gaps.txt")));
    assert_eq!(verdict(false, None), None);
    assert_eq!(
        verdict(false, Some("differs".into())).as_deref(),
        Some("differs")
    );
}
