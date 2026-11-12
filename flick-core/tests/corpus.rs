//! Syntax corpus: every ```` ```flick ```` block of the Russian spec, the
//! original text (pinned by `tests/syntax/spec_examples.txt`), and of `docs/`, plus every `.fl` file under
//! `tests/`. All must round-trip losslessly; error-free ones must also format
//! idempotently without changing the AST (every `.fl` of the repository is
//! also checked by `fmt`'s own corpus test). `tests/syntax/*.fl` must parse
//! without diagnostics. The repository's `.fl` sources must already be
//! formatted. The blocks of the site's pages must also check on their own.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::{env, fs, panic};

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::fmt;
use flick_core::markdown::{Block, Mode, blocks};
use flick_core::source::SourceMap;
use flick_core::syntax::parse;
use flick_test_support::files as fixture_files;

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

/// The syntax diagnostics of `text` as `(code, rendered)`.
fn parse_diagnostics(name: &str, text: &str) -> Vec<(String, String)> {
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
            (
                d.code.to_string(),
                format!("{name}:{}:{}: {} {}", line + 1, col + 1, d.code, d.message),
            )
        })
        .collect()
}

fn parse_errors(name: &str, text: &str) -> Vec<String> {
    parse_diagnostics(name, text)
        .into_iter()
        .map(|(_, rendered)| rendered)
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
    files.extend(fixture_files(&root().join("tests"), "fl"));
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
    // the Russian explanations are sources of the generated reference pages,
    // checked there with the code their fence fails with
    docs.retain(|path| !path.to_string_lossy().contains("/error-translations/"));
    let mut errors = Vec::new();
    for path in docs {
        let markdown = fs::read_to_string(&path).expect("readable markdown");
        for block in blocks(&markdown) {
            let name = format!("{}:{}", path.display(), block.line);
            let mut found = parse_diagnostics(&name, &block.text);
            // a block that must fail may fail to parse, but only with the
            // diagnostics its fence names
            if block.mode == Mode::Fail {
                let named: Vec<String> = block
                    .codes
                    .iter()
                    .map(|code| short(code).to_owned())
                    .collect();
                found.retain(|(code, _)| !named.iter().any(|named| named == short(code)));
            }
            errors.extend(found.into_iter().map(|(_, rendered)| rendered));
        }
    }
    assert!(
        errors.is_empty(),
        "docs examples fail to parse:\n{}",
        errors.join("\n")
    );
}

/// Every block of the site's pages checks on its own, imports taken on trust:
/// hidden lines declare the names a fragment uses. A `fail` block must report
/// the codes its fence names and no other error, a checked block no error, and
/// an `ignore` block of the reference is a fragment the checker cannot compile,
/// listed with its reason in `docs/error-examples.txt`; that list may only
/// shrink, and changing an exempted example needs a new reviewed line.
#[test]
fn docs_examples_check() {
    let mut pages = Vec::new();
    md_files(&root().join("docs/src/content/docs"), &mut pages);
    // an example's README whose blocks stand on their own
    pages.push(root().join("examples/typed-interpreter/README.md"));
    pages.sort();
    let exemptions = exemptions();
    let mut seen = BTreeSet::new();
    let mut failures = Vec::new();
    let mut counts = Counts::default();
    for path in pages {
        let markdown = fs::read_to_string(&path).expect("readable markdown");
        let page = path
            .strip_prefix(root())
            .unwrap_or(&path)
            .display()
            .to_string();
        for block in blocks(&markdown) {
            if block.mode == Mode::Ignore {
                ignored(
                    &page,
                    &block,
                    &exemptions,
                    &mut seen,
                    &mut counts,
                    &mut failures,
                );
            } else {
                checked(&page, &block, &mut counts, &mut failures);
            }
        }
    }
    // every exemption covers an ignored example of the English page and of
    // its translation, and no example is left unexplained
    assert!(
        counts.exempt >= exemptions.len(),
        "{} ignored examples for {} exemptions",
        counts.exempt,
        exemptions.len()
    );
    for exemption in &exemptions {
        if !seen.contains(&(exemption.code.clone(), exemption.fingerprint.clone())) {
            failures.push(format!(
                "docs/error-examples.txt lists {} {} ({}), but no ignored example has that text",
                exemption.code, exemption.fingerprint, exemption.reason
            ));
        }
    }
    assert!(
        counts.checked > 300,
        "only {} docs examples were checked",
        counts.checked
    );
    assert!(
        failures.is_empty(),
        "docs examples do not check as their fences say:\n{}",
        failures.join("\n")
    );
    // the counts of what each kind of example does may only grow where
    // examples are repaired
    assert!(
        counts.failed >= 3 && counts.valid >= 80,
        "only {} examples report the code of their page and {} check as valid programs",
        counts.failed,
        counts.valid
    );
}

/// What the examples of the site's pages turned out to be.
#[derive(Default)]
struct Counts {
    checked: usize,
    failed: usize,
    valid: usize,
    exempt: usize,
}

/// An `ignore` block: a reviewed exemption of the reference, or elsewhere an
/// inline reason of the fence itself.
fn ignored(
    page: &str,
    block: &Block,
    exemptions: &[Exemption],
    seen: &mut BTreeSet<(String, String)>,
    counts: &mut Counts,
    failures: &mut Vec<String>,
) {
    let line = block.line;
    counts.exempt += 1;
    let Some(code) = page_code(page) else {
        if block.reason.trim().len() < MIN_REASON {
            failures.push(format!(
                "{page}:{line}: an `ignore` block must say why it cannot be checked:\
                 \n```flick ignore the reason\n```"
            ));
        }
        return;
    };
    if !block.reason.is_empty() {
        failures.push(format!(
            "{page}:{line}: the reason of a reference example is reviewed in docs/error-examples.txt, not in the fence"
        ));
    }
    let print = fingerprint(&block.text);
    if exemptions
        .iter()
        .any(|e| e.code == code && e.fingerprint == print)
    {
        seen.insert((code, print));
    } else {
        failures.push(format!(
            "{page}:{line}: this example is neither checked nor exempted; \
             if it cannot be compiled, add a reviewed line to docs/error-examples.txt:\n\
             {code} {print} <why the checker cannot compile it>"
        ));
    }
}

/// A `fail` or checked block, as its fence says.
fn checked(page: &str, block: &Block, counts: &mut Counts, failures: &mut Vec<String>) {
    let line = block.line;
    counts.checked += 1;
    let mut sources = SourceMap::default();
    let file = sources.add(format!("{page}:{line}"), block.text.as_str());
    let diagnostics = flick_core::check(&sources, file);
    let found: Vec<String> = diagnostics
        .iter()
        .map(|d| {
            let code = d.code.to_string();
            short(&code).to_owned()
        })
        .collect();
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    if block.mode == Mode::Fail {
        counts.failed += 1;
        failed_as_fenced(page, block, &found, &errors, failures);
    } else {
        if errors.is_empty() {
            counts.valid += 1;
        }
        failures.extend(errors.iter().map(|d| {
            let at = d
                .span
                .map_or(0, |s| sources.get(file).line_col(s.start).0 as usize);
            format!("{page}:{}: {} {}", line + at, d.code, d.message)
        }));
    }
}

/// The failures of a `fail` block: it must name the codes it reports, and
/// report the codes it names.
fn failed_as_fenced(
    page: &str,
    block: &Block,
    found: &[String],
    errors: &[&Diagnostic],
    failures: &mut Vec<String>,
) {
    let line = block.line;
    if block.codes.is_empty() {
        failures.push(format!(
            "{page}:{line}: a `fail` block must name the code it expects:\n\
             ```flick fail {}```",
            page_code(page).unwrap_or_else(|| "T010".to_owned())
        ));
        return;
    }
    for code in &block.codes {
        let code = short(code);
        if !found.iter().any(|found| found == code) {
            failures.push(format!(
                "{page}:{line}: expected {code}, got {}",
                if found.is_empty() {
                    "no diagnostic".to_owned()
                } else {
                    found.join(", ")
                }
            ));
        }
    }
    let unexpected: Vec<String> = errors
        .iter()
        .map(|d| {
            let code = d.code.to_string();
            short(&code).to_owned()
        })
        .filter(|code| !block.codes.iter().any(|wanted| short(wanted) == code))
        .collect();
    if !unexpected.is_empty() {
        failures.push(format!(
            "{page}:{line}: besides the codes it names, the block reports {}",
            unexpected.join(", ")
        ));
    }
}

/// An example the checker cannot compile, with the reason it may not.
struct Exemption {
    code: String,
    fingerprint: String,
    reason: String,
}

/// The reviewed exemptions of `docs/error-examples.txt`: one line per
/// `flick ignore` example, `CODE FINGERPRINT a reason of at least 20 characters`.
fn exemptions() -> Vec<Exemption> {
    let path = root().join("docs/error-examples.txt");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{} is readable: {err}", path.display()));
    let mut exemptions = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.splitn(3, char::is_whitespace);
        let (Some(code), Some(print), Some(reason)) = (words.next(), words.next(), words.next())
        else {
            panic!(
                "{}:{}: expected `CODE FINGERPRINT reason`",
                path.display(),
                index + 1
            );
        };
        assert!(
            codes::lookup(&format!("FLICK-{code}")).is_some(),
            "{}:{}: `{code}` is not a diagnostic code",
            path.display(),
            index + 1
        );
        assert!(
            reason.trim().len() >= MIN_REASON,
            "{}:{}: the reason says too little",
            path.display(),
            index + 1
        );
        assert!(
            !exemptions
                .iter()
                .any(|e: &Exemption| e.code == code && e.fingerprint == print),
            "{}:{}: listed twice",
            path.display(),
            index + 1
        );
        exemptions.push(Exemption {
            code: code.to_owned(),
            fingerprint: print.to_owned(),
            reason: reason.to_owned(),
        });
    }
    exemptions
}

/// The fewest characters an `ignore` reason can have to say anything.
const MIN_REASON: usize = 20;

/// `TRIAGE_CODE=T010 cargo test --test corpus dump -- --ignored --nocapture`
/// prints what each example of an explanation reports today (the `~dump`
/// filter of `just e2e-tests` keeps it out of the e2e set).
#[test]
#[ignore = "debug helper"]
fn dump_explanations() {
    let only = env::var("TRIAGE_CODE").ok();
    let mut files = Vec::new();
    md_files(
        &root().join("flick-core/src/diagnostics/explain"),
        &mut files,
    );
    files.sort();
    if let Some(only) = &only {
        files.retain(|path| path.file_stem().is_some_and(|name| name == only.as_str()));
    }
    for path in files {
        let code = path
            .file_stem()
            .expect("a code name")
            .to_string_lossy()
            .into_owned();
        let markdown = fs::read_to_string(&path).expect("readable explanation");
        for block in blocks(&markdown) {
            let mut sources = SourceMap::default();
            let file = sources.add(code.clone(), block.text.as_str());
            let diagnostics = flick_core::check(&sources, file);
            let found: Vec<String> = diagnostics
                .iter()
                .map(|d| {
                    let code = d.code.to_string();
                    let short = short(&code).to_owned();
                    match d.severity {
                        Severity::Error => short,
                        _ => format!("{short}?"),
                    }
                })
                .collect();
            if only.is_some() {
                println!(
                    "{code}:{line} {mode:?}",
                    line = block.line,
                    mode = block.mode
                );
                for d in &diagnostics {
                    println!("    {:?} {}", d.severity, d.message);
                }
            } else {
                println!(
                    "{code}|{line}|{mode:?}|{found}|{print}",
                    line = block.line,
                    mode = block.mode,
                    print = fingerprint(&block.text),
                    found = if found.is_empty() {
                        "clean".to_owned()
                    } else {
                        found.join(",")
                    },
                );
            }
        }
    }
}

/// `code` without its `FLICK-` prefix, as a fence names it.
fn short(code: &str) -> &str {
    code.strip_prefix("FLICK-").unwrap_or(code)
}

/// The code of a generated reference page, `reference/errors/flick-t010.md`.
fn page_code(page: &str) -> Option<String> {
    let name = page.rsplit('/').next()?.strip_suffix(".md")?;
    let short = name.strip_prefix("flick-")?;
    (short.len() == 4).then(|| short.to_ascii_uppercase())
}

/// Syntax fixtures exercise the grammar only; their names need not resolve.
#[test]
fn syntax_fixtures_parse() {
    let mut files = Vec::new();
    files.extend(fixture_files(&root().join("tests/syntax"), "fl"));
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
        "docs/tour",
        "docs/exercises",
        "packages",
        "flick/src/commands/templates",
    ];
    let excluded = ["tests/check/fail", "tests/check/projects"].map(|d| root().join(d));
    let mut files = Vec::new();
    for dir in dirs {
        let dir = root().join(dir);
        if dir.is_dir() {
            files.extend(fixture_files(&dir, "fl"));
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
