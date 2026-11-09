use std::collections::HashMap;

mod fd_coherence;
mod fd_resolution;

use super::Dict;
use crate::diagnostics::Diagnostic;
use crate::iface::{self, Interface};
use crate::resolve::{self, Module};
use crate::source::SourceMap;
use crate::syntax;
use crate::types::{Limits, Options, Thresholds, Watched, builtins, check_with_evidence};

/// Checks `modules` in order, each seeing the interfaces of those before;
/// returns the diagnostics and dictionaries of the last one.
fn check_all(modules: &[(&str, &str)]) -> (Vec<Diagnostic>, Vec<Dict>) {
    let mut done: HashMap<String, Interface> = HashMap::new();
    let mut last = (Vec::new(), Vec::new());
    for (index, (name, text)) in modules.iter().enumerate() {
        let mut sources = SourceMap::default();
        let file = sources.add(format!("{name}.fl"), *text);
        let parse = syntax::parse(text, file);
        let syntax = resolve::module_syntax(&parse.tree, text, file);
        let module = Module {
            name,
            file,
            text,
            tree: &parse.tree,
            syntax: &syntax,
            generated: resolve::NO_GENERATED,
        };
        let imported = |m: &str| done.get(m).cloned();
        let (mut interface, mut diagnostics) = resolve::resolve(&module, &imported);
        diagnostics.extend(parse.diagnostics);
        let local = |_: &str| true;
        let options = Options {
            local: &local,
            limits: Limits::default(),
            no_let_assert: false,
            thresholds: Thresholds::default(),
            effects: true,
            watched: Watched::none(),
        };
        let (more, evidence) =
            check_with_evidence(&module, &imported, builtins(), options, &mut interface);
        diagnostics.extend(more);
        diagnostics.retain(|d| !d.unnecessary());
        if index + 1 < modules.len() {
            assert!(diagnostics.is_empty(), "{name}: {diagnostics:?}");
            let encoded = iface::write(&interface);
            let decoded = iface::read(encoded.as_bytes(), name).expect("interface round trip");
            assert_eq!(decoded, interface);
            done.insert((*name).to_owned(), decoded);
        }
        last = (diagnostics, evidence.into_iter().map(|(_, d)| d).collect());
    }
    last
}

const SHAPES: (&str, &str) = (
    "App.Shapes",
    "module App.Shapes exposing (Shape(..))\n\
     type Shape = Circle Float\n  deriving Eq, Show\n",
);

const PRETTY: (&str, &str) = (
    "App.Pretty",
    "module App.Pretty exposing (Pretty(..))\n\
     import App.Shapes exposing (Shape(..))\n\
     class Show a => Pretty a =\n  pretty : a -> String\n\
     instance Pretty Shape =\n  pretty : Shape -> String\n  let pretty s = show s\n",
);

const USES: &str = "texts : List String\n\
     let texts = [show (Circle 1), pretty (Circle 2), show [Circle 3]]\n";

#[test]
fn instances_do_not_depend_on_the_order_or_form_of_imports() {
    let mains = [
        "module App.Main exposing (texts)\n\
         import App.Shapes exposing (Shape(..))\n\
         import App.Pretty exposing (Pretty(..))\n",
        "module App.Main exposing (texts)\n\
         import App.Pretty as P exposing (pretty)\n\
         import App.Shapes exposing (Circle)\n",
        // the instances of `App.Shapes` come through `App.Pretty` alone
        "module App.Main exposing (texts)\n\
         import App.Pretty exposing (..)\n\
         import App.Shapes as S exposing (Shape(..))\n",
    ];
    let results: Vec<_> = mains
        .iter()
        .map(|header| {
            let main = format!("{header}{USES}");
            check_all(&[SHAPES, PRETTY, ("App.Main", &main)])
        })
        .collect();
    let (diagnostics, dicts) = &results[0];
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(dicts.len(), 3, "{dicts:?}");
    assert!(
        matches!(&dicts[0], Dict::Instance { module, name, .. } if module == "App.Shapes" && name == "Show Shape")
    );
    for other in &results[1..] {
        assert_eq!(other, &results[0]);
    }
}

#[test]
fn an_instance_is_seen_through_the_modules_that_depend_on_it() {
    let middle = (
        "App.Middle",
        "module App.Middle exposing (Pretty(..))\nimport App.Pretty exposing (Pretty(..))\n",
    );
    let main = "module App.Main exposing (text)\n\
                import App.Middle exposing (Pretty(..))\n\
                text : String\nlet text = pretty 1\n";
    let (diagnostics, _) = check_all(&[SHAPES, PRETTY, middle, ("App.Main", main)]);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0]
            .message
            .contains("no instance for `Pretty Int`"),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics[0].message.contains("`Pretty Shape`"),
        "{diagnostics:?}"
    );
}
