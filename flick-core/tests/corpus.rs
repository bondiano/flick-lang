//! Syntax corpus: every ```` ```flick ```` block of the spec (pinned by
//! `tests/syntax/spec_examples.txt`) and of `docs/`, plus every `.fl` file under
//! `tests/`. All must round-trip losslessly; error-free ones must also format
//! idempotently without changing the AST. `tests/syntax/*.fl` must parse
//! without diagnostics.

use std::fs;
use std::path::{Path, PathBuf};

use flick_core::source::SourceMap;
use flick_core::syntax::parse;

const SPEC: &str = "Flick спецификация ML языка с HKT и Effect.md";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// `(first line number, text)` of each ```` ```flick ```` block.
fn flick_blocks(markdown: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut current: Option<(usize, String)> = None;
    for (index, line) in markdown.lines().enumerate() {
        match &mut current {
            None if line.trim_start() == "```flick" => current = Some((index + 2, String::new())),
            None => {}
            Some(_) if line.trim_start() == "```" => blocks.extend(current.take()),
            Some((_, text)) => {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    blocks
}

/// FNV-1a, enough to notice a changed or new example.
fn fingerprint(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

fn parse_errors(name: &str, text: &str) -> Vec<String> {
    let mut sources = SourceMap::default();
    let file = sources.add(name, text);
    let parsed = parse(text, file);
    assert_eq!(parsed.tree.text(text), text, "{name}: tree is not lossless");
    parsed
        .diagnostics
        .iter()
        .map(|d| {
            let span = d.span.expect("syntax diagnostics have spans");
            let (line, col) = sources.get(file).line_col(span.start);
            format!("{name}:{}:{}: {} {}", line + 1, col + 1, d.code, d.message)
        })
        .collect()
}

#[test]
fn spec_examples_parse_and_are_pinned() {
    let spec = fs::read_to_string(root().join(SPEC)).expect("spec is readable");
    let blocks = flick_blocks(&spec);
    let manifest_path = root().join("tests/syntax/spec_examples.txt");
    let manifest = fs::read_to_string(&manifest_path).unwrap_or_default();
    let pinned: Vec<&str> = manifest
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    let mut errors = Vec::new();
    let mut unpinned = Vec::new();
    for (line, text) in &blocks {
        let print = fingerprint(text);
        if !pinned.contains(&print.as_str()) {
            let first = text.lines().next().unwrap_or_default();
            unpinned.push(format!("{print}  spec:{line}  {first}"));
        }
        errors.extend(parse_errors(&format!("spec:{line}"), text));
    }
    assert!(
        errors.is_empty(),
        "spec examples fail to parse:\n{}",
        errors.join("\n")
    );
    assert!(
        unpinned.is_empty(),
        "spec examples not listed in {}; review that they parse as intended, then add:\n{}",
        manifest_path.display(),
        unpinned.join("\n")
    );
    let mut prints: Vec<String> = blocks.iter().map(|(_, text)| fingerprint(text)).collect();
    prints.sort();
    prints.dedup();
    let stale: Vec<_> = pinned
        .iter()
        .filter(|p| !prints.iter().any(|q| q == *p))
        .collect();
    assert!(
        stale.is_empty(),
        "manifest lists examples the spec no longer has: {stale:?}"
    );
}

/// `DUMP=out.txt [DUMP_FILE=x.fl] cargo test --test corpus dump -- --ignored`
#[test]
#[ignore = "debug helper"]
fn dump() {
    let blocks = match std::env::var("DUMP_FILE") {
        Ok(path) => vec![(1, fs::read_to_string(path).expect("DUMP_FILE"))],
        Err(_) => flick_blocks(&fs::read_to_string(root().join(SPEC)).expect("spec")),
    };
    let mut out = String::new();
    for (line, text) in blocks {
        let mut sources = SourceMap::default();
        let file = sources.add("x", text.as_str());
        let tree = parse(&text, file).tree.debug_dump(&text);
        out.extend(["=== ", &line.to_string(), "\n", &text, &tree]);
    }
    fs::write(std::env::var("DUMP").expect("DUMP"), out).expect("write");
}

fn fl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("readable dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            fl_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "fl") {
            out.push(path);
        }
    }
}

fn md_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("readable dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            md_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

/// `(name, text)` of every corpus entry.
fn corpus() -> Vec<(String, String)> {
    let mut entries = Vec::new();
    let mut files = Vec::new();
    fl_files(&root().join("tests"), &mut files);
    files.sort();
    for path in files {
        let text = fs::read_to_string(&path).expect("UTF-8 fixture");
        entries.push((path.display().to_string(), text));
    }
    let mut docs = vec![root().join(SPEC)];
    md_files(&root().join("docs"), &mut docs);
    for path in docs {
        let markdown = fs::read_to_string(&path).expect("readable markdown");
        for (line, text) in flick_blocks(&markdown) {
            entries.push((format!("{}:{line}", path.display()), text));
        }
    }
    entries
}

#[test]
fn docs_examples_parse() {
    let mut docs = Vec::new();
    md_files(&root().join("docs"), &mut docs);
    let mut errors = Vec::new();
    for path in docs {
        let markdown = fs::read_to_string(&path).expect("readable markdown");
        for (line, text) in flick_blocks(&markdown) {
            errors.extend(parse_errors(&format!("{}:{line}", path.display()), &text));
        }
    }
    assert!(
        errors.is_empty(),
        "docs examples fail to parse:\n{}",
        errors.join("\n")
    );
}

/// Syntax fixtures exercise the grammar only; their names need not resolve.
#[test]
fn syntax_fixtures_parse() {
    let mut files = Vec::new();
    fl_files(&root().join("tests/syntax"), &mut files);
    assert!(!files.is_empty(), "no syntax fixtures");
    let errors: Vec<String> = files
        .iter()
        .flat_map(|path| {
            let text = fs::read_to_string(path).expect("UTF-8 fixture");
            parse_errors(&path.display().to_string(), &text)
        })
        .collect();
    assert!(
        errors.is_empty(),
        "syntax fixtures fail to parse:\n{}",
        errors.join("\n")
    );
}

/// The checker reports what it does not understand instead of panicking,
/// also on examples whose names are defined elsewhere.
#[test]
fn checker_survives_the_corpus() {
    for (name, text) in corpus() {
        let mut sources = SourceMap::default();
        let file = sources.add(name.as_str(), text.as_str());
        let result = std::panic::catch_unwind(|| flick_core::check(&sources, file));
        assert!(result.is_ok(), "{name}: the checker panicked");
    }
}

#[test]
fn corpus_is_lossless_and_formats_stably() {
    let mut problems = Vec::new();
    let mut formatted = 0;
    for (name, text) in corpus() {
        let mut sources = SourceMap::default();
        let file = sources.add(name.as_str(), text.as_str());
        let parsed = parse(&text, file);
        if parsed.tree.text(&text) != text {
            problems.push(format!("{name}: tree is not lossless"));
        }
        if !parsed.diagnostics.is_empty() {
            continue;
        }
        // `format` itself rejects output whose AST differs from the input's
        match flick_core::fmt::format(&text, file) {
            Ok(once) => match flick_core::fmt::format(&once, file) {
                Ok(twice) if twice == once => formatted += 1,
                Ok(twice) => problems.push(format!(
                    "{name}: not idempotent\n--- once\n{once}--- twice\n{twice}"
                )),
                Err(e) => problems.push(format!("{name}: formatted output fails: {e:?}")),
            },
            Err(e) => problems.push(format!("{name}: cannot format: {e:?}")),
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(
        formatted > 50,
        "only {formatted} corpus entries were formatted"
    );
}
