//! Checker fixture harness: `tests/check/pass/**/*.fl` must produce no
//! diagnostics; `tests/check/fail/**/*.fl` declares its expected diagnostics in
//! annotation comments (format in `docs/testing.md`). Reports a diff per file.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use flick_core::diagnostics::{Diagnostic, codes};
use flick_core::source::{SourceFile, SourceMap};

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
    let mut expected = Vec::new();
    let mut target_line = None;
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let err = |reason: &str| format!("line {number}: {reason}");
        let Some(comment) = line.trim_start().strip_prefix("//") else {
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
    codes::lookup(code).ok_or_else(|| format!("`{code}` is not a registered diagnostic code"))?;
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

/// Matches diagnostics against expectations; returns the diff lines.
fn diff(sources: &SourceMap, actual: &[Diagnostic], expected: Vec<Expected>) -> Vec<String> {
    let mut missing = expected;
    let mut unexpected = Vec::new();
    for d in actual {
        let pos = d.span.map(|s| to_pos(sources.get(s.file), s.start, s.end));
        let code = d.code.to_string();
        let hit = missing
            .iter()
            .position(|e| e.code == code && e.pos == pos && d.message.contains(&e.message));
        match hit {
            Some(i) => {
                missing.remove(i);
            }
            None => unexpected.push(Expected {
                code,
                pos,
                message: d.message.clone(),
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
        actual = flick_core::check(&sources, file);
    }
    let expected = match parse_annotations(&sources.get(file).text) {
        Ok(expected) => expected,
        Err(e) => return vec![format!("  bad annotation: {e}")],
    };
    match (should_fail, expected.is_empty()) {
        (true, true) => vec!["  fail fixture declares no expected diagnostics".into()],
        (false, false) => vec!["  pass fixture must not contain annotations".into()],
        _ => diff(&sources, &actual, expected),
    }
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
        flick_core::source::Span::new(file, 5, 7),
    )];
    let ok = parse_annotations("ab\nцde\n//~ FLICK-S002 @2:2-2:4 bo\n").expect("valid");
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
