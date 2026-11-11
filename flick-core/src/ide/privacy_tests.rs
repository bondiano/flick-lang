//! Private file modules stay useful to their owner, but not to a dependent.

use super::tests::{Fixture, fixture_with_dependencies};
use crate::diagnostics::codes;
use crate::project::config::{Dependency, Source};
use crate::source::{FileId, Span};

fn fixture(main: &str) -> Fixture {
    fixture_with_dependencies(
        &[
            (
                "/lib/flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n",
            ),
            (
                "/lib/src/Lib/Hidden.fl",
                "private module Lib.Hidden exposing (seed, foreign_secret, ForeignNamed)\n\
                 let seed = 1\n\
                 foreign_secret : Int\nlet foreign_secret = 42\n\
                 class ForeignNamed a =\n  foreign_name : a -> String\n",
            ),
            (
                "/lib/src/Lib/Facade.fl",
                "module Lib.Facade exposing (seed, foreign_secret, ForeignNamed)\n\
                 import Lib.Hidden exposing (seed, foreign_secret, ForeignNamed)\n",
            ),
            (
                "/lib/src/Lib/Loader.fl",
                "module Lib.Loader exposing (seed)\nimport Lib.Facade exposing (seed)\n",
            ),
            (
                "src/App/Hidden.fl",
                "private module App.Hidden exposing (seed, local_secret, LocalNamed)\n\
                 let seed = 1\n\
                 local_secret : Int\nlet local_secret = 7\n\
                 class LocalNamed a =\n  local_name : a -> String\n",
            ),
            (
                "src/App/Facade.fl",
                "module App.Facade exposing (seed)\nimport App.Hidden exposing (seed)\n",
            ),
            ("src/App/Main.fl", main),
        ],
        vec![Dependency {
            name: "lib".into(),
            source: Source::Path("/lib".into()),
            span: Span::new(FileId::default(), 0, 0),
        }],
    )
}

const HEADER: &str = "module App.Main exposing (..)\n\
                     import Lib.Loader\nimport App.Facade\n";

#[test]
fn private_module_auto_imports_respect_package_ownership() {
    for (prefix, name, path) in [
        ("foreign_se", "foreign_secret", "Lib.Facade"),
        ("local_se", "local_secret", "App.Hidden"),
    ] {
        let main = format!("{HEADER}let answer = {prefix}\n");
        let f = fixture(&main);
        let ide = f.ide();
        assert!(ide.interface("Lib.Hidden").expect("loaded").private);
        assert!(ide.interface("App.Hidden").expect("loaded").private);
        let file = f.file("App.Main");
        let items = ide.complete(file, f.at("App.Main", prefix, 0) + 2, false);
        let matches: Vec<_> = items.iter().filter(|c| c.label == name).collect();
        assert_eq!(matches.len(), 1, "{items:?}");
        let qualified = format!("{path}.{name}");
        assert_eq!(matches[0].snippet.as_deref(), Some(qualified.as_str()));
        assert_eq!(matches[0].edits.len(), 1);
        assert_eq!(matches[0].edits[0].replacement, format!("\nimport {path}"));
        assert!(
            !items
                .iter()
                .any(|c| c.edits.iter().any(|e| e.replacement.contains("Lib.Hidden")))
        );
    }
}

#[test]
fn private_module_class_import_fixes_respect_package_ownership() {
    for (name, owner) in [("ForeignNamed", "Lib.Facade"), ("LocalNamed", "App.Hidden")] {
        let main = format!("{HEADER}type Box a =\n  | {name} a => Box a : Box a\n");
        let f = fixture(&main);
        let file = f.file("App.Main");
        let at = f.at("App.Main", name, 0);
        let ide = f.ide();
        assert!(
            ide.analysis
                .diagnostics
                .iter()
                .any(|d| d.code == codes::T001)
        );
        let imports: Vec<_> = ide
            .actions(file, at, at)
            .into_iter()
            .filter(|a| a.title.starts_with(&format!("import `{name}`")))
            .collect();
        assert_eq!(imports.len(), 1, "{imports:?}");
        assert_eq!(imports[0].title, format!("import `{name}` from `{owner}`"));
        assert_eq!(imports[0].edits.len(), 1);
        let replacement = &imports[0].edits[0].replacement;
        assert!(
            replacement.contains(&format!(" exposing ({name})")),
            "{replacement}"
        );
        assert!(!replacement.contains("Lib.Hidden"), "{replacement}");
    }
}

#[test]
fn private_module_qualified_completion_respects_package_ownership() {
    for (path, visible) in [
        ("Lib.Hidden", false),
        ("App.Hidden", true),
        ("Lib.Facade", true),
    ] {
        let main = format!("{HEADER}let answer = {path}.seed\n");
        let f = fixture(&main);
        let items = f.ide().complete(
            f.file("App.Main"),
            f.at("App.Main", &format!("{path}.seed"), 0)
                + u32::try_from(path.len()).expect("short path"),
            true,
        );
        assert_eq!(
            items.iter().any(|c| c.label == "seed"),
            visible,
            "{path}: {items:?}"
        );
    }
}

#[test]
fn a_rejected_private_import_does_not_expose_completions() {
    let main =
        format!("{HEADER}import Lib.Hidden as Hidden exposing (..)\nlet answer = Hidden.seed\n");
    let f = fixture(&main);
    let ide = f.ide();
    assert!(
        ide.analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::R017)
    );
    let file = f.file("App.Main");
    let at = f.at("App.Main", "Hidden.seed", 0);
    assert!(ide.complete(file, at + 6, true).is_empty());
    let items = ide.complete(file, at, false);
    assert!(
        !items
            .iter()
            .any(|c| c.module.as_deref() == Some("Lib.Hidden")),
        "{items:?}"
    );
}
