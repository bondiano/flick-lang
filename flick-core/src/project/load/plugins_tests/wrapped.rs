//! A type a plugin declares over another: a type of its own to the checker,
//! the other's values when the program runs, and a constructor only the
//! plugin's code can name.

use std::path::PathBuf;

use super::{LIBRARY, host, library, report, roots, show};
use crate::ir;
use crate::project::config::{Dependency, Source};
use crate::project::load::{check, compile};
use crate::source::{FileId, SourceMap, Span};
use crate::types::Rep;

const HEAD: &str =
    "module App.Main exposing (..)\n\nimport Pairs exposing (plugin wrapped)\n\nwrapped Meters\n\n";

#[test]
fn a_type_a_plugin_wraps_is_its_own_and_costs_nothing() {
    let main = format!(
        "{HEAD}length : Meters\nlet length = Meters.make 3.0\n\nmetres : Float\nlet metres = Meters.value length\n"
    );
    assert_eq!(report(&main), [""; 0]);
    // the constructor is unboxed in the code built
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let (_, compilation) = compile(&host(&main), &mut sources, Some(&project), &roots());
    let made = (compilation.modules[0].ir.items.iter()).find_map(|item| match &item.kind {
        ir::ItemKind::Constructor(c) => Some((c.name.as_str(), c.rep.clone())),
        _ => None,
    });
    assert_eq!(made, Some(("Meters$Make", Rep::Unboxed)));
    // a `Float` is no `Meters`, and a `Meters` no `Float`
    let mixed = format!("{HEAD}length : Meters\nlet length = 3.0\n");
    assert_eq!(report(&mixed).len(), 1, "{:?}", report(&mixed));
    let back = format!("{HEAD}sum : Float\nlet sum = Meters.make 3.0 + 1.0\n");
    assert!(!report(&back).is_empty());
}

#[test]
fn the_constructor_of_a_wrapped_type_is_not_a_name_of_the_user() {
    let main = format!("{HEAD}length : Meters\nlet length = Meters$Make 3.0\n");
    assert_eq!(report(&main)[0], "FLICK-S007 `$`: unexpected `$`");
}

/// What checking `main` finds when the package `twin` is a dependency too:
/// its plugin has the heads of `pairs` under a name of its own.
fn report_with_twin(main: &str) -> Vec<String> {
    let mut sources = SourceMap::default();
    let mut project = library(&mut sources);
    project.config.dependencies.push(Dependency {
        name: "twin".into(),
        source: Source::Path("../twin".into()),
        span: Span::new(FileId::default(), 0, 0),
    });
    let host = host(main);
    host.write("/twin/flick.toml", LIBRARY);
    host.write(
        "/twin/src/Twin.fl",
        "module Twin exposing (t)\n\nt : Float\nlet t = 1.0\n",
    );
    let found = check(&host, &mut sources, Some(&project), &roots());
    found.iter().map(|d| show(&sources, d)).collect()
}

#[test]
fn the_constructor_of_a_wrapped_type_is_its_plugins_alone() {
    // the plugin that declared the type names its constructor
    let own = "module App.Main exposing (..)\n\nimport Pairs exposing (plugin wrapped, plugin forged)\n\nwrapped Meters\n\nmade : Meters\nlet made = forged Meters\n";
    assert_eq!(report_with_twin(own), [""; 0]);
    // the code of another plugin does not
    let other = "module App.Main exposing (..)\n\nimport Pairs exposing (plugin wrapped)\nimport Twin exposing (plugin forged)\n\nwrapped Meters\n\nmade : Meters\nlet made = forged Meters\n";
    assert_eq!(
        report_with_twin(other),
        [
            "FLICK-P036 `forged Meters`: plugin `twin` gave code that cannot be placed: it names `Meters$Make`, which only the code of plugin `pairs` may name"
        ]
    );
}

#[test]
fn a_wrapped_type_leaves_its_module_without_its_constructor() {
    // `(..)` exports the type and its companions, not the constructor
    let host = host(HEAD);
    host.write(
        "/p/src/App/Use.fl",
        "module App.Use exposing (made, back)\n\nimport App.Main exposing (Meters(..))\nimport Pairs exposing (plugin forged)\n\nback : Float\nlet back = Meters.value (Meters.make 2.0)\n\nmade : Meters\nlet made = forged Meters\n",
    );
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let roots = [
        PathBuf::from("/p/src/App/Main.fl"),
        PathBuf::from("/p/src/App/Use.fl"),
    ];
    let found = check(&host, &mut sources, Some(&project), &roots);
    let found: Vec<String> = found.iter().map(|d| show(&sources, d)).collect();
    assert_eq!(
        found,
        [
            "FLICK-R005 `Meters(..)`: `Meters` is opaque: `App.Main` does not export its constructors",
            "FLICK-T001 `forged Meters`: cannot find constructor `Meters$Make`"
        ]
    );
    // and a module cannot export it by name
    let named = HEAD.replace("exposing (..)", "exposing (Meters(..))");
    assert_eq!(
        report(&named),
        ["FLICK-R011 `Meters(..)`: opaque type `Meters` cannot export its constructors"]
    );
}
