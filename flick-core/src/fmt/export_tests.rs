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

#[test]
fn formats_record_field_and_namespaced_attributes_idempotently() {
    let messy = "\
module Demo exposing (User(..), Profile)

@meta.prefix   \"usr\"
type alias Profile = { @rename \"user_id\" userName : String; @skip @default -1 retries : Int }

@meta.prefix   \"usr\"
type User = User { @rename \"user_id\" userName : String; @skip @default -1 retries : Int }
";
    let expected = "\
module Demo exposing (User(..), Profile)

@meta.prefix \"usr\"
type alias Profile = {
  @rename \"user_id\"
  userName : String
  @skip
  @default -1
  retries : Int
}

@meta.prefix \"usr\"
type User =
  | User {
    @rename \"user_id\"
    userName : String
    @skip
    @default -1
    retries : Int
  }
";
    let once = format(messy, FileId::new(0)).expect("format once");
    assert_eq!(once, expected);
    let twice = format(&once, FileId::new(0)).expect("format twice");
    assert_eq!(twice, once);

    // Comments above and between stacked attributes stay attached cleanly.
    let commented = "\
module Demo exposing (Item)

type alias Item = {
  /// Field doc
  @rename \"item_id\" // wire name
  // fallback
  @default 0
  id : Int
}
";
    let fmt_commented = format(commented, FileId::new(0)).expect("format commented");
    assert_eq!(fmt_commented, commented);
}
