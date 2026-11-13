//! Output layout and source cursors of the JavaScript document renderer.

use super::{Labels, compile, compile_with};

#[test]
fn parentheses_stay_only_where_the_code_around_needs_them() {
    let js = compile(
        "module App.Parens exposing (..)\n\
         plain : Float -> Float -> Float\nlet plain a b = a + b\n\
         scaled : Float -> Float -> Float\nlet scaled a b = (a + b) * 2\n\
         apply : (Float -> Float) -> Float -> Float -> Float\nlet apply f a b = f (a + b)\n\
         choose : (Float -> Float) -> Float\nlet choose f = f 1\n\
         pick : Bool -> Float\nlet pick c = choose (if c then (fun x -> x + 1) else (fun x -> x))\n",
    );
    assert!(
        js.contains("export const plain = (a) => (b) => a + b;"),
        "{js}"
    );
    assert!(js.contains("(a + b) * 2"), "{js}");
    assert!(js.contains("f(a + b)"), "{js}");
    // an arrow before `:` keeps the parentheses of its body
    assert!(
        js.contains("choose(c ? (x) => (x + 1) : (x$1) => x$1)"),
        "{js}"
    );
}

#[test]
fn a_wide_list_breaks_one_element_per_line_and_a_lone_callback_hugs() {
    let source = "module App.Layout exposing (..)\n\
         wide : Float -> Float -> List Float\n\
         let wide first second = [first * 1234567, second * 2345678, first * 3456789, second * 4567891, first]\n\
         run : ((Option Float -> Float) -> Float) -> Float\n\
         let run f = f (fun x -> match x with\n  | Some y -> y\n  | None -> 0)\n";
    let js = compile(source);
    assert!(
        js.contains("=> [\n  first * 1234567,\n  second * 2345678,\n"),
        "{js}"
    );
    assert!(
        js.contains("return f((x) => {\n    if (x._tag === \"Some\") {"),
        "{js}"
    );
    assert!(js.lines().all(|line| line.len() <= 100), "{js}");
    let release = compile_with(source, true, Labels::default()).code;
    assert!(
        release.contains(
            "[first * 1234567, second * 2345678, first * 3456789, second * 4567891, first]"
        ),
        "{release}"
    );
}

#[cfg(feature = "native-project")]
#[test]
fn a_mapping_on_a_line_the_printer_broke_points_at_its_source() {
    let text = "module App.Wide exposing (..)\n\
         scaled : Float -> Float -> Float -> Float -> List Float\n\
         let scaled first second third fourth = [first * 1234567, second * 2345678, third * 3456789, fourth * 4567891]\n";
    let output = compile_with(text, false, Labels::default());
    let lines: Vec<&str> = output.code.lines().collect();
    let line = lines
        .iter()
        .position(|l| l.trim_start().starts_with("third * 3456789"))
        .unwrap_or_else(|| panic!("the list breaks: {}", output.code));
    let column = lines[line].find("third").unwrap_or_default();
    let (.., span, _) = output
        .mappings
        .iter()
        .find(|(l, c, ..)| *l as usize == line && *c as usize == column)
        .unwrap_or_else(|| panic!("a mapping at {line}:{column}: {:?}", output.mappings));
    assert!(
        text[span.start as usize..].starts_with("third * 3456789"),
        "{span:?}"
    );
}

#[cfg(feature = "native-project")]
#[test]
fn source_cursors_after_astral_text_count_utf16_columns() {
    let source = "module App exposing (pair)\n\npair : Float -> (String, Float)\nlet pair n = (\"😀\", n + 1)\n";
    let output = compile_with(source, false, Labels::default());
    let (line, column, _, _) = output
        .mappings
        .iter()
        .find(|(_, _, span, _)| source[span.start as usize..].starts_with("n + 1"))
        .expect("the binary expression has a cursor");
    let generated = output
        .code
        .lines()
        .nth(*line as usize)
        .expect("mapped line");
    let before = generated.split_once("n + 1").expect("emitted addition").0;
    assert!(before.contains('😀'), "{generated}");
    assert_eq!(
        *column as usize,
        before.encode_utf16().count(),
        "{generated}"
    );
}
