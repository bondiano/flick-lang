//! The model accepts what flick-core accepts: every fixture of
//! `tests/check/pass` and every checked example of the docs that flick-core
//! compiles as a program is checked by the model against the schemes
//! flick-core gives it. A case the model cannot translate yet (`gap`), or
//! where it disagrees for a reason `docs/formal` records (`disagree`), is a
//! line of `known_gaps.txt`; the list may only shrink.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::{fmt, fs};

use flick_core::markdown::{Mode, blocks};
use flick_core::project::Compiled;
use flick_model::check::{TypeError, check_program};
use flick_model::run;
use flick_model::syntax::Name;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// How the model judges a case flick-core accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    Accepted,
    /// Only forms the model does not translate yet.
    Gap(String),
    /// A type error of the model's own rules.
    Disagree(String),
}

impl Verdict {
    fn class(&self) -> Option<&'static str> {
        match self {
            Verdict::Accepted => None,
            Verdict::Gap(_) => Some("gap"),
            Verdict::Disagree(_) => Some("disagree"),
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verdict::Accepted => f.write_str("accepted"),
            Verdict::Gap(why) | Verdict::Disagree(why) => f.write_str(why),
        }
    }
}

/// A foreign binding is taken on its declared scheme, as a native is.
fn foreign(error: &TypeError) -> bool {
    (error.rule == "Translation" && error.message.starts_with("no native for"))
        || error.message.ends_with("has no type in the model")
}

fn verdict(modules: &[Compiled]) -> Verdict {
    let program = run::program(modules);
    let decls = run::decls(modules);
    let checked: HashSet<Name> = modules
        .iter()
        .map(|m| Name::from(m.name.as_str()))
        .collect();
    let errors: Vec<_> = check_program(&program, &decls, &checked)
        .into_iter()
        .filter(|(_, e)| !foreign(e))
        .collect();
    let first = |errors: &[(_, TypeError)]| {
        let (path, error) = &errors[0];
        format!("{path}: {error}")
    };
    if errors.is_empty() {
        Verdict::Accepted
    } else if errors.iter().all(|(_, e)| e.rule == "Translation") {
        Verdict::Gap(first(&errors))
    } else {
        Verdict::Disagree(first(&errors))
    }
}

fn fixtures(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("readable fixtures")
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            fixtures(&path, out);
        } else if path.extension().is_some_and(|e| e == "fl") {
            out.push(path);
        }
    }
}

fn md_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("readable docs")
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            md_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "md" || e == "mdx") {
            out.push(path);
        }
    }
}

/// FNV-1a, as `flick-core/tests/corpus.rs` names an example.
fn fingerprint(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

fn relative(path: &Path) -> String {
    path.strip_prefix(repo())
        .unwrap_or(path)
        .display()
        .to_string()
}

/// `key -> class` of `known_gaps.txt`.
fn known() -> BTreeMap<String, String> {
    let text =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/known_gaps.txt"))
            .unwrap_or_default();
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(|l| {
            let (class, key) = l.split_once(' ').expect("`<class> <key>`");
            (key.trim().to_owned(), class.to_owned())
        })
        .collect()
}

#[test]
#[ignore = "checks every pass fixture and docs example in the model; run `just model`"]
fn the_model_accepts_what_flick_core_accepts() {
    let mut cases: BTreeMap<String, Verdict> = BTreeMap::new();
    let mut paths = Vec::new();
    fixtures(&repo().join("tests/check/pass"), &mut paths);
    let fixtures = paths.len();
    for path in paths {
        let modules = run::compile(&path).unwrap_or_else(|e| {
            panic!(
                "{}: flick-core rejects a pass fixture:\n{e}",
                relative(&path)
            )
        });
        cases.insert(relative(&path), verdict(&modules));
    }
    let mut pages = Vec::new();
    md_files(&repo().join("docs/src/content/docs"), &mut pages);
    let mut examples = 0;
    for page in pages {
        let markdown = fs::read_to_string(&page).expect("readable page");
        for block in blocks(&markdown) {
            if !matches!(block.mode, Mode::Check | Mode::Keep) {
                continue;
            }
            // a fragment flick-core checks alone but does not compile as a
            // program is no case of the model
            let Ok(modules) = run::compile_source(&block.text) else {
                continue;
            };
            examples += 1;
            let key = format!("{} {}", relative(&page), fingerprint(&block.text));
            cases.insert(key, verdict(&modules));
        }
    }
    assert!(fixtures >= 80, "too few pass fixtures: {fixtures}");
    assert!(examples >= 100, "too few docs examples compile: {examples}");
    let known = known();
    let mut failures = Vec::new();
    for (key, verdict) in &cases {
        match (verdict.class(), known.get(key)) {
            (None, Some(_)) => failures.push(format!("now accepted, remove its line: {key}")),
            (Some(class), None) => {
                failures.push(format!("unlisted: `{class} {key} # <reason>`: {verdict}"));
            }
            (Some(class), Some(listed)) if class != listed => {
                failures.push(format!(
                    "`{key}` is `{class}`, listed `{listed}`: {verdict}"
                ));
            }
            _ => {}
        }
    }
    for key in known.keys() {
        if !cases.contains_key(key) {
            failures.push(format!("listed but no such case: {key}"));
        }
    }
    let accepted = cases.values().filter(|v| **v == Verdict::Accepted).count();
    println!(
        "{fixtures} fixtures, {examples} docs examples: {accepted} of {} accepted",
        cases.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
