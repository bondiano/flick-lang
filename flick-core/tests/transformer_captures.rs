//! Capture flows of transformers under typing by use: a pure callback over
//! a scoped handle uses nothing of it, so it may be held anywhere, and a
//! callback that uses the handle keeps the region in its type
//! (`transformer_captures/mtl.rs`, `effectful_callbacks_keep_their_region_through_transformers`).

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
        generated: resolve::NO_GENERATED,
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
            no_let_assert: false,
            thresholds: types::Thresholds::default(),
            effects: true,
            watched: types::Watched::none(),
            stages_by_class: false,
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
touch : forall s. Handle s -> Float\n\
let touch handle =\n  ignore handle\n  1\n\n";

fn consumer(body: &str) -> Vec<Diagnostic> {
    check(&format!("{CONSUMER}{body}"), slice::from_ref(library())).1
}

/// Holding a pure callback past its `scope` uses nothing of it: such a
/// program is accepted.
fn accepts(body: &str) {
    let diagnostics = consumer(body);
    assert!(diagnostics.is_empty(), "{body}\n{diagnostics:?}");
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
        r"inside : Unit ->{Files; OpenError} Float
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
fn an_extracted_pure_callback_may_leave_through_an_imported_constructor() {
    accepts(
        r"leak : Unit ->{Files; OpenError} Unit -> Float
let leak () =
  scope {
    let handle = Files.open_scoped ()
    match Callback (fun () -> touch handle) with
    | Callback callback -> callback
  }
",
    );
    accepts(
        r"leak : Unit ->{Files; OpenError} Callback
let leak () =
  scope {
    let handle = Files.open_scoped ()
    match Callback (fun () -> touch handle) with
    | Callback callback -> Callback callback
  }
",
    );
}

#[test]
fn partial_map2_and_transformed_computations_may_hold_pure_callbacks() {
    accepts(
        r"leak : Unit ->{Files; OpenError} Identity Unit -> Identity (Unit -> Float)
let leak () =
  scope {
    let handle = Files.open_scoped ()
    map2 (fun callback _ -> callback) (Identity (fun () -> touch handle))
  }
",
    );
    accepts(
        r"leak : Unit ->{Files; OpenError} ReaderT Unit Identity Float
let leak () =
  scope {
    let handle = Files.open_scoped ()
    ReaderT (fun () -> Identity (touch handle))
  }
",
    );
    accepts(
        r"leak : Unit ->{Files; OpenError} MaybeT (StateT Float (ReaderT Unit Identity)) Float
let leak () =
  scope {
    let handle = Files.open_scoped ()
    MaybeT (StateT (fun initial -> ReaderT (fun () -> Identity (Some (touch handle), initial))))
  }
",
    );
}

#[test]
fn a_zero_bound_cannot_narrow_generic_or_concrete_capture_data() {
    accepts("narrow : forall a. a -> a^{}\nlet narrow value = value\n");
    rejects(
        "narrow : forall s. Handle s -> (Handle s)^{}\nlet narrow value = value\n",
        &[codes::G001],
    );
    accepts(
        r"inside : Unit ->{Files; OpenError} Unit
let inside () =
  scope {
    let handle = Files.open_scoped ()
    ignore (zero (fun () -> touch handle))
  }
",
    );
}

#[test]
fn unqualified_data_patterns_do_not_claim_generic_payloads_are_capture_free() {
    accepts(
        r"report : Failure none ->{} Unit
let report _ = ()
spawn_hidden : forall s a. Fiber.Owner s -> Identity a ->{} Unit
let spawn_hidden owner (Identity value) =
  ignore (Fiber.spawn owner report (task { ignore value }))
",
    );
}

#[test]
fn curried_callback_stages_inherit_their_outer_environment() {
    let (_, diagnostics) = check(
        r"module App.Curried exposing (..)
apply_curried : forall c a. (Float -> a -> Float)^c -> a -> Float
let apply_curried f value = f 0 value
through_callback : forall c a. (a -> Float)^c -> a -> Float
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
        &format!("{source}type error LiveError = LiveError (Box (Ref Float))\n"),
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
fn pure_function_dependencies_need_nothing_of_child_lifetimes() {
    accepts(
        r"report : Failure none ->{} Unit
let report _ = ()
spawn_none : forall s a. Fiber.Owner s -> (Option a -> Float) ->{} Unit
let spawn_none owner f =
  ignore (Fiber.spawn owner report (task { ignore (f None) }))
",
    );
}

#[test]
fn a_zero_bound_on_a_pure_stage_says_nothing() {
    let (_, diagnostics) = check(
        r"module App.ExplicitZero exposing (..)
erase : forall c. (Unit -> Float)^c -> (Unit -> Float)^{}
let erase f () = f ()
",
        &[],
    );
    // calling a pure function needs nothing live: its qualifier says nothing
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
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
                let combined = format!("(Unit -> Float)^({union})");
                let consumers = "((Unit -> Float)^c -> Float) -> ((Unit -> Float)^d -> Float)";
                let accept_d = if close_d {
                    "empty"
                } else {
                    "(fun callback -> callback ())"
                };
                let (parameters, arguments) = if union_first {
                    (
                        format!("{combined} -> {consumers} -> Float"),
                        format!("(fun () -> touch handle) empty {accept_d}"),
                    )
                } else {
                    (
                        format!("{consumers} -> {combined} -> Float"),
                        format!("empty {accept_d} (fun () -> touch handle)"),
                    )
                };
                let source = format!(
                    r"module App.UnionBounds exposing (..)
type Handle (s : Region)
touch : forall s. Handle s -> Float
let touch handle =
  ignore handle
  1
empty : (Unit -> Float)^{{}} -> Float
let empty callback = callback ()
accept_union : forall c d. {parameters}
let accept_union _ _ _ = 0
inside : forall s. Handle s -> Float
let inside handle = accept_union {arguments}
"
                );
                let (_, diagnostics) = check(&source, &[]);
                let case = format!("{union}, union_first={union_first}, close_d={close_d}");
                // a pure callback over a handle uses nothing of it, whichever
                // tail of the union takes it
                assert!(diagnostics.is_empty(), "{case}: {diagnostics:?}");
                let _ = close_d;
            }
        }
    }
}

#[test]
fn a_value_bound_on_a_pure_function_says_nothing() {
    let (_, diagnostics) = check(
        r"module App.FunctionSlot exposing (..)
choose_value : forall c a. a -> (a^c -> a)^c
let choose_value exemplar =
  ignore exemplar
  fun value -> value
bad : forall c. (Unit -> Float)^c -> Unit -> Float
let bad read = choose_value (fun () -> 0) (fun () -> read ())
",
        &[],
    );
    // what a pure function returns is used by no stage that needs a region
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    accepts(
        r"get_fn : StateT (Unit -> Float) Identity (Unit -> Float)
let get_fn = StateT (fun initial -> Identity (initial, initial))
unit : StateT (Unit -> Float) Identity Unit
let unit = pure ()
deferred : StateT (Unit -> Float) Identity ((Eff none none Float)^{})
let deferred = do { f <- get_fn and _ <- unit; pure (task { f () }) }
erase : forall c. (Unit -> Float)^c -> Eff none none Float
let erase read =
  let Identity (later, _) = state deferred (fun () -> read ())
  later
",
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
        r"type PolyCallback = PolyCallback (forall captures. Float -> Float)
inside : Unit ->{Files; OpenError} Float
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
fn a_task_that_reads_a_cell_uses_nothing_it_holds() {
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
        // the task reads the cell and uses nothing of what the value holds
        assert!(diagnostics.is_empty(), "{receiver}: {diagnostics:?}");
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
