//! `main.go` of a program: the runtime, the optional packages it reaches and
//! Go's zone database when it names a zone.

use std::collections::{BTreeSet, HashMap, HashSet};

use flick_backend_support::effects::bindings;
use flick_backend_support::order::post_order;
use flick_backend_support::reach::{self, Key};
use flick_pretty::{Document, DocumentArena, docvec};

use super::layout::{self, block, statement};
use super::{RUNTIME_PACKAGE, Unsupported, names};
use flick_ir::{ItemKind, Module};

/// The optional packages of the runtime (`PACKAGES`) and the bindings that
/// need one: a program that reaches a binding imports the package, which
/// registers itself with the runtime, and its Go module requires the
/// modules of the package.
pub const OPTIONAL: &[(&str, &str, &[&str])] = bindings::GO_OPTIONAL;

/// The optional packages `reached` reaches.
pub fn packages(modules: &[&Module], reached: &HashSet<(usize, usize)>) -> Vec<&'static str> {
    OPTIONAL
        .iter()
        .filter(|(_, module, exports)| reaches(modules, reached, module, exports))
        .map(|&(package, _, _)| package)
        .collect()
}

/// Whether `reached` looks a named time zone up.
pub fn zones(modules: &[&Module], reached: &HashSet<(usize, usize)>) -> bool {
    reaches(
        modules,
        reached,
        bindings::ZONE_MODULE,
        bindings::ZONE_EXPORTS,
    )
}

/// `main.go`: imports the runtime, the optional `packages` and with `zones`
/// Go's zone database, and has `run` run the program.
pub fn main_go<'a>(
    a: &'a DocumentArena<'a>,
    run: Document<'a>,
    packages: &[&'a str],
    zones: bool,
) -> String {
    let import_packages = a.concat(
        packages
            .iter()
            .map(|package| docvec![a, "import _ \"flick.app/", *package, "\"", a.lines(2)]),
    );
    let (import_zones, use_zones) = if zones {
        (
            docvec![a, "import _ \"time/tzdata\"", a.lines(2)],
            Some(statement(a, "flickrt.UseZones()")),
        )
    } else {
        (a.empty(), None)
    };
    layout::print(docvec![
        a,
        "package main",
        a.lines(2),
        "import \"",
        RUNTIME_PACKAGE,
        "\"",
        a.lines(2),
        import_packages,
        import_zones,
        "func main() ",
        block(a, use_zones.into_iter().chain([statement(a, run)])),
        a.line()
    ])
}

/// Whether the program reaches a binding of `exports` of the foreign module
/// `module`.
fn reaches(
    modules: &[&Module],
    reached: &HashSet<(usize, usize)>,
    module: &str,
    exports: &[&str],
) -> bool {
    reached.iter().any(|&(m, i)| {
        matches!(&modules[m].items[i].kind, ItemKind::Foreign(f)
            if f.binding.module_export().is_some_and(|(from, name)| from == module && exports.contains(&name)))
    })
}

/// Lexical module references may form a module-level cycle while their
/// value-initialization graph is acyclic. Schedule that graph directly.
pub(super) fn initialization_order(
    modules: &[&Module],
    index: &HashMap<Key, (usize, usize)>,
    reached: &HashSet<(usize, usize)>,
) -> Vec<(usize, usize)> {
    fn visit(
        at: (usize, usize),
        modules: &[&Module],
        index: &HashMap<Key, (usize, usize)>,
        reached: &HashSet<(usize, usize)>,
        seen: &mut HashSet<(usize, usize)>,
        out: &mut Vec<(usize, usize)>,
    ) {
        if !seen.insert(at) {
            return;
        }
        let mut used = Vec::new();
        reach::uses(&modules[at.0].items[at.1], &mut used);
        for key in used {
            if let Some(&next) = index.get(&key)
                && reached.contains(&next)
            {
                visit(next, modules, index, reached, seen, out);
            }
        }
        out.push(at);
    }
    let mut items: Vec<_> = reached.iter().copied().collect();
    items.sort_unstable();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for at in items {
        visit(at, modules, index, reached, &mut seen, &mut out);
    }
    out
}

/// The calls initializing the modules `roots` read, each after those it reads.
pub(super) fn module_inits<'a>(
    a: &'a DocumentArena<'a>,
    roots: &[String],
    deps: &HashMap<String, BTreeSet<String>>,
) -> Vec<Document<'a>> {
    let mut order = Vec::new();
    let mut visited = HashSet::new();
    for root in roots {
        init_order(root, deps, &mut visited, &mut order);
    }
    order
        .iter()
        .filter(|m| deps.contains_key(*m))
        .map(|m| statement(a, docvec![a, names::init(m), "()"]))
        .collect()
}

fn init_order(
    module: &str,
    deps: &HashMap<String, BTreeSet<String>>,
    visited: &mut HashSet<String>,
    out: &mut Vec<String>,
) {
    if !visited.insert(module.to_owned()) {
        return;
    }
    for dep in deps.get(module).into_iter().flatten() {
        init_order(dep, deps, visited, out);
    }
    out.push(module.to_owned());
}

/// The reached items of a module in an order where everything an item uses
/// in the module is initialized first.
pub(super) fn item_order(
    module: &Module,
    items: &[usize],
    index: &HashMap<Key, (usize, usize)>,
) -> Vec<usize> {
    let mut uses = |i: usize| {
        let mut used = Vec::new();
        reach::uses(&module.items[i], &mut used);
        (used.iter())
            .filter(|(m, _)| *m == module.name)
            .filter_map(|key| index.get(key).map(|&(_, j)| j))
            .collect()
    };
    post_order(module.items.len(), items.iter().copied(), &mut uses)
}

/// The same message at the same place once.
pub(super) fn dedup(diagnostics: &mut Vec<Unsupported>) {
    let mut seen = HashSet::new();
    diagnostics.retain(|d| seen.insert(format!("{:?}{}", d.span, d.message)));
}
