use std::slice;
use std::sync::LazyLock;

use flick_core::diagnostics::{Code, Diagnostic, codes};
use flick_core::iface::{self, Interface};
use flick_core::source::SourceMap;
use flick_core::{resolve, syntax, types};

#[path = "transformer_captures/mtl.rs"]
mod mtl;

fn check(text: &str, imported: &[Interface]) -> (Interface, Vec<Diagnostic>) {
    let mut sources = SourceMap::default();
    let file = sources.add("capture.fl", text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let module = resolve::Module {
        name: syntax.header.as_ref().expect("module header").name.as_str(),
        file,
        text,
        tree: &parsed.tree,
        syntax: &syntax,
    };
    let lookup = |name: &str| {
        imported
            .iter()
            .chain(types::builtins())
            .find(|i| i.module == name)
            .cloned()
    };
    let (mut interface, mut diagnostics) = resolve::resolve(&module, &lookup);
    diagnostics.extend(parsed.diagnostics);
    diagnostics.extend(types::check(
        &module,
        &lookup,
        types::builtins(),
        types::Options {
            local: &|_| true,
            limits: types::Limits::default(),
            sql: Default::default(),
            no_let_assert: false,
            thresholds: types::Thresholds::default(),
            effects: true,
        },
        &mut interface,
    ));
    diagnostics.retain(|d| !d.unnecessary());
    (interface, diagnostics)
}

fn library() -> &'static Interface {
    static LIBRARY: LazyLock<Interface> = LazyLock::new(|| {
        let (interface, diagnostics) = check(
            include_str!("../../tests/check/pass/regions/transformers.fl"),
            &[],
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let encoded = iface::write(&interface);
        let decoded = iface::read(encoded.as_bytes(), "App.Transformers").expect("round trip");
        assert_eq!(iface::write(&decoded), encoded);
        assert_eq!(iface::fingerprint(&decoded), iface::fingerprint(&interface));
        decoded
    });
    &LIBRARY
}

const CONSUMER: &str = "module App.Consumer exposing (..)\n\
import App.Transformers exposing (..)\n\
type Handle (s : Region)\n\
type error OpenError = OpenError String\n\
service Files =\n  open_scoped : forall s. Unit ->{Scope s; OpenError} Handle s\n\
touch : forall s. Handle s -> Number\n\
let touch handle =\n  ignore handle\n  1\n\n";

fn consumer(body: &str) -> Vec<Diagnostic> {
    check(&format!("{CONSUMER}{body}"), slice::from_ref(library())).1
}

fn rejects(body: &str, allowed: &[Code]) {
    let diagnostics = consumer(body);
    assert!(
        diagnostics.iter().any(|d| allowed.contains(&d.code)),
        "{body}\n{diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| allowed.contains(&d.code)),
        "unexpected diagnostic: {diagnostics:?}"
    );
}

#[test]
fn nominal_transformers_and_zero_bounds_round_trip() {
    let interface = library();
    let zero = interface
        .find(iface::Namespace::Value, "zero")
        .expect("zero");
    assert!(zero.scheme.as_ref().expect("type").contains(&mut |t| {
        matches!(t, types::Type::Bounded { captures, .. }
            if **captures == types::Type::empty_row())
    }));
    let diagnostics = consumer(
        r"inside : Unit ->{Files; OpenError} Number
let inside () =
  scope {
    let handle = Files.open_scoped ()
    let wrapped = Callback (fun () -> touch handle)
    match wrapped with
    | Callback callback -> callback ()
  }
",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn extracted_and_reboxed_callback_cannot_escape_an_imported_constructor() {
    rejects(
        r"leak : Unit ->{Files; OpenError} Unit -> Number
let leak () =
  scope {
    let handle = Files.open_scoped ()
    match Callback (fun () -> touch handle) with
    | Callback callback -> callback
  }
",
        &[codes::G001, codes::G002],
    );
    rejects(
        r"leak : Unit ->{Files; OpenError} Callback
let leak () =
  scope {
    let handle = Files.open_scoped ()
    match Callback (fun () -> touch handle) with
    | Callback callback -> Callback callback
  }
",
        &[codes::G001, codes::G002],
    );
}

#[test]
fn partial_map2_and_transformed_computations_keep_their_captures() {
    rejects(
        r"leak : Unit ->{Files; OpenError} Identity Unit -> Identity (Unit -> Number)
let leak () =
  scope {
    let handle = Files.open_scoped ()
    map2 (fun callback _ -> callback) (Identity (fun () -> touch handle))
  }
",
        &[codes::G001, codes::G002],
    );
    rejects(
        r"leak : Unit ->{Files; OpenError} ReaderT Unit Identity Number
let leak () =
  scope {
    let handle = Files.open_scoped ()
    ReaderT (fun () -> Identity (touch handle))
  }
",
        &[codes::G001, codes::G002],
    );
    rejects(
        r"leak : Unit ->{Files; OpenError} MaybeT (StateT Number (ReaderT Unit Identity)) Number
let leak () =
  scope {
    let handle = Files.open_scoped ()
    MaybeT (StateT (fun initial -> ReaderT (fun () -> Identity (Some (touch handle), initial))))
  }
",
        &[codes::G001, codes::G002],
    );
}

#[test]
fn a_zero_bound_cannot_narrow_generic_or_concrete_capture_data() {
    rejects(
        "narrow : forall a. a -> a^{}\nlet narrow value = value\n",
        &[codes::G001],
    );
    rejects(
        "narrow : forall s. Handle s -> (Handle s)^{}\nlet narrow value = value\n",
        &[codes::G001],
    );
    rejects(
        r"inside : Unit ->{Files; OpenError} Unit
let inside () =
  scope {
    let handle = Files.open_scoped ()
    ignore (zero (fun () -> touch handle))
  }
",
        &[codes::G001],
    );
}

#[test]
fn unqualified_data_patterns_do_not_claim_generic_payloads_are_capture_free() {
    rejects(
        r"report : Failure none ->{} Unit
let report _ = ()
spawn_hidden : forall s a. Fiber.Owner s -> Identity a ->{} Unit
let spawn_hidden owner (Identity value) =
  ignore (Fiber.spawn owner report (task { ignore value }))
",
        &[codes::G003],
    );
}

#[test]
fn curried_callback_stages_inherit_their_outer_environment() {
    let (_, diagnostics) = check(
        r"module App.Curried exposing (..)
apply_curried : forall c a. (Number -> a -> Number)^c -> a -> Number
let apply_curried f value = f 0 value
through_callback : forall c a. (a -> Number)^c -> a -> Number
let through_callback f value = apply_curried (fun _ x -> f x) value
",
        &[],
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn error_payloads_inspect_specialized_fields_not_constructor_metadata() {
    let source = r"module App.Errors exposing (..)
type Box a = Box (Option a)
type alias Detail a = Box a
type error CopyError = CopyError { details : Detail String; label : Option String }
";
    let (_, diagnostics) = check(source, &[]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let (_, diagnostics) = check(
        &format!("{source}type error LiveError = LiveError (Box (Ref Number))\n"),
        &[],
    );
    assert!(
        diagnostics.iter().any(|d| d.code == codes::E005),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| d.code == codes::E005),
        "{diagnostics:?}"
    );
}

#[test]
fn abstract_function_type_dependencies_cannot_hide_from_child_lifetimes() {
    rejects(
        r"report : Failure none ->{} Unit
let report _ = ()
spawn_none : forall s a. Fiber.Owner s -> (Option a -> Number) ->{} Unit
let spawn_none owner f =
  ignore (Fiber.spawn owner report (task { ignore (f None) }))
",
        &[codes::G003],
    );
}

#[test]
fn an_explicit_zero_bound_on_a_later_curried_stage_cannot_be_widened() {
    let (_, diagnostics) = check(
        r"module App.ExplicitZero exposing (..)
erase : forall c. (Unit -> Number)^c -> (Unit -> Number)^{}
let erase f () = f ()
",
        &[],
    );
    assert!(!diagnostics.is_empty());
    assert!(
        diagnostics
            .iter()
            .all(|d| [codes::G001, codes::G002].contains(&d.code)),
        "{diagnostics:?}"
    );
}

#[test]
fn generic_identity_cannot_widen_mutable_payload_qualifiers() {
    for (input, output) in [
        ("Ref ((Option a)^c)", "Ref (Option a)"),
        ("Ref ((Option a)^c)", "Ref ((Option a)^{})"),
        ("Ref (a^c)", "Ref a"),
        ("Ref (a^c)", "Ref (a^{})"),
    ] {
        let source = format!(
            r"module App.MutablePayload exposing (..)
forward : forall a. a -> a
let forward value = value
narrow : forall c a. {input} -> {output}
let narrow cell = forward cell
"
        );
        let (_, diagnostics) = check(&source, &[]);
        assert!(!diagnostics.is_empty(), "{input} -> {output}");
        assert!(
            diagnostics
                .iter()
                .all(|d| [codes::G001, codes::G002, codes::T003, codes::T008].contains(&d.code)),
            "{input} -> {output}: {diagnostics:?}"
        );
    }
}

#[test]
fn generic_identity_preserves_unchanged_mutable_and_immutable_bounds() {
    let (_, diagnostics) = check(
        r"module App.PreservedPayload exposing (..)
forward : forall a. a -> a
let forward value = value
keep_option_ref : forall c a. Ref ((Option a)^c) -> Ref ((Option a)^c)
let keep_option_ref cell = forward cell
keep_value_ref : forall c a. Ref (a^c) -> Ref (a^c)
let keep_value_ref cell = forward cell
keep_option : forall c a. (Option a)^c -> (Option a)^c
let keep_option value = forward value
keep_value : forall c a. a^c -> a^c
let keep_value value = forward value
",
        &[],
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn generic_identity_cannot_hide_a_concrete_region_under_a_value_bound() {
    let (_, diagnostics) = check(
        r"module App.ConcretePayload exposing (..)
type Handle (s : Region)
forward : forall a. a -> a
let forward value = value
narrow : forall s. (Option (Handle s))^{s} -> (Option (Handle s))^{}
let narrow value = forward value
",
        &[],
    );
    assert!(!diagnostics.is_empty());
    assert!(
        diagnostics
            .iter()
            .all(|d| [codes::G001, codes::G002].contains(&d.code)),
        "{diagnostics:?}"
    );
}

#[test]
fn capture_unions_choose_an_unblocked_tail_independently_of_order() {
    for union in ["c + d", "d + c"] {
        for union_first in [true, false] {
            for close_d in [false, true] {
                let combined = format!("(Unit -> Number)^({union})");
                let consumers = "((Unit -> Number)^c -> Number) -> ((Unit -> Number)^d -> Number)";
                let accept_d = if close_d {
                    "empty"
                } else {
                    "(fun callback -> callback ())"
                };
                let (parameters, arguments) = if union_first {
                    (
                        format!("{combined} -> {consumers} -> Number"),
                        format!("(fun () -> touch handle) empty {accept_d}"),
                    )
                } else {
                    (
                        format!("{consumers} -> {combined} -> Number"),
                        format!("empty {accept_d} (fun () -> touch handle)"),
                    )
                };
                let source = format!(
                    r"module App.UnionBounds exposing (..)
type Handle (s : Region)
touch : forall s. Handle s -> Number
let touch handle =
  ignore handle
  1
empty : (Unit -> Number)^{{}} -> Number
let empty callback = callback ()
accept_union : forall c d. {parameters}
let accept_union _ _ _ = 0
inside : forall s. Handle s -> Number
let inside handle = accept_union {arguments}
"
                );
                let (_, diagnostics) = check(&source, &[]);
                let case = format!("{union}, union_first={union_first}, close_d={close_d}");
                if close_d {
                    assert!(!diagnostics.is_empty(), "{case}");
                    assert!(
                        diagnostics
                            .iter()
                            .all(|d| [codes::G001, codes::G002].contains(&d.code)),
                        "{case}: {diagnostics:?}"
                    );
                } else {
                    assert!(diagnostics.is_empty(), "{case}: {diagnostics:?}");
                }
            }
        }
    }
}

#[test]
fn a_value_bound_cannot_widen_a_fixed_function_identity() {
    let (_, diagnostics) = check(
        r"module App.FunctionSlot exposing (..)
choose_value : forall c a. a -> (a^c -> a)^c
let choose_value exemplar =
  ignore exemplar
  fun value -> value
bad : forall c. (Unit -> Number)^c -> Unit -> Number
let bad read = choose_value (fun () -> 0) (fun () -> read ())
",
        &[],
    );
    assert!(
        diagnostics.iter().any(|d| d.code == codes::G001),
        "{diagnostics:?}"
    );
    rejects(
        r"get_fn : StateT (Unit -> Number) Identity (Unit -> Number)
let get_fn = StateT (fun initial -> Identity (initial, initial))
unit : StateT (Unit -> Number) Identity Unit
let unit = pure ()
deferred : StateT (Unit -> Number) Identity ((Eff none none Number)^{})
let deferred = do { f <- get_fn and _ <- unit; pure (task { f () }) }
erase : forall c. (Unit -> Number)^c -> Eff none none Number
let erase read =
  let Identity (later, _) = state deferred (fun () -> read ())
  later
",
        &[codes::G001, codes::G002],
    );
}

#[test]
fn inference_preserves_qualified_mutable_type_arguments() {
    let (_, diagnostics) = check(
        r"module App.RefSlot exposing (..)
forward_ref : forall a. Ref a -> Ref a
let forward_ref value = value
widen : forall c a. Ref (a^c) -> Ref a
let widen value = forward_ref value
",
        &[],
    );
    assert!(
        diagnostics.iter().any(|d| d.code == codes::T003),
        "{diagnostics:?}"
    );
    let (_, diagnostics) = check(
        r"module App.RefSlot exposing (..)
forward_ref : forall a. Ref a -> Ref a
let forward_ref value = value
keep : forall c a. Ref (a^c) -> Ref (a^c)
let keep value = forward_ref value
",
        &[],
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn constructor_capture_binders_do_not_shadow_payload_quantifiers() {
    let diagnostics = consumer(
        r"type PolyCallback = PolyCallback (forall captures. Number -> Number)
inside : Unit ->{Files; OpenError} Number
let inside () =
  scope {
    let handle = Files.open_scoped ()
    match PolyCallback (fun _ -> touch handle) with
    | PolyCallback callback -> callback 0
  }
",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_mutable_alias_cannot_invalidate_an_empty_capture_bound() {
    for receiver in ["ref", "wide"] {
        let alias = if receiver == "wide" {
            "wide : (Ref a)^c\n  let wide = ref\n  "
        } else {
            ""
        };
        let source = format!(
            r"module App.RefCapacity exposing (..)
unsafe : forall c a. a^c -> a^{{}} ->{{}} (Eff none none Unit)^{{}}
let unsafe full empty =
  let ref = Ref.make empty
  {alias}Ref.set {receiver} full
  task {{ ignore (Ref.get ref) }}
"
        );
        let (_, diagnostics) = check(&source, &[]);
        assert!(
            diagnostics
                .iter()
                .any(|d| [codes::G001, codes::G002].contains(&d.code)),
            "{receiver}: {diagnostics:?}"
        );
        assert!(
            diagnostics
                .iter()
                .all(|d| [codes::G001, codes::G002].contains(&d.code)),
            "{receiver}: {diagnostics:?}"
        );
    }
}

#[test]
fn recursive_copy_data_does_not_retain_a_mapping_callbacks_captures() {
    let (_, diagnostics) = check(
        r#"module App.JsonData exposing (..)
type Handle (s : Region)
type Value = Text String | Array (List Value) | Object (List (String, Value))
convert : forall s. Handle s -> List String -> Value
let convert handle values =
  Object [("values", Array (List.map (fun text ->
    ignore handle
    Text text) values))]
"#,
        &[],
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
