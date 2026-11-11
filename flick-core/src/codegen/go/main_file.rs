//! `main.go` of a program: the runtime, the optional packages it reaches and
//! Go's zone database when it names a zone.

use std::collections::HashSet;

use crate::effects::bindings;
use flick_pretty::{Document, DocumentArena, docvec};

use super::RUNTIME_PACKAGE;
use super::layout::{self, block, statement};
use crate::ir::{ItemKind, Module};

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
            if f.binding.from.as_deref() == Some(module) && exports.contains(&f.binding.name.as_str()))
    })
}
