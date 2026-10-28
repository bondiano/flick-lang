use crate::source::SourceMap;

/// `text` after the first fix of the first diagnostic with `code`.
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

#[test]
fn an_unused_variable_is_renamed_with_an_underscore() {
    let text = format!("{HEAD}f : (Float, Float) -> Float\nlet f (x, y) = x\n");
    assert!(fixed(&text, "FLICK-R013").contains("let f (x, _y) = x"));
    let text = format!("{HEAD}f : {{ a : Float; b : Float }} -> Float\nlet f {{ a; b }} = a\n");
    assert!(fixed(&text, "FLICK-R013").contains("let f { a; b = _b } = a"));
}
