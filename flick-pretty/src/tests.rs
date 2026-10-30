use std::time::Instant;

use crate::{Document, DocumentArena, Documentable, Indentation, NextBreakFitsMode, Position};

fn words<'a>(a: &'a DocumentArena<'a>, words: &[&'a str], separator: Document<'a>) -> Document<'a> {
    a.join(words.iter().map(|word| a.text(word)), separator)
}

/// `name(arguments)`, the arguments on their own lines when it does not fit.
fn call<'a>(a: &'a DocumentArena<'a>, name: &'a str, arguments: &[Document<'a>]) -> Document<'a> {
    let arguments = a.join(arguments.iter().copied(), a.break_(",", ", "));
    docvec![
        a,
        name,
        "(",
        docvec![a, a.break_("", ""), arguments].nest(a, 2),
        a.break_(",", ""),
        ")"
    ]
    .group(a)
}

#[test]
fn leaves_and_documentables() {
    let a = &DocumentArena::new();
    let owned = String::from("owned");
    let doc = docvec![
        a,
        "a",
        a.line(),
        "b",
        a.lines(2),
        "c",
        a.empty(),
        a.lines(0),
        'é',
        42,
        -7i64,
        &owned,
        String::from("!"),
    ];
    assert_eq!(doc.to_pretty_string(80), "a\nb\n\ncé42-7owned!");
    let extremes = docvec![a, i128::MIN, " ", u128::MAX];
    assert_eq!(
        extremes.to_pretty_string(80),
        format!("{} {}", i128::MIN, u128::MAX)
    );
}

#[test]
fn concat_join_and_surround() {
    let a = &DocumentArena::new();
    assert_eq!(
        words(a, &["a", "b", "c"], a.text(", ")).to_pretty_string(80),
        "a, b, c"
    );
    assert!(a.join([], a.text(", ")).is_empty());
    assert!(a.concat([]).is_empty());
    // More documents than one concatenation node holds, each built while the
    // arena is concatenating.
    let many = a.concat((0..20).map(|i| a.concat([a.text("x"), i.to_doc(a)])));
    let expected: String = (0..20).fold(String::new(), |all, i| all + "x" + &i.to_string());
    assert_eq!(many.to_pretty_string(80), expected);
    assert_eq!(
        a.text("x").surround(a, "(", ')').to_pretty_string(80),
        "(x)"
    );
    assert_eq!(
        a.text("x")
            .append(a, a.empty())
            .append(a, "y")
            .to_pretty_string(80),
        "xy"
    );
}

#[test]
fn is_empty() {
    let a = &DocumentArena::new();
    let nothing = docvec![a, a.empty(), a.cursor(0), a.text("")]
        .group(a)
        .nest(a, 2)
        .force_break(a);
    assert!(nothing.is_empty());
    assert!(!a.text("x").is_empty());
    assert!(!a.line().is_empty());
    assert!(!a.break_("", "").is_empty());
    assert!(!docvec![a, a.cursor(0), a.zero_width("x")].is_empty());
}

#[test]
fn group_fits_or_breaks() {
    let a = &DocumentArena::new();
    let doc = words(a, &["one", "two", "three"], a.break_("", " ")).group(a);
    assert_eq!(doc.to_pretty_string(13), "one two three");
    assert_eq!(doc.to_pretty_string(12), "one\ntwo\nthree");
}

#[test]
fn breaks_outside_groups_stay_flat() {
    let a = &DocumentArena::new();
    let doc = words(a, &["aaaa", "bbbb"], a.break_("", " "));
    assert_eq!(doc.to_pretty_string(3), "aaaa bbbb");
}

/// The example of Lindig's "Strictly Pretty": `if a == b then a << 2 else a + b`.
#[test]
fn lindig_conditional() {
    let a = &DocumentArena::new();
    let space = a.break_("", " ");
    let binop = |left, op, right| {
        let head = docvec![a, left, space, op].group(a);
        docvec![a, head, space, right].nest(a, 2).group(a)
    };
    let clause = |keyword, body| docvec![a, keyword, space, body].nest(a, 2).group(a);
    let doc = docvec![
        a,
        clause("if", binop("a", "==", "b")),
        space,
        clause("then", binop("a", "<<", "2")),
        space,
        clause("else", binop("a", "+", "b")),
    ]
    .group(a);
    assert_eq!(doc.to_pretty_string(32), "if a == b then a << 2 else a + b");
    assert_eq!(
        doc.to_pretty_string(15),
        "if a == b\nthen a << 2\nelse a + b"
    );
    assert_eq!(
        doc.to_pretty_string(10),
        "if a == b\nthen\n  a << 2\nelse a + b"
    );
    assert_eq!(
        doc.to_pretty_string(8),
        "if\n  a == b\nthen\n  a << 2\nelse\n  a + b"
    );
    assert_eq!(
        doc.to_pretty_string(7),
        "if\n  a ==\n    b\nthen\n  a <<\n    2\nelse\n  a + b"
    );
}

#[test]
fn nested_groups_break_outer_first() {
    let a = &DocumentArena::new();
    let inner = call(a, "inner", &[a.text("bbbb"), a.text("cccc")]);
    let doc = call(a, "outer", &[a.text("aaaa"), inner]);
    assert_eq!(doc.to_pretty_string(30), "outer(aaaa, inner(bbbb, cccc))");
    assert_eq!(
        doc.to_pretty_string(25),
        "outer(\n  aaaa,\n  inner(bbbb, cccc),\n)"
    );
    assert_eq!(
        doc.to_pretty_string(15),
        "outer(\n  aaaa,\n  inner(\n    bbbb,\n    cccc,\n  ),\n)"
    );
}

#[test]
fn flex_breaks_fill_lines() {
    let a = &DocumentArena::new();
    let all = ["aaa", "bbb", "ccc", "ddd", "eee"];
    let flex = words(a, &all, a.flex_break("", " ")).group(a);
    assert_eq!(flex.to_pretty_string(11), "aaa bbb ccc\nddd eee");
    assert_eq!(flex.to_pretty_string(80), "aaa bbb ccc ddd eee");
    let strict = words(a, &all, a.break_("", " ")).group(a);
    assert_eq!(strict.to_pretty_string(11), "aaa\nbbb\nccc\nddd\neee");
}

#[test]
fn nest_and_nest_if_broken() {
    let a = &DocumentArena::new();
    let body = docvec![a, "a", a.line(), "b"];
    // The group stops measuring at the line, fits, and so stays flat.
    assert_eq!(
        body.nest_if_broken(a, 2).group(a).to_pretty_string(80),
        "a\nb"
    );
    assert_eq!(body.nest(a, 2).group(a).to_pretty_string(80), "a\n  b");
    let broken = docvec![a, a.empty().force_break(a), body.nest_if_broken(a, 2)].group(a);
    assert_eq!(broken.to_pretty_string(80), "a\n  b");
}

#[test]
fn set_nesting_puts_a_line_directive_at_column_0() {
    let a = &DocumentArena::new();
    let directive = docvec![a, a.line(), "//line main.flick:7:1"].set_nesting(a, 0);
    let body = docvec![a, a.line(), "x := 1", directive, a.line(), "y := 2"];
    let doc = docvec![a, "{", body.nest(a, 4), a.line(), "}"];
    assert_eq!(
        doc.to_pretty_string(80),
        "{\n    x := 1\n//line main.flick:7:1\n    y := 2\n}"
    );
}

/// `doc` printed with tabs of `width` columns, and the cursors it passed.
fn with_tabs(doc: Document<'_>, limit: usize, width: usize) -> (String, Vec<(usize, Position)>) {
    let mut seen = Vec::new();
    let mut output = String::new();
    doc.pretty_print_with(
        limit,
        Indentation::Tabs { width },
        &mut output,
        &mut |mark, at| seen.push((mark, at)),
    )
    .expect("a String accepts any text");
    (output, seen)
}

#[test]
fn tabs_indent_by_width_columns() {
    let a = &DocumentArena::new();
    let inner = docvec![a, a.line(), "b", docvec![a, a.line(), "c"].nest(a, 2)].nest(a, 4);
    let directive = docvec![a, a.line(), "//line x:1"].set_nesting(a, 0);
    let doc = docvec![
        a,
        "{",
        docvec![a, a.line(), "a", inner, directive, a.line(), a.cursor(0)].nest(a, 4),
        a.line(),
        "}"
    ];
    let (output, seen) = with_tabs(doc, 80, 4);
    // Whole tabs first, then spaces for what is left; set nesting still wins.
    assert_eq!(output, "{\n\ta\n\t\tb\n\t\t  c\n//line x:1\n\n}");
    // The cursor is at four layout columns but one UTF-16 code unit.
    let at = Position {
        line: 5,
        column: 4,
        utf16_column: 1,
    };
    assert_eq!(seen, [(0, at)]);
    // The default is spaces, as before.
    assert_eq!(
        doc.to_pretty_string(80),
        "{\n    a\n        b\n          c\n//line x:1\n\n}"
    );
}

#[test]
fn a_tab_counts_as_its_width_when_fitting() {
    let a = &DocumentArena::new();
    let group = words(a, &["aaa", "bbb"], a.break_("", " ")).group(a);
    let doc = docvec![
        a,
        "{",
        docvec![a, a.line(), group].nest(a, 8),
        a.line(),
        "}"
    ];
    // The group needs 8 + 7 columns: its two tabs count as eight.
    assert_eq!(with_tabs(doc, 15, 4).0, "{\n\t\taaa bbb\n}");
    assert_eq!(with_tabs(doc, 14, 4).0, "{\n\t\taaa\n\t\tbbb\n}");
    // Tabs of one column: eight of them for the same nesting.
    assert_eq!(
        with_tabs(doc, 14, 1).0,
        "{\n\t\t\t\t\t\t\t\taaa\n\t\t\t\t\t\t\t\tbbb\n}"
    );
    // A tab of no width is read as one column, never a division by zero.
    assert_eq!(with_tabs(doc, 80, 0).0, with_tabs(doc, 80, 1).0);
}

#[test]
fn force_break_breaks_every_enclosing_group() {
    let a = &DocumentArena::new();
    let list = docvec![
        a,
        "[",
        docvec![a, a.break_("", ""), "1"].nest(a, 2),
        a.break_(",", ""),
        "]"
    ];
    let doc = call(a, "f", &[list.force_break(a).group(a)]);
    assert_eq!(doc.to_pretty_string(80), "f(\n  [\n    1,\n  ],\n)");
    // Measuring stops at a newline, before the `force_break`.
    let after_line = docvec![
        a,
        "a",
        a.break_("", " "),
        "b",
        a.line(),
        a.empty().force_break(a)
    ];
    assert_eq!(after_line.group(a).to_pretty_string(80), "a b\n");
}

#[test]
fn next_break_fits_hugs_the_last_argument() {
    let a = &DocumentArena::new();
    let body = docvec![
        a,
        "{",
        docvec![a, a.break_("", " "), "statement"].nest(a, 2),
        a.break_("", " "),
        "}"
    ]
    .group(a);
    let call_with = |last| {
        docvec![
            a,
            "call(",
            a.break_("", ""),
            "arg, fn() ",
            last,
            a.break_("", ""),
            ")"
        ]
    };
    let plain = call_with(body).group(a);
    assert_eq!(plain.to_pretty_string(29), "call(arg, fn() { statement })");
    assert_eq!(
        plain.to_pretty_string(20),
        "call(\narg, fn() {\n  statement\n}\n)"
    );
    let hugging = call_with(body.next_break_fits(a, NextBreakFitsMode::Enabled));
    assert_eq!(
        hugging.group(a).to_pretty_string(20),
        "call(arg, fn() {\n  statement\n})"
    );
    let disabled = hugging
        .next_break_fits(a, NextBreakFitsMode::Disabled)
        .group(a);
    assert_eq!(disabled.to_pretty_string(20), plain.to_pretty_string(20));
}

#[test]
fn zero_width_text_takes_no_room() {
    let a = &DocumentArena::new();
    let doc = docvec![
        a,
        a.zero_width("\x1b[1m"),
        "aaaa",
        a.zero_width("\x1b[0m"),
        a.break_("", " "),
        "bbbb"
    ]
    .group(a);
    assert_eq!(doc.to_pretty_string(9), "\x1b[1maaaa\x1b[0m bbbb");
}

#[test]
fn width_counts_grapheme_clusters() {
    let a = &DocumentArena::new();
    // An `e` with a combining acute accent, and a family emoji made of three
    // people joined by zero-width joiners: one cluster each.
    let doc = docvec![a, "e\u{301}e\u{301}", a.break_("", " "), "👨‍👩‍👧"].group(a);
    assert_eq!(doc.to_pretty_string(4), "e\u{301}e\u{301} 👨‍👩‍👧");
    assert_eq!(doc.to_pretty_string(3), "e\u{301}e\u{301}\n👨‍👩‍👧");
}

#[test]
fn no_trailing_whitespace() {
    let a = &DocumentArena::new();
    let body = docvec![a, a.line(), "a", a.lines(2), "b"];
    let block = docvec![a, "{", body.nest(a, 4), a.line(), "}"];
    assert_eq!(block.to_pretty_string(80), "{\n    a\n\n    b\n}");
    let ends_in_line = docvec![a, "a", a.line()].nest(a, 4);
    assert_eq!(ends_in_line.to_pretty_string(80), "a\n");
    let broken = docvec![a, "x =", docvec![a, a.break_("", " "), "value"].nest(a, 2)];
    assert_eq!(broken.group(a).to_pretty_string(5), "x =\n  value");
}

#[test]
fn newlines_inside_text_start_at_column_0() {
    let a = &DocumentArena::new();
    let doc = docvec![
        a,
        "{",
        docvec![a, a.line(), "s = `a\n\nb`"].nest(a, 2),
        a.cursor(0)
    ];
    let mut seen = Vec::new();
    let mut output = String::new();
    doc.pretty_print_observed(80, &mut output, &mut |mark, at| seen.push((mark, at)))
        .expect("a String accepts any text");
    assert_eq!(output, "{\n  s = `a\n\nb`");
    let end = Position {
        line: 3,
        column: 2,
        utf16_column: 2,
    };
    assert_eq!(seen, [(0, end)]);
}

#[test]
fn observer_positions() {
    let a = &DocumentArena::new();
    let doc = docvec![
        a,
        a.cursor(0),
        "let ",
        a.cursor(1),
        "👍 = ",
        a.zero_width("<b>"),
        a.cursor(2),
        "1",
        docvec![a, a.line(), a.cursor(3), "x"].nest(a, 2),
        a.line(),
        a.cursor(4),
    ];
    let mut seen = Vec::new();
    let mut output = String::new();
    doc.pretty_print_observed(80, &mut output, &mut |mark, at| seen.push((mark, at)))
        .expect("a String accepts any text");
    assert_eq!(output, "let 👍 = <b>1\n  x\n");
    let at = |line, column, utf16_column| Position {
        line,
        column,
        utf16_column,
    };
    // The emoji is one cluster but two UTF-16 code units; the zero-width tag
    // moves only the UTF-16 column.
    assert_eq!(
        seen,
        [
            (0, at(0, 0, 0)),
            (1, at(0, 4, 4)),
            (2, at(0, 8, 12)),
            (3, at(1, 2, 2)),
            (4, at(2, 0, 0))
        ]
    );
    // Without an observer, cursors print nothing.
    assert_eq!(doc.to_pretty_string(80), output);
}

#[test]
fn deep_nesting_uses_no_call_stack() {
    let a = &DocumentArena::new();
    let depth = 100_000;

    // Groups that do not fit, each measured only up to the line limit.
    let mut doc = a.text("x");
    for _ in 0..depth {
        doc = docvec![a, "(", doc, ")"]
            .group(a)
            .nest(a, 2)
            .next_break_fits(a, NextBreakFitsMode::Disabled);
    }
    assert_eq!(doc.to_pretty_string(100).len(), 2 * depth + 1);

    // Groups that all fit: the outermost is measured once, the rest trust it.
    let mut doc = a.text("x");
    for _ in 0..depth {
        doc = docvec![a, a.break_("", ""), doc].group(a);
    }
    assert_eq!(doc.to_pretty_string(100), "x");

    // A chain of appends as deep as it is long.
    let mut doc = a.empty();
    for _ in 0..depth {
        doc = doc.append(a, a.cursor(0)).append(a, "x");
    }
    assert_eq!(doc.to_pretty_string(100).len(), depth);
    assert!(!doc.is_empty());

    let mut doc = a.empty();
    for _ in 0..depth {
        doc = docvec![a, a.cursor(0), doc].group(a).force_break(a);
    }
    assert!(doc.is_empty());
}

/// A module of `size` calls followed by `size` nested parentheses.
fn sample<'a>(a: &'a DocumentArena<'a>, size: usize) -> Document<'a> {
    let statements = (0..size).map(|i| {
        let arguments: Vec<_> = (0..4).map(|j| docvec![a, "argument_", i, "_", j]).collect();
        docvec![a, "result_", i, " = ", call(a, "compute", &arguments), ";"]
    });
    let mut deep = a.text("leaf");
    for i in 0..size {
        deep = docvec![a, "(", a.break_("", ""), deep, a.cursor(i), ")"].group(a);
    }
    docvec![a, a.join(statements, a.line()), a.line(), deep]
}

#[test]
#[ignore = "timing: cargo test -p flick-pretty --release -- --ignored --nocapture"]
fn printing_time_grows_linearly() {
    let mut first = None;
    for size in [25_000u32, 50_000, 100_000, 200_000] {
        let a = &DocumentArena::new();
        let start = Instant::now();
        let output = sample(a, size as usize).to_pretty_string(100);
        let elapsed = start.elapsed();
        println!("{size:>7} items: {elapsed:?}, {} bytes", output.len());
        let (base_size, base_time) = *first.get_or_insert((size, elapsed));
        // Linear would be the size ratio; quadratic its square.
        let ratio = elapsed.as_secs_f64() / base_time.as_secs_f64();
        assert!(ratio < 3.0 * f64::from(size / base_size), "{ratio}");
    }
}
