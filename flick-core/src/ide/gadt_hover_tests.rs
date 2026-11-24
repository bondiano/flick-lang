use super::tests::fixture_of;
use crate::projection;

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

#[test]
fn the_projection_of_a_gadt_guards_its_cases_and_maps_its_name() {
    let source = "module App.Expr exposing (..)

type Expr a =
  | NatLit Int : Expr Int
  | If (Expr Bool) (Expr a) (Expr a) : Expr a
";
    let f = fixture_of(&[("src/App/Expr.fl", source)]);
    let ide = f.ide();
    let module = f.analysis.module("App.Expr").expect("module");
    let lookup = |m: &str| ide.interface(m).cloned();
    let projected = projection::project(module, source, "", &lookup, &|_| None, &|m| {
        projection::relative(&module.name, m)
    });
    let ts = &projected.text;
    for case in [
        "export type Expr<A> = { readonly \"~flick/params\"?: [A] } & (([number] extends [A] ? { readonly _tag: \"NatLit#",
        "{ readonly _tag: \"If#",
    ] {
        assert!(ts.contains(case), "{case}\n{ts}");
    }
    let declared = u32::try_from(source.find("Expr a").expect("declaration")).expect("small");
    assert!(
        projected.mappings.iter().any(|m| m.exact
            && &ts[m.ts[0] as usize..m.ts[1] as usize] == "Expr"
            && m.fl == [declared, declared + 4]),
        "{:?}",
        projected.mappings
    );
}
