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
