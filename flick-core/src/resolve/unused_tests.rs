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
    let body = "f : Number -> Number\nlet f x = abs x\n";
    let cases = [
        (
            "import Number exposing (abs, floor)\n",
            "import Number exposing (abs)\n",
        ),
        (
            "import Number exposing (floor, abs)\n",
            "import Number exposing (abs)\n",
        ),
        (
            "import Number exposing (abs, floor, ceil)\n",
            "import Number exposing (abs, ceil)\n",
        ),
    ];
    for (import, want) in cases {
        let text = format!("{HEAD}{import}{body}");
        assert_eq!(fixed(&text, "FLICK-R012"), format!("{HEAD}{want}{body}"));
    }
    let text = format!(
        "{HEAD}import Number exposing (floor)\n{}",
        body.replace("abs x", "Number.abs x")
    );
    assert_eq!(
        fixed(&text, "FLICK-R012"),
        format!(
            "{HEAD}import Number\n{}",
            body.replace("abs x", "Number.abs x")
        )
    );
}

#[test]
fn removing_an_unused_import_or_value_deletes_its_lines() {
    let text = format!("{HEAD}import String\nf : Number -> Number\nlet f x = x\n");
    assert_eq!(
        fixed(&text, "FLICK-R012"),
        format!("{HEAD}f : Number -> Number\nlet f x = x\n")
    );
    let text = format!(
        "{HEAD}f : Number -> Number\nlet f x = x\n\ng : Number\nlet g = 1\n\nh : Number\nlet h = 2\n"
    );
    assert_eq!(
        fixed(&text, "FLICK-R014"),
        format!("{HEAD}f : Number -> Number\nlet f x = x\n\nh : Number\nlet h = 2\n")
    );
}

#[test]
fn an_unused_variable_is_renamed_with_an_underscore() {
    let text = format!("{HEAD}f : (Number, Number) -> Number\nlet f (x, y) = x\n");
    assert!(fixed(&text, "FLICK-R013").contains("let f (x, _y) = x"));
    let text = format!("{HEAD}f : {{ a : Number; b : Number }} -> Number\nlet f {{ a; b }} = a\n");
    assert!(fixed(&text, "FLICK-R013").contains("let f { a; b = _b } = a"));
}
