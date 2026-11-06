//! Syntax corpus: every ```` ```flick ```` block of the Russian spec, the
//! original text (pinned by `tests/syntax/spec_examples.txt`), and of `docs/`, plus every `.fl` file under
//! `tests/`. All must round-trip losslessly; error-free ones must also format
//! idempotently without changing the AST (every `.fl` of the repository is
//! also checked by `fmt`'s own corpus test). `tests/syntax/*.fl` must parse
//! without diagnostics. The repository's `.fl` sources must already be
//! formatted. The blocks of the site's pages must also check on their own.

use std::path::{Path, PathBuf};
use std::{env, fs, panic};

use flick_core::diagnostics::Severity;
use flick_core::fmt;
use flick_core::markdown::{Mode, blocks};
use flick_core::source::SourceMap;
use flick_core::syntax::parse;

const SPEC: &str = "docs/src/content/docs/ru/spec";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
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

/// `(chapter:line, text)` of each block of the spec, chapter by chapter.
fn spec_blocks() -> Vec<(String, String)> {
    let mut chapters = Vec::new();
    md_files(&root().join(SPEC), &mut chapters);
    chapters.sort();
    chapters
        .iter()
        .flat_map(|path| {
            let markdown = fs::read_to_string(path).expect("spec is readable");
            let chapter = path
                .file_stem()
                .expect("file name")
                .to_string_lossy()
                .into_owned();
            blocks(&markdown)
                .into_iter()
                .map(move |b| (format!("{chapter}:{}", b.line), b.text))
        })
        .collect()
}

#[test]
fn spec_examples_parse_and_are_pinned() {
    let blocks = spec_blocks();
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
            unpinned.push(format!("{print}  {line}  {first}"));
        }
        errors.extend(parse_errors(line, text));
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
    let blocks = match env::var("DUMP_FILE") {
        Ok(path) => vec![(path.clone(), fs::read_to_string(path).expect("DUMP_FILE"))],
        Err(_) => spec_blocks(),
    };
    let mut out = String::new();
    for (line, text) in blocks {
        let mut sources = SourceMap::default();
        let file = sources.add("x", text.as_str());
        let tree = parse(&text, file).tree.debug_dump(&text);
        out.extend(["=== ", &line, "\n", &text, &tree]);
    }
    fs::write(env::var("DUMP").expect("DUMP"), out).expect("write");
}

fn fl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("readable dir") {
        let path = entry.expect("entry").path();
        // installed packages and build output hold no sources of the repository
        let skipped = path.file_name().is_some_and(|n| {
            n == "node_modules" || n == "dist" || n.to_string_lossy().starts_with('.')
        });
        if skipped {
            continue;
        }
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
        // `research` holds notes that sketch syntax the parser does not have yet
        let skipped = path.file_name().is_some_and(|n| {
            n == "node_modules"
                || n == "dist"
                || n == "research"
                || n.to_string_lossy().starts_with('.')
        });
        if skipped {
            continue;
        }
        if path.is_dir() {
            md_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "md" || e == "mdx") {
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
    let mut docs = Vec::new();
    md_files(&root().join("docs"), &mut docs);
    for path in docs {
        let markdown = fs::read_to_string(&path).expect("readable markdown");
        for block in blocks(&markdown) {
            entries.push((format!("{}:{}", path.display(), block.line), block.text));
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
        for block in blocks(&markdown) {
            let name = format!("{}:{}", path.display(), block.line);
            errors.extend(parse_errors(&name, &block.text));
        }
    }
    assert!(
        errors.is_empty(),
        "docs examples fail to parse:\n{}",
        errors.join("\n")
    );
}

/// Every block of the site's pages checks on its own, imports taken on trust:
/// hidden lines declare the names a fragment uses.
#[test]
fn docs_examples_check() {
    let mut pages = Vec::new();
    md_files(&root().join("docs/src/content/docs"), &mut pages);
    // an example's README whose blocks stand on their own
    pages.push(root().join("examples/typed-interpreter/README.md"));
    pages.sort();
    let mut failures = Vec::new();
    let mut checked = 0;
    for path in pages {
        let markdown = fs::read_to_string(&path).expect("readable markdown");
        let page = path
            .strip_prefix(root())
            .unwrap_or(&path)
            .display()
            .to_string();
        for block in blocks(&markdown) {
            let (line, mode) = (block.line, block.mode);
            if mode == Mode::Ignore {
                continue;
            }
            checked += 1;
            let mut sources = SourceMap::default();
            let file = sources.add(format!("{page}:{line}"), block.text);
            let errors: Vec<_> = flick_core::check(&sources, file)
                .into_iter()
                .filter(|d| d.severity == Severity::Error)
                .collect();
            if mode == Mode::Fail && errors.is_empty() {
                failures.push(format!(
                    "{page}:{line}: a `fail` block checks without errors"
                ));
            }
            if matches!(mode, Mode::Check | Mode::Keep) {
                failures.extend(errors.iter().map(|d| {
                    let at = d
                        .span
                        .map_or(0, |s| sources.get(file).line_col(s.start).0 as usize);
                    format!("{page}:{}: {} {}", line + at, d.code, d.message)
                }));
            }
        }
    }
    assert!(checked > 300, "only {checked} docs examples were checked");
    assert!(
        failures.is_empty(),
        "docs examples do not check (declare free names on hidden `# ` lines):\n{}",
        failures.join("\n")
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
        let result = panic::catch_unwind(|| flick_core::check(&sources, file));
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
        match fmt::format(&text, file) {
            Ok(once) => match fmt::format(&once, file) {
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

/// `.fl` sources in the repository are kept as `flick fmt` writes them; run
/// `flick fmt <file>` on a file this reports. The fixtures of the checker's
/// failures and of the editor grammar are left as written: their
/// annotations point at lines and columns of the code around them.
#[test]
fn fl_sources_are_formatted() {
    let dirs = [
        "tests",
        "examples",
        "tools",
        "docs/src/playground",
        "docs/src/widgets",
        "packages",
        "flick/src/commands/templates",
    ];
    let excluded = ["tests/check/fail", "tests/check/projects"].map(|d| root().join(d));
    let mut files = Vec::new();
    for dir in dirs {
        let dir = root().join(dir);
        if dir.is_dir() {
            fl_files(&dir, &mut files);
        }
    }
    files.retain(|f| !excluded.iter().any(|d| f.starts_with(d)));
    files.sort();
    assert!(files.len() > 100, "only {} .fl sources found", files.len());
    let unformatted: Vec<String> = files
        .iter()
        .filter(|path| {
            let text = fs::read_to_string(path).expect("UTF-8 source");
            let mut sources = SourceMap::default();
            let file = sources.add(path.display().to_string(), text.as_str());
            // files with syntax errors are fixtures of those errors
            fmt::format(&text, file).is_ok_and(|out| out != text)
        })
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        unformatted.is_empty(),
        "not formatted (run `flick fmt` on them):\n{}",
        unformatted.join("\n")
    );
}
