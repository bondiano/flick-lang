use super::*;
use crate::resolve::{self, Module};
use crate::source::SourceMap;
use crate::types::Elided;
use crate::{syntax, types};

#[test]
fn persistent_templates_follow_the_artifact_capability() {
    let interface = Interface {
        module: "App.Factory".into(),
        template: Some(ModuleTemplate {
            source: "module App.Factory(K : App.Contracts.Empty) : App.Contracts.Empty\n".into(),
            capture: None,
            materialized_body: None,
        }),
        ..Interface::default()
    };
    let serialized = serde_json::to_value(&interface).expect("interface serializes");
    let persistent = cfg!(feature = "native-project");
    assert_eq!(serialized.get("template").is_some(), persistent);
    let restored: Interface = serde_json::from_value(serialized).expect("interface reads");
    if persistent {
        assert_eq!(restored.template, interface.template);
    } else {
        assert!(restored.template.is_none());
    }
}

fn interface_of(text: &str) -> Interface {
    let mut sources = SourceMap::default();
    let file = sources.add("t.fl", text);
    let parse = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parse.tree, text, file);
    let module = Module {
        name: "App.Shapes",
        file,
        text,
        tree: &parse.tree,
        syntax: &syntax,
        generated: resolve::NO_GENERATED,
    };
    let (mut interface, mut diagnostics) = resolve::resolve(&module, &|_| None);
    let local = |name: &str| name == "App.Shapes";
    let options = crate::types::Options {
        local: &local,
        limits: types::Limits::default(),
        no_let_assert: false,
        thresholds: types::Thresholds::default(),
        effects: true,
        watched: types::Watched::none(),
        stages_by_class: false,
    };
    diagnostics.extend(types::check(
        &module,
        &|_| None,
        types::builtins(),
        options,
        &mut interface,
    ));
    diagnostics.retain(|d| !d.unnecessary());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    interface
}

#[test]
fn private_visibility_round_trips_and_changes_fingerprint() {
    let public = interface_of("module App.Shapes exposing (answer)\nlet answer = 42\n");
    let private = interface_of("private module App.Shapes exposing (answer)\nlet answer = 42\n");
    assert!(!public.private);
    assert!(private.private);
    assert_ne!(fingerprint(&public), fingerprint(&private));
    assert_eq!(read(write(&private).as_bytes(), "App.Shapes"), Ok(private));
}

#[test]
fn schema_is_pinned_and_round_trips() {
    let interface = interface_of(
        "module App.Shapes exposing (Shape(..), Id, Size, Mode(..), Show(..), Collection(..), area, now, Pair, Clock, NotFound(..), Box, wrap, later)\n\
         type Shape = Circle Float | Square Float\n  deriving Eq\n\
         opaque type Id = Id String\n\
         type alias Size = Float\n\
         kind Mode = Small | Big\n\
         class Show a =\n  show : a -> String\n\
         class Eq a => Collection c a | c -> a =\n  empty : c\n\
         instance Show Shape =\n  show : Shape -> String\n  let show s = \"shape\"\n\
         area : Shape -> Float\nlet area s = 0\n\
         now : Unit ->{} Float\nforeign now from global \"Date.now\"\n\
         type Pair\n\
         service Clock =\n  tick : Unit ->{} Float\n\
         type error NotFound = NotFound String\n\
         type alias Box (f : Type -> Type) a = { value : f a; size : Size }\n\
         wrap : f a -> Box f a\nlet wrap value = { value = value; size = 1 }\n\
         /// A thunk of `h`.\n\
         later : forall s. Handle s -> (Unit -> Handle s)^{s}\nlet later h = fun () -> h\n\
         type Handle (s : Region)\n",
    );
    let json = write(&interface);
    insta::assert_snapshot!(json);
    let back = read(json.as_bytes(), "App.Shapes").expect("valid");
    assert_eq!(write(&back), json);
}

#[test]
fn capture_qualification_kind_round_trips() {
    // whether the checker read the qualifier into a signature is kept too
    for value in [false, true] {
        let mut interface = interface_of("module App.Shapes exposing (answer)\nlet answer = 42\n");
        let scheme = &mut interface.exports[0].scheme;
        *scheme = Some(Type::Captured {
            ty: Box::new(scheme.take().expect("answer has a type")),
            captures: Box::new(Type::empty_row()),
            value,
            elided: Elided(value),
        });
        let json = write(&interface);
        let encoded: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(encoded["exports"][0]["type"]["captured"]["value"], value);
        let elided = &encoded["exports"][0]["type"]["captured"]["elided"];
        assert_eq!(elided.as_bool().unwrap_or(false), value);
        assert_eq!(
            read(json.as_bytes(), "App.Shapes").expect("valid interface"),
            interface
        );
    }
}

#[test]
fn rejects_other_versions_and_modules() {
    let json = write(&Interface {
        module: "A".into(),
        ..Interface::default()
    });
    assert!(
        read(json.as_bytes(), "B")
            .expect_err("rejected")
            .contains("describes `A`")
    );
    let current = format!("\"iface-version\": {IFACE_VERSION}");
    let future = json.replace(&current, "\"iface-version\": 99");
    assert!(
        read(future.as_bytes(), "A")
            .expect_err("rejected")
            .contains("`iface-version` 99")
    );
    let other = json.replace(
        &format!("\"compiler\": \"{}\"", crate::VERSION),
        "\"compiler\": \"9.9.9\"",
    );
    assert!(
        read(other.as_bytes(), "A")
            .expect_err("rejected")
            .contains("flick 9.9.9")
    );
    assert!(read(b"{", "A").is_err());
}
