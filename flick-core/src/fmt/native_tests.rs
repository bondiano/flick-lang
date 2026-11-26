use crate::fmt::format;
use crate::source::FileId;

#[test]
fn formats_native_overrides_and_is_idempotent() {
    let messy = "\
module Demo exposing (hypot, fetch)

hypot : Float -> Float -> Float
let hypot a b = Float.sqrt (a * a + b * b)
    @native  \"Math.hypot\"  when  js&&! browser && ( node||effect-adapter )
    @native  global  \"math.Hypot\"  when  go

fetch : String -> String
let fetch url =
  let normalized = url
  normalized
  @native \"./net.mjs\" \"fetch\" when js
    this
    variadic
";
    let expected = "\
module Demo exposing (hypot, fetch)

hypot : Float -> Float -> Float
let hypot a b = Float.sqrt (a * a + b * b)
  @native \"Math.hypot\" when js && !browser && (node || effect-adapter)
  @native global \"math.Hypot\" when go

fetch : String -> String
let fetch url =
  let normalized = url
  normalized
  @native \"./net.mjs\" \"fetch\" when js
    this
    variadic
";
    let once = format(messy, FileId::new(0)).expect("format once");
    assert_eq!(once, expected);
    let twice = format(&once, FileId::new(0)).expect("format twice");
    assert_eq!(twice, once);
}

/// The `@native when` of a type takes its own line, one level in, after
/// `deriving`.
#[test]
fn formats_the_native_condition_of_a_type() {
    let messy = "\
module Demo exposing (Box, Handle)

type Box a =
  | Box a
  deriving Eq
     @native   when  js||go

type Handle
 @native  when js
";
    let expected = "\
module Demo exposing (Box, Handle)

type Box a = Box a
  deriving Eq
  @native when js || go

type Handle
  @native when js
";
    let once = format(messy, FileId::new(0)).expect("format once");
    assert_eq!(once, expected);
    assert_eq!(format(&once, FileId::new(0)).expect("format twice"), once);
}
