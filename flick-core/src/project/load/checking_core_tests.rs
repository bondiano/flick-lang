use super::*;
use crate::syntax;

fn loaded(source: &str, broken: bool) -> Loaded {
    let mut sources = SourceMap::default();
    let file = sources.add("A/Parent.fl", source);
    let parse = syntax::parse(source, file).into_module();
    let syntax = resolve::module_syntax(&parse.tree, source, file);
    let diagnostics = if broken {
        vec![Diagnostic::error(
            codes::S007,
            "unexpected `)`",
            Span::new(file, 6, 7),
        )]
    } else {
        Vec::new()
    };
    Loaded {
        path: PathBuf::from("A/Parent.fl"),
        file,
        root: Root::Other(None),
        parse: Arc::new(Parse {
            tree: parse.tree,
            diagnostics,
        }),
        syntax: Arc::new(syntax),
        name: "A.Parent".into(),
        expansion: None,
        editing: Vec::new(),
        loader: None,
        virtual_of: None,
        member_of: None,
        scope_of: None,
    }
}

/// A template fallback has no structure to export, so it must not invent
/// names; it marks the module incomplete exactly when parsing failed.
#[test]
fn the_template_fallback_is_incomplete_only_when_the_parser_reported() {
    let source = "module A.Parent exposing (x)\nlet x = 1\n";
    let broken = Checking::module_template(&loaded(source, true));
    assert!(broken.exported.incomplete);
    assert!(!broken.interface.incomplete);
    assert!(broken.exported.exports.is_empty());
    assert!(broken.exported.parameterized.is_none());
    assert_eq!(broken.diagnostics.len(), 1);
    assert_eq!(broken.diagnostics[0].code, codes::S007);

    let clean = Checking::module_template(&loaded(source, false));
    assert!(!clean.exported.incomplete);
    assert!(!clean.interface.incomplete);
    assert!(clean.exported.exports.is_empty());
    assert!(clean.diagnostics.is_empty());
}
