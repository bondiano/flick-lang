use std::fmt::Write;

use super::Projection;
use crate::iface::{Interface, Namespace};
use crate::ir::tests::check_text;
use crate::types;

/// The declarations and each exported value's type, or why it has none.
fn project(text: &str) -> String {
    let (_, interface) = check_text("App.Shapes", text).expect("checks");
    let own = interface.clone();
    let lookup = move |m: &str| -> Option<Interface> {
        if m == own.module {
            return Some(own.clone());
        }
        types::builtins().iter().find(|i| i.module == m).cloned()
    };
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
         type Shape = Circle Number | Rect Number Number\n\
         type Tree a = Leaf | Node (Tree a) a (Tree a)\n\
         type alias Point = { x : Number; y : Number }\n\
         opaque type Token = Token String\n\
         type error NotFound = NotFound String\n\
         service Users =\n  get : String ->{; NotFound} Point\n\
         class Describe a =\n  describe : a -> String\n\
         identity : a -> a\nlet identity x = x\n\
         area : Shape -> Number\nlet area s = 1\n\
         origin : Point\nlet origin = { x = 0; y = 0 }\n\
         empty : List a\nlet empty = []\n\
         pair : a -> b -> (a, b)\nlet pair a b = (a, b)\n\
         load : String ->{Users; NotFound} Point\nlet load id = Users.get id\n\
         later : String -> Eff {Users} {NotFound} Point\nlet later id = task { Users.get id }\n\
         first : List a -> Option a\nlet first xs = None\n\
         parse : String -> Result NotFound Number\nlet parse s = Ok 1\n\
         shown : Describe a => a -> String\nlet shown x = describe x\n\
         apply_all : f a -> f a\nlet apply_all x = x\n",
    );
    insta::assert_snapshot!(out);
    let typed: Vec<&str> = out.lines().filter(|l| !l.contains(": !")).collect();
    assert!(typed.iter().all(|l| !l.contains("any")), "{out}");
}

#[test]
fn unit_never_and_empty_rows_have_distinct_typescript_types() {
    let out = project(
        "module App.Shapes exposing (..)\n\
         type error Oops = Oops\n\
         skip : Unit -> Unit\nlet skip u = u\n\
         absurd : Never -> Number\nlet absurd n = 1\n\
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
        Type::prim("Ref"),
        qualified(
            Type::pure_fun(Type::prim("Number"), Type::prim("Number")),
            Type::Var("r".to_owned()),
        ),
    );
    let scoped = scheme(
        vec![binder("r", Kind::Region), binder("c", Kind::CaptureRow)],
        qualified(Type::pure_fun(Type::prim("Unit"), stored), c),
    );
    assert!(projection.value(&scoped).is_err());
    assert!(
        projection
            .value(&qualified(
                Type::prim("Number"),
                Type::Var("unknown".to_owned()),
            ))
            .is_err()
    );
}
