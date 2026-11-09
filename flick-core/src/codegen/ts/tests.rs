use std::fmt::Write;

use super::{Projection, declaration_file};
use crate::iface::{Interface, Namespace};
use crate::ir::tests::check_text;
use crate::types;
use crate::types::lang::Lang;

/// The interface of `text`, and a lookup of it beside the built-in modules.
fn checked(text: &str) -> (Interface, impl Fn(&str) -> Option<Interface>) {
    let (_, interface) = check_text("App.Shapes", text).expect("checks");
    let own = interface.clone();
    let lookup = move |m: &str| -> Option<Interface> {
        if m == own.module {
            return Some(own.clone());
        }
        types::builtins().iter().find(|i| i.module == m).cloned()
    };
    (interface, lookup)
}

/// The declarations and each exported value's type, or why it has none.
fn project(text: &str) -> String {
    let (interface, lookup) = checked(text);
    let package = |_: &str| Some("@acme/shapes".to_owned());
    let projection = Projection::new("App.Shapes", &lookup, &package);
    let (declarations, refused) = projection.declarations(&interface);
    let mut out = declarations.join("\n");
    for (name, reason) in refused {
        let _ = write!(out, "\ntype {name}: {reason}");
    }
    for e in &interface.exports {
        if e.entity.namespace == Namespace::Value {
            let ty = projection.value(e.scheme.as_ref().expect("typed"));
            let ty = ty.unwrap_or_else(|r| format!("! {r}"));
            let _ = write!(out, "\n{}: {ty}", e.entity.name);
        }
    }
    out
}

#[test]
fn exports_have_typescript_types_without_any() {
    let out = project(
        "module App.Shapes exposing (..)\n\
         type Shape = Circle Float | Rect Float Float\n\
         type Tree a = Leaf | Node (Tree a) a (Tree a)\n\
         type alias Point = { x : Float; y : Float }\n\
         opaque type Token = Token String\n\
         type error NotFound = NotFound String\n\
         service Users =\n  get : String ->{; NotFound} Point\n\
         class Describe a =\n  describe : a -> String\n\
         identity : a -> a\nlet identity x = x\n\
         area : Shape -> Float\nlet area s = 1\n\
         origin : Point\nlet origin = { x = 0; y = 0 }\n\
         empty : List a\nlet empty = []\n\
         pair : a -> b -> (a, b)\nlet pair a b = (a, b)\n\
         load : String ->{Users; NotFound} Point\nlet load id = Users.get id\n\
         later : String -> Eff {Users} {NotFound} Point\nlet later id = task { Users.get id }\n\
         first : List a -> Option a\nlet first xs = None\n\
         parse : String -> Result NotFound Float\nlet parse s = Ok 1\n\
         shown : Describe a => a -> String\nlet shown x = describe x\n\
         apply_all : f a -> f a\nlet apply_all x = x\n",
    );
    insta::assert_snapshot!(out);
    let typed: Vec<&str> = out.lines().filter(|l| !l.contains(": !")).collect();
    assert!(typed.iter().all(|l| !l.contains("any")), "{out}");
}

#[test]
fn first_class_module_projects_its_public_values_to_typescript() {
    let out = project(
        "module App.Shapes exposing (S, packed)\n\
         type module S =\n  identity : a -> a\n\
         packed : Module S\nlet packed = pack Prelude as S\n",
    );
    assert!(
        out.contains("packed: { readonly identity: <A>(arg: A) => A }"),
        "{out}"
    );
    assert!(!out.contains("interface S"), "{out}");
}

#[test]
fn abstract_module_witness_is_not_projected_as_any() {
    let out = project(
        "module App.Shapes exposing (S, packed)\n\
         type module S =\n  type Key\n  value : Key\n\
         packed : Module S\nforeign pure packed from \"fixture\" \"packed\"\n",
    );
    assert!(out.contains("packed: !"), "{out}");
    assert!(out.contains("abstract type"), "{out}");
}

#[test]
fn existential_module_value_is_exported_as_unknown() {
    let lookup = |m: &str| types::builtins().iter().find(|i| i.module == m).cloned();
    let package = |_: &str| None;
    let projection = Projection::new("App.Shapes", &lookup, &package);
    let ty = types::Type::Exists {
        binder: "K".to_owned(),
        contract: Box::new(types::Type::con("App.Shapes", "Box")),
        body: Box::new(types::Type::con("$exists:K", "Key")),
    };
    assert_eq!(projection.value(&ty), Ok("unknown".to_owned()));
}

/// A case per constructor, over the type's parameters; a hidden type is
/// `unknown`, and a held dictionary is no field.
#[test]
fn a_gadt_is_a_union_of_its_cases_without_dictionaries() {
    let out = project(
        "module App.Shapes exposing (..)\n\
         type Expr a =\n  | IntLit Int : Expr Int\n  | If (Expr Bool) (Expr a) (Expr a) : Expr a\n  | Apply (Expr (b -> a)) (Expr b) : Expr a\n  | Lit a\n\
         type Ty a =\n  | TInt : Ty Int\n  | TStr : Ty String\n\
         type Showable =\n  | Show b => Showable b : Showable\n",
    );
    insta::assert_snapshot!(out, @r#"
    export type Expr<A> = { readonly _tag: "IntLit#662280bfd5ca"; readonly _0: number } | { readonly _tag: "If#7415f24a3923"; readonly _0: Expr<boolean>; readonly _1: Expr<A>; readonly _2: Expr<A> } | { readonly _tag: "Apply#cfce91e23371"; readonly _0: Expr<(arg: unknown) => A>; readonly _1: Expr<unknown> } | { readonly _tag: "Lit#90f2c71f30bd"; readonly _0: A };
    export type Ty<T0> = { readonly _tag: "TInt#7ba8d1aef192" } | { readonly _tag: "TStr#9bbfbdae731e" };
    export type Showable = { readonly _tag: "Showable#a990c444ac9c"; readonly _0: unknown };
    "#);
}

#[test]
fn unit_never_and_empty_rows_have_distinct_typescript_types() {
    let out = project(
        "module App.Shapes exposing (..)\n\
         type error Oops = Oops\n\
         skip : Unit -> Unit\nlet skip u = u\n\
         absurd : Never -> Float\nlet absurd n = 1\n\
         quiet : Eff none none Unit\nlet quiet = Effect.void\n\
         failing : Eff none {Oops} Never\nlet failing = task { from_result (Err Oops) }\n",
    );
    for expected in [
        "skip: (arg: undefined) => undefined",
        "absurd: (arg: never) => number",
        "quiet: $Effect.Effect<undefined, never, never>",
        "failing: $Effect.Effect<never, Oops, never>",
    ] {
        assert!(out.contains(expected), "{expected}\n{out}");
    }
}

#[test]
fn capture_projection_preserves_outer_data_binders_but_not_stored_regions() {
    use crate::types::{Binder, Kind, Type};

    let lookup = |m: &str| types::builtins().iter().find(|i| i.module == m).cloned();
    let package = |_: &str| None;
    let projection = Projection::new("App.Shapes", &lookup, &package);
    let binder = |name: &str, kind| Binder {
        name: name.to_owned(),
        kind,
    };
    let scheme = |vars, body| Type::Forall {
        vars,
        constraints: Vec::new(),
        body: Box::new(body),
    };
    let qualified = |ty, captures| Type::Captured {
        ty: Box::new(ty),
        captures: Box::new(captures),
        value: false,
    };
    let a = Type::Var("a".to_owned());
    let c = Type::Var("c".to_owned());
    let nested = scheme(
        vec![binder("b", Kind::Type)],
        Type::pure_fun(
            Type::Var("b".to_owned()),
            Type::Bounded {
                ty: Box::new(a.clone()),
                captures: Box::new(Type::Union(Box::new(a.clone()), Box::new(c.clone()))),
            },
        ),
    );
    let scope_free = scheme(
        vec![binder("a", Kind::Type), binder("c", Kind::CaptureRow)],
        qualified(Type::pure_fun(a, nested), c.clone()),
    );
    assert_eq!(
        projection.value(&scope_free),
        Ok("<A>(arg: A) => (<B>(arg: B) => A)".to_owned())
    );

    let stored = Type::app(
        Type::lang(Lang::Ref),
        qualified(
            Type::pure_fun(Type::lang(Lang::Float), Type::lang(Lang::Float)),
            Type::Var("r".to_owned()),
        ),
    );
    let scoped = scheme(
        vec![binder("r", Kind::Region), binder("c", Kind::CaptureRow)],
        qualified(Type::pure_fun(Type::lang(Lang::Unit), stored), c),
    );
    assert!(projection.value(&scoped).is_err());
    assert!(
        projection
            .value(&qualified(
                Type::lang(Lang::Float),
                Type::Var("unknown".to_owned()),
            ))
            .is_err()
    );
}

#[test]
fn ui_is_the_runtime_ui_imported_only_where_used() {
    let lookup = |m: &str| types::builtins().iter().find(|i| i.module == m).cloned();
    let package = |_: &str| None;
    let path = |m: &str| m.to_owned();
    let projection = Projection::new("App.Shapes", &lookup, &package);
    assert_eq!(
        projection.value(&types::Type::lang(Lang::Float)),
        Ok("number".to_owned())
    );
    assert!(!projection.imports(&path).contains("$Ui"));
    let view = types::Type::pure_fun(types::Type::lang(Lang::Bool), types::Type::lang(Lang::Ui));
    assert_eq!(
        projection.value(&view),
        Ok("(arg: boolean) => $Ui".to_owned())
    );
    assert!(
        projection
            .imports(&path)
            .contains("import type { Ui as $Ui } from \"@flick-lang/effect-ts/ui/react\";")
    );
}

#[test]
fn an_enum_is_the_union_of_its_literals_or_a_brand_over_them() {
    let out = project(
        "module App.Shapes exposing (Method(..), Level, pick, level)\n\
         untagged type Method = Get = \"GET\" | Post = \"POST\"\n\
         opaque untagged type Level = Low = 0 | High = 1\n\
         pick : Method\nlet pick = Post\n\
         level : Level\nlet level = Low\n",
    );
    assert!(
        out.contains("export type Method = \"GET\" | \"POST\";"),
        "{out}"
    );
    assert!(
        out.contains("export type Level = number & { readonly \"~flick/brand\": \"@acme/shapes/App.Shapes.Level\" };"),
        "{out}"
    );
    assert!(out.contains("pick: Method"), "{out}");
}

#[test]
fn doc_comments_are_jsdoc_and_records_are_interfaces() {
    let (interface, lookup) = checked(
        "module App.Shapes exposing (..)\n\
         /// A shape.\n\
         type Shape =\n\
         \x20 /// A circle by its radius.\n\
         \x20 | Circle Float\n\
         \x20 | Rect Float Float\n\
         /// A point on the plane.\n\
         ///\n\
         /// `x` grows to the right; a */ stays inside.\n\
         type alias Point = { x : Float; y : Float }\n\
         type alias Box a = { value : a; label : String }\n\
         type alias Size = Float\n\
         type alias Pair = (Float, Float)\n\
         type alias Measure = Shape -> Float\n\
         /// An opaque token.\n\
         opaque type Token = Token String\n\
         /// The area of a shape.\n\
         area : Shape -> Float\nlet area s = 1\n",
    );
    let package = |_: &str| Some("@acme/shapes".to_owned());
    let projection = Projection::new("App.Shapes", &lookup, &package);
    let (dts, refused) = declaration_file(&projection, &interface, &|m: &str| m.to_owned());
    assert!(refused.is_empty(), "{refused:?}");
    insta::assert_snapshot!(dts);
}

#[test]
fn a_set_of_services_and_errors_has_no_declaration() {
    let (interface, lookup) = checked(
        "module App.Sets exposing (..)\n\
         service Log =\n  line : String ->{} Unit\n\
         type error Boom = Boom\n\
         type alias Env = {Log; Boom}\n\
         type alias Errs = {Boom}\n\
         type alias Db s = {Scope s, Log}\n\
         run : String ->{...Env} Unit\nlet run line = Log.line line\n",
    );
    let package = |_: &str| Some("@acme/sets".to_owned());
    let projection = Projection::new("App.Sets", &lookup, &package);
    let (dts, refused) = declaration_file(&projection, &interface, &|m: &str| m.to_owned());
    assert!(refused.is_empty(), "{refused:?}");
    for name in ["Env", "Errs", "Db"] {
        assert!(!dts.contains(&format!("export type {name}")), "{dts}");
        assert!(!dts.contains(&format!("export interface {name}")), "{dts}");
    }
}
