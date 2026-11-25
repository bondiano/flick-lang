use crate::fmt::format;
use crate::source::FileId;

#[test]
fn formats_export_attributes_before_signature_and_let_idempotently() {
    let messy = "\
module Demo exposing (add, sub)

@export   \"addNumbers\"
add : Int -> Int -> Int
let add a b = a + b

sub : Int -> Int -> Int
@export  \"subNumbers\"
let sub a b = a - b
";
    let expected = "\
module Demo exposing (add, sub)

@export \"addNumbers\"
add : Int -> Int -> Int
let add a b = a + b

sub : Int -> Int -> Int
@export \"subNumbers\"
let sub a b = a - b
";
    let once = format(messy, FileId::new(0)).expect("format once");
    assert_eq!(once, expected);
    let twice = format(&once, FileId::new(0)).expect("format twice");
    assert_eq!(twice, once);
}
