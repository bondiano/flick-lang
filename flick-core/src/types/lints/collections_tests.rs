use crate::types::lint_tests::{HEAD, assert_fix, fixed};

const NUMS: &str = "f : List Float -> Option Float\n";
const ANY: &str = "f : List Float -> Bool\n";

#[test]
fn the_head_of_a_filter_is_find() {
    assert_fix(
        &format!("{NUMS}let f xs = List.head (List.filter (fun x -> x > 1) xs)\n"),
        "FLICK-T047",
        &format!("{NUMS}let f xs = List.find (fun x -> x > 1) xs\n"),
    );
    assert_fix(
        &format!("{NUMS}let f xs = xs |> List.filter (fun x -> x > 1) |> List.head\n"),
        "FLICK-T047",
        &format!("{NUMS}let f xs = List.find (fun x -> x > 1) xs\n"),
    );
}

#[test]
fn a_filtered_length_against_zero_is_any() {
    assert_fix(
        &format!("{ANY}let f xs = List.length (List.filter (fun x -> x > 1) xs) > 0\n"),
        "FLICK-T048",
        &format!("{ANY}let f xs = List.any (fun x -> x > 1) xs\n"),
    );
    assert_fix(
        &format!("{ANY}let f xs = List.length (List.filter (fun x -> x > 1) xs) == 0\n"),
        "FLICK-T048",
        &format!("{ANY}let f xs = not (List.any (fun x -> x > 1) xs)\n"),
    );
}

#[test]
fn a_length_against_zero_is_is_empty() {
    assert_fix(
        &format!("{ANY}let f xs = List.length xs == 0\n"),
        "FLICK-T049",
        &format!("{ANY}let f xs = List.is_empty xs\n"),
    );
    assert_fix(
        &format!("{ANY}let f xs = List.length xs >= 1\n"),
        "FLICK-T049",
        &format!("{ANY}let f xs = not (List.is_empty xs)\n"),
    );
    let two = format!("{HEAD}{ANY}let f xs = List.length xs == 2\n");
    assert_eq!(fixed(&two, "FLICK-T049"), None);
}

#[test]
fn concat_of_map_is_flat_map() {
    assert_fix(
        "f : List Float -> List Float\nlet f xs = List.concat (List.map (fun x -> [x, x]) xs)\n",
        "FLICK-T050",
        "f : List Float -> List Float\nlet f xs = List.flat_map (fun x -> [x, x]) xs\n",
    );
}

#[test]
fn an_option_match_with_a_default_is_with_default() {
    assert_fix(
        "f : Option Float -> Float\nlet f o = match o with\n  | Some x -> x\n  | None -> 0\n",
        "FLICK-T051",
        "f : Option Float -> Float\nlet f o = Option.with_default 0 o\n",
    );
    let used = format!(
        "{HEAD}f : Option Float -> Float\nlet f o = match o with\n  | Some x -> x + 1\n  | None -> 0\n"
    );
    assert_eq!(fixed(&used, "FLICK-T051"), None);
}
