//! The fixtures of the live discipline of regions, beside `check_fixtures`.

use super::*;

/// `tests/check/live/{pass,fail}/**/*.fl` under `[build] regions = "live"`,
/// with the annotations of `check_fixtures`; each is also checked under the
/// default discipline, whose verdict its first line states: `// captures:
/// accepts` or `// captures: FLICK-G002` and the like.
#[test]
fn live_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check/live");
    let mut report = Vec::new();
    let mut count = 0;
    for (kind, should_fail) in [("pass", false), ("fail", true)] {
        let mut paths = Vec::new();
        fixtures(&root.join(kind), &mut paths);
        paths.sort();
        count += paths.len();
        for path in paths {
            let mut problems = check_fixture_in(&path, should_fail, RegionMode::Live);
            problems.extend(captures_verdict(&path));
            if !problems.is_empty() {
                let name = path.strip_prefix(&root).unwrap_or(&path).display();
                report.push(format!("{name}:\n{}", problems.join("\n")));
            }
        }
    }
    assert!(count > 0, "no fixtures found under {}", root.display());
    assert!(
        report.is_empty(),
        "live fixture mismatches:\n{}",
        report.join("\n")
    );
}

/// The codes of the errors of a live fixture under the default discipline,
/// against what its first line states.
fn captures_verdict(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).expect("readable fixture");
    let Some(stated) = (text.lines().next()).and_then(|l| l.strip_prefix("// captures: ")) else {
        return Some("  no first line `// captures: …`".to_owned());
    };
    let stated = stated.trim();
    let mut sources = SourceMap::default();
    let file = sources.add(path.display().to_string(), text.clone());
    let mut codes: Vec<String> = flick_core::check(&sources, file)
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.to_string())
        .collect();
    codes.sort();
    codes.dedup();
    let actual = if codes.is_empty() {
        "accepts".to_owned()
    } else {
        codes.join(" ")
    };
    (actual != stated).then(|| format!("  captures: stated `{stated}`, got `{actual}`"))
}
