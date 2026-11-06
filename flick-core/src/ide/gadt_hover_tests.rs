use super::tests::fixture_of;

#[test]
fn a_gadt_branch_shows_what_it_learned() {
    let source = "module App.Main exposing (Ty(..), describe)

type Ty a =
  | TInt : Ty Int
  | TStr : Ty String

describe : forall a. Ty a -> a -> String
let describe t x =
  match t with
  | TInt -> show (x + 1)
  | TStr -> x ++ \"!\"
";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Main");
    for (needle, expected) in [
        ("x + 1", "x : Int (= a)"),
        ("x ++", "x : String (= a)"),
        ("x =\n", "x : a"),
        ("TInt :", "TInt : Ty Int"),
    ] {
        let start = u32::try_from(source.find(needle).expect("hover target")).expect("small");
        let (markdown, _) = ide.hover(file, start).expect(needle);
        assert!(markdown.contains(expected), "{needle}: {markdown}");
    }
}
