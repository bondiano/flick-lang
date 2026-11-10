//! Every ignored test is run by a recipe that can run it.
//!
//! `just e2e-tests` runs `--run-ignored only` over every test target of
//! `flick` and `flick-core` and excludes only what another recipe owns: the
//! cases of `tests/run`, on Flick's own runtime too (`just e2e-cases`,
//! sharded), the `PostgreSQL` suite (`just e2e-postgres`), the Conduit API
//! suite (`just conduit-api`) and the helpers that need an environment
//! variable or write the tree. The plugin
//! tests are `#[cfg(feature = "wasm-plugins")]`, so `just wasm-plugins` is
//! the recipe that builds and runs them; the reference model is `just model`
//! (`--include-ignored`), and `flick-pretty`'s timing probe is manual.
//!
//! This test holds those recipes to that mapping: an ignore attribute is a
//! test that runs somewhere, and a new one under `flick/tests/` is covered by
//! `just e2e-tests` the moment it lands. One that needs another runner has to
//! say so in the table below, and a runner that stops running it fails here.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// An ignore attribute, where it is, and the test it names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Ignored {
    package: String,
    target: String,
    name: String,
    file: String,
    line: usize,
}

impl Ignored {
    fn at(&self) -> String {
        format!("{}:{}", self.file, self.line)
    }

    fn what(&self) -> String {
        format!(
            "{}::{} ({}::{})",
            self.package,
            self.target,
            self.name,
            self.at()
        )
    }
}

/// Who runs an ignored test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Runner {
    /// `just e2e-tests`, selected by default.
    E2eTests,
    /// `just e2e-cases`, one shard of `tests/run` at a time.
    E2eCases,
    /// `just e2e-postgres`, which needs Docker.
    Postgres,
    /// `just conduit-api`, which needs Docker and hurl.
    Conduit,
    /// `just wasm-plugins`, which builds with the plugin feature.
    Plugins,
    /// `just model`, with `--include-ignored`.
    Model,
    /// No recipe: a probe or a generator, listed in `MANUAL` with its reason.
    Manual,
}

/// The ignored tests no recipe runs, and why; each must still exist.
const MANUAL: &[(&str, &str, &str)] = &[
    (
        "flick-core",
        "dump",
        "prints a tree for a file named by `DUMP`/`DUMP_FILE`",
    ),
    (
        "flick-core",
        "dump_explanations",
        "prints what the examples of each explanation report, for triage",
    ),
    (
        "flick-core",
        "write_fixes_of_built_in_signatures",
        "regenerates `packages/*/src`, a generator",
    ),
    (
        "flick-pretty",
        "printing_time_grows_linearly",
        "a timing probe; run it in release with `--nocapture`",
    ),
];

/// The recipe that runs each kind, and what its body must contain to do so.
const ANCHORS: &[(&str, &str)] = &[
    ("e2e-cases", "test(=run_cases)"),
    ("e2e-cases", "test(=rt::rt_cases)"),
    ("e2e-postgres", "--test sql_postgres"),
    ("conduit-api", "--test conduit"),
    ("wasm-plugins", "--test plugins"),
    ("model", "--include-ignored"),
];

/// The packages `just e2e-tests` selects.
const E2E_PACKAGES: &[&str] = &["flick", "flick-core"];

fn runner(test: &Ignored) -> Runner {
    match (
        test.package.as_str(),
        test.target.as_str(),
        test.name.as_str(),
    ) {
        // `tests/rt/` is a module of the `e2e` target, named here by its directory
        ("flick", "e2e", "run_cases") | ("flick", "rt", "rt_cases") => Runner::E2eCases,
        ("flick", "sql_postgres", _) => Runner::Postgres,
        ("flick", "conduit", _) => Runner::Conduit,
        ("flick", "plugins", _) => Runner::Plugins,
        ("flick-model", _, _) => Runner::Model,
        ("flick-core", _, "dump" | "dump_explanations" | "write_fixes_of_built_in_signatures")
        | ("flick-pretty", _, "printing_time_grows_linearly") => Runner::Manual,
        _ => Runner::E2eTests,
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate is in the repository")
        .to_path_buf()
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

/// The crate name a member's manifest declares.
fn crate_name(member: &Path) -> Option<String> {
    let manifest = fs::read_to_string(member.join("Cargo.toml")).ok()?;
    let line = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("name ="))?;
    Some(line.split('"').nth(1)?.to_owned())
}

/// The workspace members of the root manifest.
fn members(root: &Path) -> Vec<PathBuf> {
    let manifest = fs::read_to_string(root.join("Cargo.toml")).expect("a readable Cargo.toml");
    let start = manifest.find("members = [").expect("a members list");
    let rest = &manifest[start..];
    let end = rest.find(']').expect("a closed members list");
    rest[..end]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(|member| root.join(member))
        .collect()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if name != "target" && name != "node_modules" {
                rust_files(&path, out);
            }
        } else if Path::new(&name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
        {
            out.push(path);
        }
    }
}

/// The test target a file belongs to: `tests/name.rs` and `tests/name/…` are
/// the `name` integration test, and `src/…` is the crate's library target.
fn target_of(member: &Path, file: &Path) -> String {
    let rel = file.strip_prefix(member).expect("a file of the member");
    let mut parts = rel.components().map(|c| c.as_os_str().to_string_lossy());
    if parts.next().as_deref() == Some("tests") {
        let first = parts.next().expect("a file under tests/").to_string();
        return first.strip_suffix(".rs").unwrap_or(&first).to_owned();
    }
    crate_name(member).expect("a crate name")
}

/// The test a `#[ignore]` line introduces, if it is followed by one.
fn test_after(lines: &[&str], index: usize) -> Option<String> {
    lines[index..].iter().take(8).find_map(|line| {
        let at = line.find("fn ")?;
        let name: String = line[at + 3..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        (!name.is_empty() && name.starts_with(|c: char| c.is_alphabetic() || c == '_'))
            .then_some(name)
    })
}

fn ignored_tests(root: &Path) -> Vec<Ignored> {
    let mut all = Vec::new();
    for member in members(root) {
        let package = crate_name(&member).expect("a crate name");
        let mut files = Vec::new();
        rust_files(&member.join("src"), &mut files);
        rust_files(&member.join("tests"), &mut files);
        for file in files {
            let text = fs::read_to_string(&file).expect("a readable source");
            let lines: Vec<&str> = text.lines().collect();
            for (index, line) in lines.iter().enumerate() {
                if !line.trim_start().starts_with("#[ignore") {
                    continue;
                }
                let name = test_after(&lines, index).unwrap_or_else(|| {
                    panic!(
                        "{}:{}: an ignore attribute with no test beside it",
                        relative(root, &file),
                        index + 1
                    )
                });
                all.push(Ignored {
                    package: package.clone(),
                    target: target_of(&member, &file),
                    name,
                    file: relative(root, &file),
                    line: index + 1,
                });
            }
        }
    }
    all.sort();
    all
}

/// The recipe bodies of the justfile, by name, with continuations joined.
fn recipes(justfile: &str) -> BTreeMap<String, String> {
    let mut found: BTreeMap<String, String> = BTreeMap::new();
    let mut current: Option<(String, String)> = None;
    for line in justfile.lines() {
        let header = !line.starts_with([' ', '\t', '#', '['])
            && line.contains(':')
            && line
                .split(':')
                .next()
                .is_some_and(|head| head.chars().next().is_some_and(|c| c.is_ascii_alphabetic()));
        if header {
            if let Some((name, body)) = current.take() {
                found.insert(name, body);
            }
            let name = line
                .split([' ', ':'])
                .next()
                .expect("a recipe name")
                .to_owned();
            // the dependencies of a one-line recipe are on its header, as in
            // `e2e: e2e-js e2e-tests …`, and a recipe with a body has none
            let dependencies = line.split_once(": ").map_or("", |(_, rest)| rest);
            current = Some((name, format!("{dependencies} ")));
        } else if line.trim().is_empty() {
            if let Some((name, body)) = current.take() {
                found.insert(name, body);
            }
        } else if let Some((_, body)) = current.as_mut() {
            body.push_str(line.trim_end_matches('\\').trim());
            body.push(' ');
        }
    }
    if let Some((name, body)) = current {
        found.insert(name, body);
    }
    found
}

/// A term of a nextest filter expression that removes tests.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Term {
    /// `test(=NAME)`, the whole name.
    Exact(String),
    /// `test(~NAME)`, a part of it: a library test is named by its modules, as
    /// in `types::builtins::tests::fixes::write_fixes_of_built_in_signatures`,
    /// which the name of its function alone does not match.
    Part(String),
    /// `binary_id(PACKAGE::TARGET)`
    Binary(String, String),
}

impl Term {
    fn matches(&self, test: &Ignored) -> bool {
        match self {
            Term::Exact(name) => *name == test.name,
            Term::Part(name) => test.name.contains(name.as_str()),
            Term::Binary(package, target) => *package == test.package && *target == test.target,
        }
    }

    /// Whether this term can name a test of a library target, whose nextest
    /// name is its module path rather than the name of its function.
    fn names_a_library_test(&self) -> bool {
        matches!(self, Term::Part(_) | Term::Binary(_, _))
    }

    fn describe(&self) -> String {
        match self {
            Term::Exact(name) => format!("test(={name})"),
            Term::Part(name) => format!("test(~{name})"),
            Term::Binary(package, target) => format!("binary_id({package}::{target})"),
        }
    }
}

/// The exclusions of the `-E '…'` filter in `body`.
fn exclusions(body: &str) -> Vec<Term> {
    let start = body.find("-E '").expect("an `-E '…'` filter") + 4;
    let rest = &body[start..];
    let end = rest.find('\'').expect("a closed `-E` filter");
    let filter = &rest[..end];
    let mut terms = Vec::new();
    for (marker, exact) in [
        ("test(=", Some(true)),
        ("test(~", Some(false)),
        ("binary_id(", None),
    ] {
        let mut from = 0;
        while let Some(at) = filter[from..].find(marker) {
            let at = from + at + marker.len();
            let close = filter[at..].find(')').expect("a closed term");
            let inner = filter[at..at + close].trim();
            terms.push(match exact {
                None => {
                    let (package, target) = inner.split_once("::").expect("`package::target`");
                    Term::Binary(package.to_owned(), target.to_owned())
                }
                Some(true) => Term::Exact(inner.to_owned()),
                Some(false) => Term::Part(inner.to_owned()),
            });
            from = at + close;
        }
    }
    terms
}

/// `e2e-tests` selects every test of its packages: no list of targets that a
/// new suite can be left out of.
fn assert_selection(body: &str) {
    assert!(
        body.contains("--run-ignored only"),
        "`e2e-tests` must run the ignored tests: {}",
        body.trim()
    );
    assert!(
        !body.contains("--test "),
        "`e2e-tests` must select its targets by what it excludes, not by a list that a new \
         suite is missing from: {}",
        body.trim()
    );
    for package in E2E_PACKAGES {
        assert!(
            body.contains(&format!("-p {package}")),
            "`e2e-tests` must select the package `{package}`: {}",
            body.trim()
        );
    }
}

/// Each recipe that owns ignored tests still runs them.
fn assert_anchors(found: &BTreeMap<String, String>) {
    for (recipe, anchor) in ANCHORS {
        let body = found.get(*recipe).unwrap_or_else(|| {
            panic!("`{recipe}` runs some of the ignored tests, and is missing from the justfile")
        });
        assert!(
            body.contains(anchor),
            "`{recipe}` must contain `{anchor}`, which runs the ignored tests it owns: {}",
            body.trim()
        );
    }
}

/// Each ignored test belongs to one runner, and the selection matches that:
/// an excluded test is owned elsewhere, and a test owned by `e2e-tests` is
/// not excluded.
fn assert_classification(tests: &[Ignored], terms: &[Term]) {
    for test in tests {
        let excluded = terms.iter().find(|term| term.matches(test));
        match runner(test) {
            Runner::E2eTests => {
                assert!(
                    E2E_PACKAGES.contains(&test.package.as_str()),
                    "{}: an ignored test of package `{}`, which no recipe selects; classify it \
                     in `runner` with the recipe that runs it",
                    test.what(),
                    test.package
                );
                assert_eq!(
                    excluded.map(Term::describe),
                    None,
                    "{}: run by `just e2e-tests` by default, and excluded from it",
                    test.what()
                );
            }
            Runner::Manual => {
                assert!(
                    MANUAL
                        .iter()
                        .any(|(package, name, _)| *package == test.package && *name == test.name),
                    "{}: manual, with no line in `MANUAL` saying why",
                    test.what()
                );
                // inside the packages `e2e-tests` selects, an exclusion is
                // what keeps a helper that needs an environment variable from
                // running there; outside them it could only hide a test
                if E2E_PACKAGES.contains(&test.package.as_str()) {
                    assert!(
                        excluded.is_some(),
                        "{}: manual, and `e2e-tests` runs the ignored tests of `{}`, so it \
                         would run this one too",
                        test.what(),
                        test.package
                    );
                    if test.target == test.package {
                        let term = excluded.expect("excluded, as asserted above");
                        assert!(
                            term.names_a_library_test(),
                            "{}: nextest names a library test by its modules, so `{}` does \
                             not exclude it: use `test(~…)`",
                            test.what(),
                            term.describe()
                        );
                    }
                } else {
                    assert_eq!(
                        excluded.map(Term::describe),
                        None,
                        "{}: manual, in a package `e2e-tests` does not select, and excluded \
                         from it anyway",
                        test.what()
                    );
                }
            }
            Runner::E2eCases | Runner::Postgres | Runner::Conduit | Runner::Plugins => {
                assert!(
                    excluded.is_some(),
                    "{}: owned by another recipe, and `e2e-tests` does not exclude it",
                    test.what()
                );
            }
            Runner::Model => assert!(
                excluded.is_none(),
                "{}: the model runs its own ignored tests, and `e2e-tests` excludes it anyway",
                test.what()
            ),
        }
    }
}

/// A term that matches nothing is a stale exception: it hides whatever a
/// later test of that name would be. A manual entry that names no test is a
/// note about a test that is gone.
fn assert_live(tests: &[Ignored], terms: &[Term]) {
    for term in terms {
        let matches: Vec<&Ignored> = tests.iter().filter(|test| term.matches(test)).collect();
        assert!(
            !matches.is_empty(),
            "`{}` excludes no ignored test: remove it, or name what it is for",
            term.describe()
        );
        for test in matches {
            assert_ne!(
                runner(test),
                Runner::E2eTests,
                "{}: excluded from `e2e-tests` by `{}`, and owned by no other recipe",
                test.what(),
                term.describe()
            );
        }
    }
    for (package, name, reason) in MANUAL {
        assert!(
            tests
                .iter()
                .any(|test| test.package == *package && test.name == *name),
            "`MANUAL` lists `{package}::{name}` ({reason}), which no longer exists"
        );
    }
}

#[test]
fn every_ignored_test_is_run_by_the_recipe_that_can_run_it() {
    let root = root();
    let found = recipes(&fs::read_to_string(root.join("justfile")).expect("a readable justfile"));
    let tests = ignored_tests(&root);
    assert!(!tests.is_empty(), "no ignore attributes found");

    let body = found.get("e2e-tests").expect("a `e2e-tests` recipe");
    assert_selection(body);
    assert_anchors(&found);
    let terms = exclusions(body);
    assert_classification(&tests, &terms);
    assert_live(&tests, &terms);
}

#[test]
fn the_e2e_recipes_are_reachable_from_one_command() {
    let root = root();
    let found = recipes(&fs::read_to_string(root.join("justfile")).expect("a readable justfile"));
    let body = found.get("e2e").expect("a `just e2e` recipe");
    for recipe in ["e2e-js", "e2e-tests", "e2e-cases", "go-test", "language"] {
        assert!(
            body.contains(recipe),
            "`just e2e` must include `{recipe}`: {}",
            body.trim()
        );
    }
}
