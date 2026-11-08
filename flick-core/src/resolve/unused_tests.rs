use crate::source::SourceMap;

/// `text` after the first fix of the first diagnostic with `code`.
#[cfg(feature = "diagnostic-fixes")]
fn fixed(text: &str, code: &str) -> String {
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.to_owned());
    let diagnostics = crate::check(&sources, file);
    let d = diagnostics
        .iter()
        .find(|d| d.code.to_string() == code)
        .unwrap_or_else(|| panic!("no {code} in {diagnostics:?}"));
    let mut out = text.to_owned();
    for edit in d.fixes[0].edits.iter().rev() {
        out.replace_range(
            edit.span.start as usize..edit.span.end as usize,
            &edit.replacement,
        );
    }
    out
}

const HEAD: &str = "module Probe exposing (f)\n";

#[cfg(feature = "diagnostic-fixes")]
#[test]
fn removing_an_import_item_keeps_the_list_well_formed() {
    let body = "f : Float -> Float\nlet f x = abs x\n";
    let cases = [
        (
            "import Float exposing (abs, floor)\n",
            "import Float exposing (abs)\n",
        ),
        (
            "import Float exposing (floor, abs)\n",
            "import Float exposing (abs)\n",
        ),
        (
            "import Float exposing (abs, floor, ceil)\n",
            "import Float exposing (abs, ceil)\n",
        ),
    ];
    for (import, want) in cases {
        let text = format!("{HEAD}{import}{body}");
        assert_eq!(fixed(&text, "FLICK-R012"), format!("{HEAD}{want}{body}"));
    }
    let text = format!(
        "{HEAD}import Float exposing (floor)\n{}",
        body.replace("abs x", "Float.abs x")
    );
    assert_eq!(
        fixed(&text, "FLICK-R012"),
        format!(
            "{HEAD}import Float\n{}",
            body.replace("abs x", "Float.abs x")
        )
    );
}

#[cfg(feature = "diagnostic-fixes")]
#[test]
fn removing_an_unused_import_or_value_deletes_its_lines() {
    let text = format!("{HEAD}import String\nf : Float -> Float\nlet f x = x\n");
    assert_eq!(
        fixed(&text, "FLICK-R012"),
        format!("{HEAD}f : Float -> Float\nlet f x = x\n")
    );
    let text = format!(
        "{HEAD}f : Float -> Float\nlet f x = x\n\ng : Float\nlet g = 1\n\nh : Float\nlet h = 2\n"
    );
    assert_eq!(
        fixed(&text, "FLICK-R014"),
        format!("{HEAD}f : Float -> Float\nlet f x = x\n\nh : Float\nlet h = 2\n")
    );
}

#[cfg(feature = "diagnostic-fixes")]
#[test]
fn unused_value_can_be_exposed_without_removing_it() {
    for (head, want) in [
        ("module Probe exposing (f)", "module Probe exposing (f, g)"),
        ("module Probe exposing ()", "module Probe exposing (g)"),
    ] {
        let text = format!("{head}\nf : Int\nlet f = 1\ng : Int\nlet g = 2\n");
        let mut sources = SourceMap::default();
        let file = sources.add("Probe.fl", text.clone());
        let diagnostics = crate::check(&sources, file);
        let unused = diagnostics
            .iter()
            .find(|d| d.code.to_string() == "FLICK-R014" && d.message.contains("`g`"))
            .expect("unused g");
        let fix = unused
            .fixes
            .iter()
            .find(|f| f.message.contains("expose `g`"))
            .expect("expose action");
        let mut changed = text.clone();
        for edit in fix.edits.iter().rev() {
            changed.replace_range(
                edit.span.start as usize..edit.span.end as usize,
                &edit.replacement,
            );
        }
        assert!(changed.starts_with(want), "{changed}");
    }
}

#[cfg(feature = "diagnostic-fixes")]
#[test]
fn an_unused_variable_is_renamed_with_an_underscore() {
    let text = format!("{HEAD}f : (Float, Float) -> Float\nlet f (x, y) = x\n");
    assert!(fixed(&text, "FLICK-R013").contains("let f (x, _y) = x"));
    let text = format!("{HEAD}f : {{ a : Float; b : Float }} -> Float\nlet f {{ a; b }} = a\n");
    assert!(fixed(&text, "FLICK-R013").contains("let f { a; b = _b } = a"));
}

#[test]
fn unused_warnings_keep_their_primary_diagnostics() {
    use crate::diagnostics::{Severity, codes};
    let text = format!(
        "{HEAD}import String\nf : Int -> Int\nlet f unused = 1\nprivate : Int\nlet private = 2\n"
    );
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.clone());
    let diagnostics = crate::check(&sources, file);
    for (code, name) in [
        (codes::R012, "String"),
        (codes::R013, "unused"),
        (codes::R014, "private"),
    ] {
        let d = diagnostics
            .iter()
            .find(|d| d.code == code)
            .unwrap_or_else(|| panic!("{diagnostics:?}"));
        assert_eq!(d.severity, Severity::Warning);
        assert!(d.unnecessary());
        let span = d.span.expect("unused binding location");
        assert_eq!(span.file, file);
        assert_eq!(&text[span.range()], name);
    }
}

#[cfg(feature = "diagnostic-fixes")]
#[test]
fn an_import_of_a_head_of_std_is_needless_unless_a_block_needs_it() {
    let head = "module Probe exposing (tool)\n";
    let body = "tool : Cli.Command none none\nlet tool = cli \"tool\" { handle fun {} -> Platform.stdout_line \"hi\" }\n";
    // `Cli.Command` uses the import, and `Codec` brings instances
    let text = format!("{head}import Cli exposing (plugin cli)\n{body}");
    assert_eq!(
        fixed(&text, "FLICK-R012"),
        format!("{head}import Cli\n{body}")
    );
    let text = format!("{head}import Codec exposing (plugin codec)\n{body}");
    assert_eq!(
        fixed(&text, "FLICK-R012"),
        format!("{head}import Codec\n{body}")
    );
    let typed = body.replace("Cli.Command", "Command");
    let text = format!("{head}import Cli exposing (Command, plugin cli)\n{typed}");
    assert_eq!(
        fixed(&text, "FLICK-R012"),
        format!("{head}import Cli exposing (Command)\n{typed}")
    );
    // a block without `{` is one only because of the import
    let text = format!(
        "{head}import Cli exposing (plugin cli)\n\ntool : Cli.Command none none\nlet tool = cli Probe\n"
    );
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text);
    let warned: Vec<String> = (crate::check(&sources, file).iter())
        .filter(|d| d.code.to_string() == "FLICK-R012")
        .map(|d| d.message.clone())
        .collect();
    assert!(warned.is_empty(), "{warned:?}");
}

#[cfg(feature = "diagnostic-fixes")]
#[test]
fn a_head_of_std_inside_another_block_keeps_its_import() {
    // the body of `cli` is read only as it expands: `codec` there may need the import
    let text = "module Probe exposing (tool)\n\nimport Codec exposing (plugin codec)\n\ntool : Cli.Command none none\nlet tool = cli \"tool\" { handle fun {} -> codec x }\n";
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text);
    let warned: Vec<String> = (crate::check(&sources, file).iter())
        .filter(|d| d.code.to_string() == "FLICK-R012")
        .map(|d| d.message.clone())
        .collect();
    assert!(warned.is_empty(), "{warned:?}");
}
