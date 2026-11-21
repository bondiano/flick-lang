//! Parsing recipes from `justfile` (including `import`ed files) and nextest
//! exclusion filters from `e2e-tests`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use super::Ignored;

/// The recipe bodies of `justfile` and any `import '...'` files it pulls in,
/// by name, with continuations joined.
pub(super) fn load_recipes(root: &Path) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    let mut queue = vec![root.join("justfile")];
    let mut seen = BTreeSet::new();
    while let Some(path) = queue.pop() {
        if !seen.insert(path.clone()) {
            continue;
        }
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("a readable justfile at {}: {err}", path.display()));
        let dir = path.parent().expect("a justfile sits in a directory");
        for line in text.lines() {
            let trimmed = line.trim();
            let rest = trimmed
                .strip_prefix("import ")
                .or_else(|| trimmed.strip_prefix("import? "));
            if let Some(rest) = rest {
                let imported = rest.trim().trim_matches(['\'', '"']);
                if !imported.is_empty() {
                    queue.push(dir.join(imported));
                }
            }
        }
        found.extend(recipes(&text));
    }
    found
}

/// The recipe bodies of one justfile, by name, with continuations joined.
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
pub(super) enum Term {
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
    pub(super) fn matches(&self, test: &Ignored) -> bool {
        match self {
            Term::Exact(name) => *name == test.name,
            Term::Part(name) => test.name.contains(name.as_str()),
            Term::Binary(package, target) => *package == test.package && *target == test.target,
        }
    }

    /// Whether this term can name a test of a library target, whose nextest
    /// name is its module path rather than the name of its function.
    pub(super) fn names_a_library_test(&self) -> bool {
        matches!(self, Term::Part(_) | Term::Binary(_, _))
    }

    pub(super) fn describe(&self) -> String {
        match self {
            Term::Exact(name) => format!("test(={name})"),
            Term::Part(name) => format!("test(~{name})"),
            Term::Binary(package, target) => format!("binary_id({package}::{target})"),
        }
    }
}

/// The exclusions of the `-E '…'` filter in `body`.
pub(super) fn exclusions(body: &str) -> Vec<Term> {
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
