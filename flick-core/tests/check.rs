//! Checker fixture harness: `tests/check/pass/**/*.fl` must produce no
//! diagnostics; `tests/check/fail/**/*.fl` declares its expected diagnostics in
//! annotation comments (format in `docs/src/content/docs/contributing/testing.md`). Each directory of
//! `tests/check/projects/` is one project checked as a whole; annotations in
//! any of its files, `#` comments in `flick.toml`, are its expectations.
//! Reports a diff per file.

#[cfg(feature = "native-project")]
use std::io;
use std::path::{Path, PathBuf};
use std::{fmt, fs};

use flick_core::diagnostics::codes::Area;
use flick_core::diagnostics::{Diagnostic, codes};
#[cfg(feature = "native-project")]
use flick_core::project::{self, Host};
use flick_core::source;
use flick_core::source::{FileId, SourceFile, SourceMap};
#[cfg(feature = "native-project")]
use flick_core::syntax::{self, cst::Node, kind::SyntaxKind};

/// 1-based `line:col-line:col`, columns in characters, end exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pos {
    start: (usize, usize),
    end: (usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Expected {
    code: String,
    pos: Option<Pos>,
    /// Substring the diagnostic message must contain (may be empty).
    message: String,
}

impl fmt::Display for Expected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code)?;
        if let Some(Pos { start, end }) = self.pos {
            write!(f, " @{}:{}-{}:{}", start.0, start.1, end.0, end.1)?;
        }
        write!(f, " {:?}", self.message)
    }
}

/// Parses annotations; `Err` carries a 1-based line and a reason.
fn parse_annotations(text: &str) -> Result<Vec<Expected>, String> {
    annotations(text, "//")
}

fn annotations(text: &str, comment_prefix: &str) -> Result<Vec<Expected>, String> {
    let mut expected = Vec::new();
    let mut target_line = None;
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let err = |reason: &str| format!("line {number}: {reason}");
        let Some(comment) = line.trim_start().strip_prefix(comment_prefix) else {
            target_line = Some(number);
            continue;
        };
        if let Some(rest) = comment.strip_prefix('~') {
            let (code, rest) = split_word(rest);
            let (pos, message) = match rest.strip_prefix('@') {
                Some(rest) => {
                    let (pos, message) = split_word(rest);
                    (
                        Some(parse_pos(pos).ok_or_else(|| err("bad `@L:C-L:C`"))?),
                        message,
                    )
                }
                None => (None, rest),
            };
            expected.push(annotation(code, pos, message).map_err(|e| err(&e))?);
        } else if comment.trim_start().starts_with('^') {
            let target = target_line.ok_or_else(|| err("`^` annotation has no line above"))?;
            let col = line.chars().position(|c| c == '^').expect("has a caret") + 1;
            let carets = line.chars().skip(col - 1).take_while(|&c| c == '^').count();
            let (code, message) = split_word(
                line.trim_start_matches(|c| c != '^')
                    .trim_start_matches('^'),
            );
            let pos = Pos {
                start: (target, col),
                end: (target, col + carets),
            };
            expected.push(annotation(code, Some(pos), message).map_err(|e| err(&e))?);
        } else {
            target_line = Some(number);
        }
    }
    Ok(expected)
}

fn annotation(code: &str, pos: Option<Pos>, message: &str) -> Result<Expected, String> {
    let plugin = (code.split_once('/')).is_some_and(|(p, c)| codes::plugin(p, c).is_some());
    if !plugin && codes::lookup(code).is_none() {
        return Err(format!("`{code}` is not a registered diagnostic code"));
    }
    Ok(Expected {
        code: code.to_owned(),
        pos,
        message: message.to_owned(),
    })
}

fn split_word(s: &str) -> (&str, &str) {
    let s = s.trim();
    s.split_once(char::is_whitespace)
        .map_or((s, ""), |(word, rest)| (word, rest.trim()))
}

fn parse_pos(s: &str) -> Option<Pos> {
    let lc = |s: &str| {
        let (l, c) = s.split_once(':')?;
        Some((l.parse().ok()?, c.parse().ok()?))
    };
    let (start, end) = s.split_once('-')?;
    Some(Pos {
        start: lc(start)?,
        end: lc(end)?,
    })
}

fn to_pos(file: &SourceFile, start: u32, end: u32) -> Pos {
    let lc = |offset: u32| {
        let (line, _) = file.line_col(offset);
        let line_start = file.line_start(line).expect("line exists") as usize;
        let col = file.text[line_start..offset as usize].chars().count();
        (line as usize + 1, col + 1)
    };
    Pos {
        start: lc(start),
        end: lc(end),
    }
}

/// Warnings about unused code, checked only in files that mention the
/// code: fixtures of other rules bind names they never use.
const UNUSED: [&str; 3] = ["FLICK-R012", "FLICK-R013", "FLICK-R014"];

/// Unused-code warnings, and in syntax fixtures every diagnostic of a later
/// phase, count only in files that mention their code.
fn opted_out(sources: &SourceMap, d: &Diagnostic) -> bool {
    let code = d.code.to_string();
    let Some(file) = d.span.map(|s| sources.get(s.file)) else {
        return false;
    };
    let syntax_fixture = file.path.contains("fail/syntax/") && d.code.area != Area::Syntax;
    (UNUSED.contains(&code.as_str()) || syntax_fixture) && !file.text.contains(&code)
}

/// Matches diagnostics against expectations; returns the diff lines.
fn diff(sources: &SourceMap, actual: &[&Diagnostic], expected: Vec<Expected>) -> Vec<String> {
    let mut missing = expected;
    let mut unexpected = Vec::new();
    for d in actual.iter().filter(|d| !opted_out(sources, d)) {
        let pos = d.span.map(|s| to_pos(sources.get(s.file), s.start, s.end));
        let code = d.code.to_string();
        // help lines read as the message they explain: `message: help`
        #[cfg(feature = "diagnostic-fixes")]
        let text = d
            .fixes
            .iter()
            .filter(|f| f.edits.is_empty())
            .fold(d.message.clone(), |text, f| {
                format!("{text}: {}", f.message)
            });
        #[cfg(not(feature = "diagnostic-fixes"))]
        let text = d.message.clone();
        let hit = missing
            .iter()
            .position(|e| e.code == code && e.pos == pos && text.contains(&e.message));
        match hit {
            Some(i) => {
                missing.remove(i);
            }
            None => unexpected.push(Expected {
                code,
                pos,
                message: text,
            }),
        }
    }
    let missing = missing.iter().map(|e| format!("  missing:    {e}"));
    let unexpected = unexpected.iter().map(|e| format!("  unexpected: {e}"));
    missing.chain(unexpected).collect()
}

fn fixtures(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("readable fixture dir").path();
        if path.is_dir() {
            fixtures(&path, out);
        } else if path.extension().is_some_and(|e| e == "fl") {
            out.push(path);
        }
    }
}

fn check_fixture(path: &Path, should_fail: bool) -> Vec<String> {
    let mut sources = SourceMap::default();
    let (file, utf8) = sources.add_bytes(
        path.display().to_string(),
        fs::read(path).expect("readable fixture"),
    );
    let mut actual: Vec<_> = utf8.into_iter().collect();
    if actual.is_empty() {
        actual = lone(&mut sources, path, file);
    }
    let expected = match parse_annotations(&sources.get(file).text) {
        Ok(expected) => expected,
        Err(e) => return vec![format!("  bad annotation: {e}")],
    };
    match (should_fail, expected.is_empty()) {
        (true, true) => vec!["  fail fixture declares no expected diagnostics".into()],
        (false, false) => vec!["  pass fixture must not contain annotations".into()],
        _ => diff(&sources, &actual.iter().collect::<Vec<_>>(), expected),
    }
}

/// The diagnostics of checking the file at `path` on its own. One with
/// plugin blocks goes through the loader, as `flick check file.fl` does,
/// which expands them.
#[cfg(feature = "native-project")]
fn lone(sources: &mut SourceMap, path: &Path, file: FileId) -> Vec<Diagnostic> {
    let tree = syntax::parse(&sources.get(file).text, file).tree;
    if !has_blocks(&tree) {
        return flick_core::check(sources, file);
    }
    let host = FixtureHost {
        root: path.parent().unwrap_or(path).to_path_buf(),
    };
    project::check(&host, sources, None, &[path.to_path_buf()])
}

#[cfg(feature = "native-project")]
fn has_blocks(node: &Node) -> bool {
    node.kind == SyntaxKind::PluginExpr || node.nodes().any(has_blocks)
}

#[cfg(not(feature = "native-project"))]
fn lone(sources: &mut SourceMap, _: &Path, file: FileId) -> Vec<Diagnostic> {
    flick_core::check(sources, file)
}

#[test]
fn check_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check");
    let mut report = Vec::new();
    let mut count = 0;
    for (kind, should_fail) in [("pass", false), ("fail", true)] {
        let mut paths = Vec::new();
        fixtures(&root.join(kind), &mut paths);
        paths.sort();
        count += paths.len();
        for path in paths {
            let problems = check_fixture(&path, should_fail);
            if !problems.is_empty() {
                let name = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                report.push(format!("{name}:\n{}", problems.join("\n")));
            }
        }
    }
    assert!(count > 0, "no fixtures found under {}", root.display());
    assert!(
        report.is_empty(),
        "fixture mismatches:\n{}",
        report.join("\n")
    );
}

/// The first example of the UI reference, in English and Russian, is code of
/// a passing fixture: the page shows what the checker accepts.
#[test]
fn ui_reference_example_is_a_checked_fixture() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let fixture =
        fs::read_to_string(repo.join("tests/check/pass/ui/save_button.fl")).expect("fixture");
    for page in ["reference/ui.md", "ru/reference/ui.md"] {
        let text = fs::read_to_string(repo.join("docs/src/content/docs").join(page)).expect(page);
        let example = text
            .split("```flick\n")
            .nth(1)
            .and_then(|rest| rest.split("```").next())
            .unwrap_or_else(|| panic!("{page} has no flick example"));
        assert!(
            fixture.contains(example),
            "the first example of {page} is not in save_button.fl:\n{example}"
        );
    }
}

#[test]
fn parses_caret_and_explicit_annotations() {
    let text = "let x = 1\n//  ^^^ FLICK-S002 bad thing\n//~ FLICK-P001 no span\n//~ FLICK-S001 @1:1-2:3\n";
    let parsed = parse_annotations(text).expect("valid");
    let shown: Vec<_> = parsed.iter().map(ToString::to_string).collect();
    assert_eq!(
        shown,
        [
            "FLICK-S002 @1:5-1:8 \"bad thing\"",
            "FLICK-P001 \"no span\"",
            "FLICK-S001 @1:1-2:3 \"\"",
        ]
    );
}

#[test]
fn rejects_unknown_codes_and_orphan_carets() {
    assert!(
        parse_annotations("x\n//~ FLICK-Z999\n")
            .expect_err("unknown code")
            .contains("line 2")
    );
    assert!(parse_annotations("// ^^ FLICK-S002\n").is_err());
}

#[test]
fn diff_reports_missing_and_unexpected() {
    let mut sources = SourceMap::default();
    let file = sources.add("t.fl", "ab\nцde\n");
    let actual = [Diagnostic::error(
        codes::S002,
        "boom",
        source::Span::new(file, 5, 7),
    )];
    let ok = parse_annotations("ab\nцde\n//~ FLICK-S002 @2:2-2:4 bo\n").expect("valid");
    let actual: Vec<_> = actual.iter().collect();
    assert_eq!(diff(&sources, &actual, ok), Vec::<String>::new());
    let wrong = parse_annotations("ab\nцde\n//  ^ FLICK-S002\n").expect("valid");
    assert_eq!(
        diff(&sources, &actual, wrong),
        [
            "  missing:    FLICK-S002 @2:5-2:6 \"\"",
            "  unexpected: FLICK-S002 @2:2-2:4 \"boom\""
        ]
    );
}

#[cfg(feature = "native-project")]
struct FixtureHost {
    root: PathBuf,
}

#[cfg(feature = "native-project")]
impl Host for FixtureHost {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn display(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .display()
            .to_string()
    }
}

/// Checks one fixture project the way `flick check` without paths does.
#[cfg(feature = "native-project")]
fn check_project(dir: &Path) -> Vec<String> {
    let host = FixtureHost {
        root: dir.to_path_buf(),
    };
    let mut sources = SourceMap::default();
    let diagnostics = match project::discover(&host, &mut sources, dir, Some(Path::new("."))) {
        Ok(Some(project)) => {
            let mut roots = Vec::new();
            fixtures(&project.source_root(), &mut roots);
            fixtures(&project.test_root(), &mut roots);
            project::check(&host, &mut sources, Some(&project), &roots)
        }
        Ok(None) => vec![],
        Err(diagnostic) => vec![diagnostic],
    };
    let diagnostics: Vec<Diagnostic> = diagnostics
        .into_iter()
        .filter(|d| !opted_out(&sources, d))
        .collect();
    let mut files: Vec<PathBuf> = Vec::new();
    fixtures(dir, &mut files);
    files.push(dir.join("flick.toml"));
    files.sort();
    let mut report = Vec::new();
    let mut expected_any = false;
    let mut accounted = vec![false; diagnostics.len()];
    for path in files {
        let shown = host.display(&path);
        let text = fs::read_to_string(&path).unwrap_or_default();
        let prefix = if path.extension().is_some_and(|e| e == "toml") {
            "#"
        } else {
            "//"
        };
        let expected = match annotations(&text, prefix) {
            Ok(expected) => expected,
            Err(e) => {
                report.push(format!("{shown}: bad annotation: {e}"));
                continue;
            }
        };
        expected_any |= !expected.is_empty();
        let mut actual = Vec::new();
        for (i, d) in diagnostics.iter().enumerate() {
            // spanless diagnostics are annotated in `flick.toml`
            let file = d.span.map(|s| sources.get(s.file).path.clone());
            if file.as_deref().unwrap_or("flick.toml") == shown {
                accounted[i] = true;
                actual.push(d);
            }
        }
        let problems = diff(&sources, &actual, expected);
        if !problems.is_empty() {
            report.push(format!("{shown}:\n{}", problems.join("\n")));
        }
    }
    for (d, _) in diagnostics.iter().zip(&accounted).filter(|(_, a)| !**a) {
        report.push(format!(
            "  unexpected in an unannotated file: {} {}",
            d.code, d.message
        ));
    }
    if !expected_any && report.is_empty() && !diagnostics.is_empty() {
        report.push("  project has diagnostics but no annotations".into());
    }
    report
}

#[cfg(feature = "native-project")]
#[test]
fn project_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check/projects");
    let mut cases: Vec<PathBuf> = fs::read_dir(&root)
        .expect("tests/check/projects exists")
        .map(|e| e.expect("readable").path())
        .filter(|p| p.is_dir())
        .collect();
    cases.sort();
    assert!(!cases.is_empty());
    let report: Vec<String> = cases
        .iter()
        .filter_map(|case| {
            let problems = check_project(&project::normalize(case));
            let name = case.file_name().expect("named").to_string_lossy();
            (!problems.is_empty()).then(|| format!("{name}:\n{}", problems.join("\n")))
        })
        .collect();
    assert!(
        report.is_empty(),
        "project fixture mismatches:\n{}",
        report.join("\n")
    );
}
