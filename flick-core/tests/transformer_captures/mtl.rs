use std::sync::LazyLock;

use flick_core::diagnostics::{Code, Diagnostic, Severity, codes};
use flick_core::iface::{self, Interface};

use super::check;

fn library() -> &'static [Interface] {
    static LIBRARY: LazyLock<Vec<Interface>> = LazyLock::new(|| {
        let mut interfaces = Vec::new();
        for (name, source) in [
            (
                "Mtl.Classes",
                include_str!("../../../examples/mtl/src/Mtl/Classes.fl"),
            ),
            (
                "Mtl.Identity",
                include_str!("../../../examples/mtl/src/Mtl/Identity.fl"),
            ),
            (
                "Mtl.MaybeT",
                include_str!("../../../examples/mtl/src/Mtl/MaybeT.fl"),
            ),
            (
                "Mtl.StateT",
                include_str!("../../../examples/mtl/src/Mtl/StateT.fl"),
            ),
            (
                "Mtl.ReaderT",
                include_str!("../../../examples/mtl/src/Mtl/ReaderT.fl"),
            ),
            (
                "Mtl.WriterT",
                include_str!("../../../examples/mtl/src/Mtl/WriterT.fl"),
            ),
            (
                "Mtl.ExceptT",
                include_str!("../../../examples/mtl/src/Mtl/ExceptT.fl"),
            ),
        ] {
            let (interface, diagnostics) = check(source, &interfaces);
            assert!(diagnostics.is_empty(), "{name}: {diagnostics:?}");
            let encoded = iface::write(&interface);
            let decoded = iface::read(encoded.as_bytes(), name).expect("MTL interface round trip");
            assert_eq!(iface::write(&decoded), encoded, "{name}");
            assert_eq!(iface::fingerprint(&decoded), iface::fingerprint(&interface));
            interfaces.push(decoded);
        }
        interfaces
    });
    &LIBRARY
}

const CONSUMER: &str = r"module App.Consumer exposing (..)
import Mtl.Classes exposing (..)
import Mtl.Identity as Identity exposing (Identity(..))
import Mtl.MaybeT as MaybeT exposing (MaybeT(..))
import Mtl.StateT as StateT exposing (StateT(..))
import Mtl.ReaderT as ReaderT exposing (ReaderT(..))
import Mtl.WriterT as WriterT exposing (WriterT(..))
import Mtl.ExceptT as ExceptT exposing (ExceptT(..))
type alias Log = List (Unit -> Float)
type alias HeldLog (c : CaptureRow) = List ((Unit -> Float)^c)
type Handle (s : Region)
type error OpenError = OpenError String
service Files =
  open_scoped : forall s. Unit ->{Scope s; OpenError} Handle s
touch : forall s. Handle s -> Float
let touch handle =
  ignore handle
  1
consume_log : forall c. HeldLog c -> Float
let consume_log output =
  apply : (Unit -> Float)^c -> Float
  let apply read = read ()
  List.sum (List.map apply output)

";

fn consumer(body: &str) -> Vec<Diagnostic> {
    check(&format!("{CONSUMER}{body}"), library()).1
}

fn accepts(body: &str) {
    // a lint on how the body is written is not a rejection
    let errors: Vec<_> = consumer(body)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{body}\n{errors:?}");
}

fn rejects(body: &str, allowed: &[Code]) -> Vec<Diagnostic> {
    let diagnostics = consumer(body);
    assert!(
        diagnostics.iter().any(|d| allowed.contains(&d.code)),
        "{body}\n{diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|d| allowed.contains(&d.code)),
        "unexpected diagnostic in {body}\n{diagnostics:?}"
    );
    diagnostics
}

#[test]
fn actual_mtl_interfaces_allow_construction_run_and_consumption_within_scope() {
    accepts(
        r"inside : Unit ->{Files; OpenError} Float
let inside () =
  scope resource {
    let handle = Files.open_scoped ()
    optional : MaybeT Identity Float
    let optional = map (fun _ -> touch handle) (pure ())
    transition : StateT Float Identity Float
    let transition = state (fun old -> (touch handle, old + 1))
    projection : ReaderT Float Identity Float
    let projection = reader (fun environment -> environment + touch handle)
    output : WriterT (HeldLog {resource}) Identity Float
    let output = writer (2, [fun () -> touch handle])
    failed : ExceptT ((Unit -> Float)^{resource}) Identity Float
    let failed = throw_error (fun () -> touch handle)
    let recovered = catch_error (fun read -> pure (read ())) failed
    let maybe_value = match Identity.run (MaybeT.run optional) with
      | None -> 0
      | Some value -> value
    let (state_value, final_state) = Identity.run (StateT.run transition 3)
    let reader_value = Identity.run (ReaderT.run projection 5)
    let ((writer_value, heard), emitted) = Identity.run (WriterT.run (listen output))
    let error_value = match Identity.run (ExceptT.run recovered) with
      | Err read -> read ()
      | Ok value -> value
    let heard_value = match heard with
      | [] -> 0
      | [read, ..._] -> read ()
    let emitted_value = match emitted with
      | [] -> 0
      | [read, ..._] -> read ()
    maybe_value + state_value + final_state + reader_value + writer_value + error_value +
      heard_value + emitted_value
  }
",
    );
}

#[test]
fn writer_output_and_returned_writer_actions_cannot_escape() {
    for (result, projection) in [
        ("WriterT Log Identity Unit", "action"),
        (
            "Log",
            "let (_, output) = Identity.run (WriterT.run action)\n    output",
        ),
    ] {
        rejects(
            &format!(
                r"leak : Unit ->{{Files; OpenError}} {result}
let leak () =
  scope {{
    let handle = Files.open_scoped ()
    action : WriterT Log Identity Unit
    let action = tell [fun () -> touch handle]
    {projection}
  }}
"
            ),
            &[codes::G001, codes::G002],
        );
    }
}

#[test]
fn except_error_data_and_returned_except_actions_cannot_escape() {
    for (result, projection) in [
        ("ExceptT (Unit -> Float) Identity Unit", "action"),
        (
            "Result (Unit -> Float) Unit",
            "Identity.run (ExceptT.run action)",
        ),
    ] {
        rejects(
            &format!(
                r"leak : Unit ->{{Files; OpenError}} {result}
let leak () =
  scope {{
    let handle = Files.open_scoped ()
    action : ExceptT (Unit -> Float) Identity Unit
    let action = throw_error (fun () -> touch handle)
    {projection}
  }}
"
            ),
            &[codes::G001, codes::G002],
        );
    }
}

#[test]
fn other_actual_transformers_preserve_captures_in_returned_computations() {
    for (result, action) in [
        (
            "MaybeT Identity (Unit -> Float)",
            "pure (fun () -> touch handle)",
        ),
        (
            "StateT Float Identity Float",
            "state (fun old -> (touch handle, old + 1))",
        ),
        (
            "ReaderT Float Identity Float",
            "reader (fun environment -> environment + touch handle)",
        ),
    ] {
        rejects(
            &format!(
                r"leak : Unit ->{{Files; OpenError}} {result}
let leak () =
  scope {{
    let handle = Files.open_scoped ()
    {action}
  }}
"
            ),
            &[codes::G001, codes::G002],
        );
    }
}

#[test]
fn actual_transformer_run_projections_preserve_captures() {
    for (action_type, projection) in [
        (
            "MaybeT Identity (Unit -> Float)",
            "match Identity.run (MaybeT.run action) with\n    | None -> fun () -> 0\n    | Some read -> read",
        ),
        (
            "StateT Float Identity (Unit -> Float)",
            "let (read, _) = Identity.run (StateT.run action 0)\n    read",
        ),
        (
            "ReaderT Unit Identity (Unit -> Float)",
            "Identity.run (ReaderT.run action ())",
        ),
    ] {
        rejects(
            &format!(
                r"leak : Unit ->{{Files; OpenError}} Unit -> Float
let leak () =
  scope {{
    let handle = Files.open_scoped ()
    action : {action_type}
    let action = pure (fun () -> touch handle)
    {projection}
  }}
"
            ),
            &[codes::G001, codes::G002],
        );
    }
}

#[test]
fn native_and_inherited_listen_projections_cannot_erase_log_captures() {
    rejects(
        r"leak : Unit ->{Files; OpenError} Log
let leak () =
  scope {
    let handle = Files.open_scoped ()
    action : WriterT Log Identity Unit
    let action = tell [fun () -> touch handle]
    let ((_, heard), _) = Identity.run (WriterT.run (listen action))
    heard
  }
",
        &[codes::G001, codes::G002],
    );
    rejects(
        r"leak : Unit ->{Files; OpenError} Log
let leak () =
  scope {
    let handle = Files.open_scoped ()
    action : MaybeT (WriterT Log Identity) Unit
    let action = tell [fun () -> touch handle]
    let (optional, _) = Identity.run (WriterT.run (MaybeT.run (listen action)))
    match optional with
    | None -> []
    | Some (_, heard) -> heard
  }
",
        &[codes::G001, codes::G002],
    );
}

#[test]
fn native_and_inherited_catch_projections_cannot_erase_error_captures() {
    for (action_type, projection) in [
        (
            "ExceptT (Unit -> Float) Identity (Unit -> Float)",
            "Identity.run (ExceptT.run recovered)",
        ),
        (
            "ReaderT Unit (ExceptT (Unit -> Float) Identity) (Unit -> Float)",
            "Identity.run (ExceptT.run (ReaderT.run recovered ()))",
        ),
    ] {
        rejects(
            &format!(
                r"leak : Unit ->{{Files; OpenError}} Unit -> Float
let leak () =
  scope {{
    let handle = Files.open_scoped ()
    action : {action_type}
    let action = throw_error (fun () -> touch handle)
    let recovered = catch_error (fun read -> pure read) action
    match {projection} with
    | Err read -> read
    | Ok read -> read
  }}
"
            ),
            &[codes::G001, codes::G002],
        );
    }
}

#[test]
fn partially_applied_operations_retain_scoped_callbacks() {
    for (result, operation) in [
        (
            "(ReaderT (Unit -> Float) Identity Float -> ReaderT (Unit -> Float) Identity Float)",
            "local (fun _ -> fun () -> touch handle)",
        ),
        (
            "(ExceptT String Identity Float -> ExceptT String Identity Float)",
            "catch_error (fun _ -> pure (touch handle))",
        ),
        (
            "(Unit -> Identity Float)",
            "ReaderT.run (ReaderT (fun () -> Identity (touch handle)))",
        ),
    ] {
        rejects(
            &format!(
                r"leak : Unit ->{{Files; OpenError}} {result}
let leak () =
  scope {{
    let handle = Files.open_scoped ()
    {operation}
  }}
"
            ),
            &[codes::G001, codes::G002],
        );
    }
}

#[test]
fn native_pass_preserves_captures_of_a_stored_modifier() {
    for (result, observation, escapes) in [
        ("Float", "consume_log output", false),
        ("Log", "output", true),
    ] {
        let source = format!(
            r"observe : Unit ->{{Files; OpenError}} {result}
let observe () =
  scope resource {{
    let handle = Files.open_scoped ()
    modify : (HeldLog {{resource}} -> HeldLog {{resource}})^{{resource}}
    let modify output = [fun () -> touch handle] ++ output
    action : WriterT (HeldLog {{resource}}) Identity (Unit, (HeldLog {{resource}} -> HeldLog {{resource}})^{{resource}})
    let action = writer (((), modify), [])
    let (_, output) = Identity.run (WriterT.run (pass action))
    {observation}
  }}
"
        );
        if escapes {
            let diagnostics = rejects(&source, &[codes::G001, codes::G002, codes::T003]);
            assert!(
                diagnostics
                    .iter()
                    .any(|d| [codes::G001, codes::G002].contains(&d.code)),
                "{source}\n{diagnostics:?}"
            );
        } else {
            accepts(&source);
        }
    }
}

#[test]
fn lifted_pass_tracks_a_modifier_supplied_by_a_later_reader_environment() {
    for (result, observation, escapes) in [
        ("Float", "consume_log output", false),
        ("Log", "output", true),
    ] {
        let source = format!(
            r"from_environment :
  forall c d. ReaderT ((HeldLog c -> HeldLog c)^d) (WriterT (HeldLog c) Identity)
    (Unit, (HeldLog c -> HeldLog c)^d)
let from_environment = do {{ modify <- ask; pure ((), modify) }}
observe : Unit ->{{Files; OpenError}} {result}
let observe () =
  scope resource {{
    action :
      ReaderT ((HeldLog {{resource}} -> HeldLog {{resource}})^{{resource}})
        (WriterT (HeldLog {{resource}}) Identity)
        (Unit, (HeldLog {{resource}} -> HeldLog {{resource}})^{{resource}})
    let action = from_environment
    let handle = Files.open_scoped ()
    modify : (HeldLog {{resource}} -> HeldLog {{resource}})^{{resource}}
    let modify output = [fun () -> touch handle] ++ output
    let (_, output) = Identity.run (WriterT.run (ReaderT.run (pass action) modify))
    {observation}
  }}
"
        );
        if escapes {
            let diagnostics = rejects(&source, &[codes::G001, codes::G002, codes::T003]);
            assert!(
                diagnostics
                    .iter()
                    .any(|d| [codes::G001, codes::G002].contains(&d.code)),
                "{source}\n{diagnostics:?}"
            );
        } else {
            accepts(&source);
        }
    }
}

#[test]
fn lifted_pass_tracks_a_log_supplied_by_a_later_reader_environment() {
    for (result, observation, escapes) in [
        ("Float", "consume_log output", false),
        ("Log", "output", true),
    ] {
        let source = format!(
            r"from_environment :
  forall c. ReaderT (HeldLog c) (WriterT (HeldLog c) Identity)
    (Unit, (HeldLog c -> HeldLog c)^{{}})
let from_environment =
  unchanged : (HeldLog c -> HeldLog c)^{{}}
  let unchanged output = output
  do {{ output <- ask; tell output; pure ((), unchanged) }}
observe : Unit ->{{Files; OpenError}} {result}
let observe () =
  scope resource {{
    action :
      ReaderT (HeldLog {{resource}}) (WriterT (HeldLog {{resource}}) Identity)
        (Unit, (HeldLog {{resource}} -> HeldLog {{resource}})^{{}})
    let action = from_environment
    let handle = Files.open_scoped ()
    let (_, output) = Identity.run (WriterT.run (ReaderT.run (pass action) [fun () -> touch handle]))
    {observation}
  }}
"
        );
        if escapes {
            let diagnostics = rejects(&source, &[codes::G001, codes::G002, codes::T003]);
            assert!(
                diagnostics
                    .iter()
                    .any(|d| [codes::G001, codes::G002].contains(&d.code)),
                "{source}\n{diagnostics:?}"
            );
        } else {
            accepts(&source);
        }
    }
}

#[test]
fn outer_refs_cannot_store_scoped_actions_or_projected_logs() {
    rejects(
        r"store : Unit ->{Files; OpenError} Unit
let store () =
  empty_action : WriterT Log Identity Unit
  let empty_action = pure ()
  let cell = Ref.make empty_action
  scope {
    let handle = Files.open_scoped ()
    Ref.set cell (tell [fun () -> touch handle])
  }
",
        &[codes::G001, codes::G002],
    );
    rejects(
        r"store : Unit ->{Files; OpenError} Unit
let store () =
  let cell = Ref.make []
  scope {
    let handle = Files.open_scoped ()
    action : WriterT Log Identity Unit
    let action = tell [fun () -> touch handle]
    let (_, output) = Identity.run (WriterT.run action)
    Ref.set cell output
  }
",
        &[codes::G001, codes::G002],
    );
}

#[test]
fn mutable_actual_transformer_payload_bounds_remain_invariant() {
    for payload in [
        "WriterT Log Identity Unit",
        "ExceptT (Unit -> Float) Identity Unit",
    ] {
        rejects(
            &format!(
                r"narrow : forall c. Ref (({payload})^c) -> Ref (({payload})^{{}})
let narrow cell = cell
"
            ),
            &[codes::T008],
        );
    }
}

#[test]
fn transformed_actions_require_a_child_owner_that_outlives_their_captures() {
    for owner in ["outer", "inner"] {
        let source = format!(
            r"report : Failure none ->{{}} Unit
let report _ = ()
children : Unit ->{{Files; OpenError}} Unit
let children () =
  scope outer {{
    scope inner {{
      let handle = Files.open_scoped ()
      action : WriterT (HeldLog {{inner}}) Identity Unit
      let action = tell [fun () -> touch handle]
      ignore (Fiber.spawn {owner} report (task {{
        let (_, output) = Identity.run (WriterT.run action)
        ignore (consume_log output)
      }}))
    }}
  }}
"
        );
        if owner == "outer" {
            rejects(&source, &[codes::G003]);
        } else {
            accepts(&source);
        }
    }
}

#[test]
fn every_generic_lift_preserves_the_base_actions_captures() {
    for (transformed, qualified, observation) in [
        (
            "MaybeT Identity (Unit -> Float)",
            "MaybeT Identity ((Unit -> Float)^{resource})",
            "match Identity.run (MaybeT.run action) with\n    | None -> 0\n    | Some read -> read ()",
        ),
        (
            "StateT Float Identity (Unit -> Float)",
            "StateT Float Identity ((Unit -> Float)^{resource})",
            "let (read, _) = Identity.run (StateT.run action 0)\n    read ()",
        ),
        (
            "ReaderT Unit Identity (Unit -> Float)",
            "ReaderT Unit Identity ((Unit -> Float)^{resource})",
            "(Identity.run (ReaderT.run action ())) ()",
        ),
        (
            "WriterT (List String) Identity (Unit -> Float)",
            "WriterT (List String) Identity ((Unit -> Float)^{resource})",
            "let (read, _) = Identity.run (WriterT.run action)\n    read ()",
        ),
        (
            "ExceptT String Identity (Unit -> Float)",
            "ExceptT String Identity ((Unit -> Float)^{resource})",
            "match Identity.run (ExceptT.run action) with\n    | Err _ -> 0\n    | Ok read -> read ()",
        ),
    ] {
        for escapes in [false, true] {
            let result = if escapes { transformed } else { "Float" };
            let projection = if escapes { "action" } else { observation };
            let action_type = if escapes { transformed } else { qualified };
            let source = format!(
                r"observe : Unit ->{{Files; OpenError}} {result}
let observe () =
  scope resource {{
    let handle = Files.open_scoped ()
    action : {action_type}
    let action = lift (Identity (fun () -> touch handle))
    {projection}
  }}
"
            );
            if escapes {
                rejects(&source, &[codes::G001, codes::G002]);
            } else {
                accepts(&source);
            }
        }
    }
}
