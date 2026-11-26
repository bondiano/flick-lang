//! End-to-end tests for polymorphic specialization with boxed-cell fallback
//! (#743), compiling real Flick source programs through `flick-core` into
//! typed `flick_ir::Program`, running `flick_backend_support::specialize`,
//! inspecting boxed-cell counts and layouts, and executing the output on a
//! genuine layout-aware fixture backend.

#![cfg(feature = "native-project")]

mod backend;
mod builtins;
mod ops;

use flick_backend_support::layout::{Discriminant, PrimitiveTable, TypeRepr};
use flick_backend_support::specialize::{
    BudgetKind, DefMode, FallbackReason, SpecializeConfig, SpecializeError, specialize,
};
use flick_ir::{IrType, TypeRef};

use backend::{FixtureBackend, Val, compile_and_specialize, compile_flick};

/// AC 1: Real Flick programs computing a `sum` over `List Int` and a `map`
/// over `Option Float` compile with ZERO boxed cells and execute to exact
/// expected values.
#[test]
fn ac1_sum_list_int_and_map_option_float_have_zero_boxed_cells() {
    let source = r"
module App.Main exposing (run)

fold : (b -> a -> b) -> b -> List a -> b
let rec fold f acc xs =
  match xs with
  | [] -> acc
  | [x, ...rest] -> fold f (f acc x) rest

sum_ints : List Int -> Int
let sum_ints xs = fold (fun a b -> a + b) 0 xs

run : Unit -> (Int, Option Float, Option Float)
let run () =
  let total = sum_ints [10, 20, 30, 40]
  let mapped_some = Option.map (fun x -> x * 2.5) (Some 4.0)
  let mapped_none = Option.map (fun x -> x * 2.5) None
  (total, mapped_some, mapped_none)
";

    let spec = compile_and_specialize(
        source,
        &[("App.Main", "run")],
        &PrimitiveTable::target_64(),
        &SpecializeConfig::default(),
    )
    .expect("AC 1 program must specialize cleanly");

    // ZERO boxed cells statically AND at runtime!
    assert_eq!(
        spec.stats.boxed_cells, 0,
        "expected zero boxed cells: {:?}",
        spec.stats
    );
    assert_eq!(spec.stats.fallback_definitions, 0);
    assert!(spec.stats.specialized_instances >= 3);

    // Concrete `Option Float` layout is computed and recorded via LayoutEngine.
    let opt_float = TypeRef::new(IrType::Named {
        module: "Prelude".into(),
        name: "Option".into(),
        args: vec![TypeRef::new(IrType::Named {
            module: "Prelude".into(),
            name: "Float".into(),
            args: Vec::new(),
        })],
    });
    let layout = spec
        .layouts
        .get(&opt_float)
        .expect("Option Float layout must be computed");
    assert_eq!(layout.size, 16);
    assert_eq!(layout.align, 8);
    assert!(matches!(
        &layout.repr,
        TypeRepr::Data(d) if matches!(d.discriminant, Discriminant::Tag { width: 1, .. })
    ));

    let mut ev = FixtureBackend::new(&spec);
    let result = ev.run_root("App.Main", "run");
    assert_eq!(ev.stats.runtime_boxes, 0);
    assert_eq!(ev.stats.runtime_unboxes, 0);

    match result {
        Val::Tuple(items) => {
            assert!(matches!(items[0], Val::Int(100)));
            match &items[1] {
                Val::Con { name, fields, .. } if name == "Some" => {
                    assert!(matches!(fields[0], Val::Float(f) if (f - 10.0).abs() < 1e-12));
                }
                other => panic!("expected Some 10.0, got {other:?}"),
            }
            assert!(
                matches!(&items[2], Val::Con { name, fields, .. } if name == "None" && fields.is_empty())
            );
        }
        other => panic!("expected 3-tuple, got {other:?}"),
    }
}

/// AC 2(a) & AC 4: Polymorphic recursion against a `forall` signature is
/// statically classified, compiles ONCE over boxed cells without hitting a
/// tiny instantiation budget, and executes to the exact expected value.
#[test]
fn ac2a_polymorphic_recursion_compiles_once_over_boxed_cells() {
    let source = r"
module App.Main exposing (run)

type Nested a = Flat a | Nest (Nested (List a))

depth : forall a. Nested a -> Int
let rec depth value =
  match value with
  | Flat _ -> 0
  | Nest inner -> 1 + depth inner

run : Unit -> Int
let run () = depth (Nest (Nest (Flat [[1, 2], [3]])))
";

    let cfg = SpecializeConfig {
        max_instances: 6,
        ..SpecializeConfig::default()
    };
    let spec = compile_and_specialize(
        source,
        &[("App.Main", "run")],
        &PrimitiveTable::target_64(),
        &cfg,
    )
    .expect("polymorphic recursion must not exceed budget");

    assert!(spec.definitions.values().any(|d| {
        d.source == ("App.Main".into(), "depth".into())
            && d.mode == DefMode::Fallback(FallbackReason::PolymorphicRecursion)
    }));
    assert!(spec.stats.boxed_cells > 0);

    let mut ev = FixtureBackend::new(&spec);
    let res = ev.run_root("App.Main", "run");
    assert!(matches!(res, Val::Int(2)), "expected 2, got {res:?}");
    assert!(ev.stats.runtime_boxes > 0);
    assert!(ev.stats.runtime_unboxes > 0);
}

/// AC 2(b): Higher-rank callbacks (`forall a. ...` in parameter position)
/// compile with boxed cells at the boundary and execute accurately.
#[test]
fn ac2b_higher_rank_callback_boxes_at_boundary_and_executes() {
    let source = r#"
module App.Main exposing (run)

apply_pair : (forall a. a -> a) -> (Int, String) -> (Int, String)
let apply_pair f pair =
  let (n, s) = pair
  (f n, f s)

each_shown : (forall a. Show a => a -> String) -> String
let each_shown f = f 42 ++ ":" ++ f true

run : Unit -> (Int, String, String)
let run () =
  let (n, s) = apply_pair (fun x -> x) (7, "ok")
  let shown = each_shown (fun x -> show x)
  (n, s, shown)
"#;

    let spec = compile_and_specialize(
        source,
        &[("App.Main", "run")],
        &PrimitiveTable::target_64(),
        &SpecializeConfig::default(),
    )
    .expect("higher-rank callback program must specialize");

    assert!(spec.definitions.values().any(|d| {
        d.source == ("App.Main".into(), "apply_pair".into())
            && d.mode == DefMode::Fallback(FallbackReason::HigherRankCallback)
    }));
    assert!(spec.definitions.values().any(|d| {
        d.source == ("App.Main".into(), "each_shown".into())
            && d.mode == DefMode::Fallback(FallbackReason::HigherRankCallback)
    }));
    assert!(spec.stats.boxed_cells > 0);

    let mut ev = FixtureBackend::new(&spec);
    let res = ev.run_root("App.Main", "run");
    match res {
        Val::Tuple(items) => {
            assert!(matches!(items[0], Val::Int(7)));
            assert!(matches!(items[1], Val::String(ref s) if s == "ok"));
            assert!(matches!(items[2], Val::String(ref s) if s == "42:true"));
        }
        other => panic!("expected (7, \"ok\", \"42:true\"), got {other:?}"),
    }
}

/// AC 2(c): Packed modules and existentials box abstract members across the
/// `pack` / `unpack` boundary and execute to the expected results.
#[test]
fn ac2c_packed_module_and_existential_box_at_boundary() {
    let source = r#"
module App.Main exposing (run)

type module Box =
  type T
  value : T
  describe : T -> String

module IntBox exposing (T, value, describe) =
  type alias T = Int
  value : T
  let value = 42
  describe : T -> String
  let describe n = "int:" ++ show n

module BoolBox exposing (T, value, describe) =
  type alias T = Bool
  value : T
  let value = true
  describe : T -> String
  let describe b = "bool:" ++ show b

use_box : Module Box -> String
let use_box m = unpack m as B in B.describe B.value

run : Unit -> String
let run () =
  let a = use_box (pack IntBox as Box)
  let b = use_box (pack BoolBox as Box)
  a ++ " / " ++ b
"#;

    let spec = compile_and_specialize(
        source,
        &[("App.Main", "run")],
        &PrimitiveTable::target_64(),
        &SpecializeConfig::default(),
    )
    .expect("packed module program must specialize");

    assert!(spec.definitions.values().any(|d| {
        d.source == ("App.Main".into(), "use_box".into())
            && d.mode == DefMode::Fallback(FallbackReason::PackedOrExistential)
    }));
    assert!(spec.stats.boxed_cells > 0);

    let mut ev = FixtureBackend::new(&spec);
    let res = ev.run_root("App.Main", "run");
    assert!(
        matches!(res, Val::String(ref s) if s == "int:42 / bool:true"),
        "got {res:?}"
    );
}

/// AC 2(d): GADT constructors holding dictionaries (`Showable`, `Witness`)
/// store existential payloads boxed alongside trailing dictionaries and execute
/// identically to JS and Go.
#[test]
fn ac2d_gadt_constructors_holding_dictionaries() {
    let source = r#"
module App.Main exposing (run)

type Showable =
  | Show b => Showable b : Showable

type Witness a =
  | Show a => Witness : Witness a

from_witness : Witness a -> a -> String
let from_witness witness value =
  match witness with
  | Witness -> show value

describe : Showable -> String
let describe s =
  match s with
  | Showable x -> show x

run : Unit -> String
let run () =
  describe (Showable 1) ++ "," ++ describe (Showable true) ++ "," ++ from_witness Witness (42 : Int)
"#;

    let spec = compile_and_specialize(
        source,
        &[("App.Main", "run")],
        &PrimitiveTable::target_64(),
        &SpecializeConfig::default(),
    )
    .expect("GADT dictionary program must specialize");

    assert!(spec.stats.boxed_cells > 0);
    assert!(spec.stats.runtime_dict_calls > 0);

    let mut ev = FixtureBackend::new(&spec);
    let res = ev.run_root("App.Main", "run");
    assert!(
        matches!(res, Val::String(ref s) if s == "1,true,42"),
        "got {res:?}"
    );
}

/// AC 2(e): Open-row record functions read fields by concrete layout offset
/// with ZERO boxed cells when called on concrete closed rows, and fall back to
/// by-label lookups on boxed records when crossing a higher-rank open-row
/// boundary.
#[test]
fn ac2e_open_row_records_specialize_by_offset_and_fallback_by_label() {
    let concrete_src = r#"
module App.Main exposing (run)

get_x : { x : Int | r } -> Int
let get_x row = row.x

run : Unit -> Int
let run () =
  get_x { x = 10; y = true } + get_x { a = "hi"; x = 32 }
"#;

    let spec_concrete = compile_and_specialize(
        concrete_src,
        &[("App.Main", "run")],
        &PrimitiveTable::target_64(),
        &SpecializeConfig::default(),
    )
    .expect("concrete open-row specialization must succeed");

    assert_eq!(spec_concrete.stats.boxed_cells, 0);
    assert!(spec_concrete.stats.offset_field_reads >= 2);
    assert_eq!(spec_concrete.stats.label_field_reads, 0);

    let mut ev_c = FixtureBackend::new(&spec_concrete);
    assert!(matches!(ev_c.run_root("App.Main", "run"), Val::Int(42)));
    assert!(ev_c.stats.offset_reads >= 2);
    assert_eq!(ev_c.stats.label_reads, 0);
    assert_eq!(ev_c.stats.runtime_boxes, 0);

    let fallback_src = r#"
module App.Main exposing (run)

get_x : { x : Int | r } -> Int
let get_x row = row.x

apply_rec : (forall r. { x : Int | r } -> Int) -> Int
let apply_rec f = f { x = 10; y = true } + f { a = "hi"; x = 32 }

run : Unit -> Int
let run () = apply_rec get_x
"#;

    let spec_fb = compile_and_specialize(
        fallback_src,
        &[("App.Main", "run")],
        &PrimitiveTable::target_64(),
        &SpecializeConfig::default(),
    )
    .expect("open-row fallback must succeed");

    assert!(spec_fb.stats.label_field_reads > 0);
    assert!(spec_fb.stats.boxed_cells > 0);

    let mut ev_fb = FixtureBackend::new(&spec_fb);
    assert!(matches!(ev_fb.run_root("App.Main", "run"), Val::Int(42)));
    assert!(ev_fb.stats.label_reads > 0);
}

/// AC 2(f): Polymorphic `foreign` bindings compile once taking/returning
/// uniform boxed cells and box/unbox at specialized call sites.
#[test]
fn ac2f_polymorphic_foreign_boxes_at_boundary() {
    let source = r#"
module App.Main exposing (run)

poly_echo : a -> a
foreign pure poly_echo from global "poly_echo"

run : Unit -> (Int, String)
let run () = (poly_echo 42, poly_echo "ok")
"#;

    let spec = compile_and_specialize(
        source,
        &[("App.Main", "run")],
        &PrimitiveTable::target_64(),
        &SpecializeConfig::default(),
    )
    .expect("polymorphic foreign program must specialize");

    assert_eq!(spec.stats.polymorphic_foreigns, 1);
    assert_eq!(spec.stats.box_count, 2);
    assert_eq!(spec.stats.unbox_count, 2);

    let mut ev = FixtureBackend::with_foreign(&spec, |_, name, poly, args| {
        if name == "poly_echo" {
            assert!(poly, "poly_echo must be marked polymorphic");
            assert!(
                matches!(args[0], Val::Boxed(_)),
                "polymorphic foreign must receive a boxed cell"
            );
            Some(args[0].clone())
        } else {
            None
        }
    });
    match ev.run_root("App.Main", "run") {
        Val::Tuple(items) => {
            assert!(matches!(items[0], Val::Int(42)));
            assert!(matches!(items[1], Val::String(ref s) if s == "ok"));
        }
        other => panic!("expected (42, \"ok\"), got {other:?}"),
    }
    assert_eq!(ev.stats.runtime_boxes, 2);
    assert_eq!(ev.stats.runtime_unboxes, 2);
}

/// AC 3: Two distinct locally supplied dictionaries at the same concrete type
/// `Int` produce distinct results, AND the `unpack_dictionary.fl` regression
/// preserves packed-module dictionary precedence over global `Show Int`
/// (including through `shown` and `Show (Option Int)`).
#[test]
fn ac3_local_given_and_unpack_dictionary_precedence() {
    // Part 1: exact `tests/run/codegen/unpack_dictionary.fl` logic!
    let unpack_src = r##"
module App.Main exposing (run)

type module Shown =
  type Item = Int
  class Show = Prelude.Show
  showing : Show.Dict Item
  instance Show Item from showing

module Hashed exposing (Item, Show(..), showing) =
  import Prelude exposing (Show(..))

  type alias Item = Int

  showing : Show.Dict Item
  let showing = Show.dictionary { show = fun n -> "#" ++ show n }

module Starred exposing (Item, Show(..), showing) =
  import Prelude exposing (Show(..))

  type alias Item = Int

  showing : Show.Dict Item
  let showing = Show.dictionary { show = fun n -> "*" ++ show n }

packed_hash : Module Shown
let packed_hash = pack Hashed as Shown

packed_star : Module Shown
let packed_star = pack Starred as Shown

shown : Show a => a -> String
let shown x = "<" ++ show x ++ ">"

run : Unit -> String
let run () =
  let hashed =
    unpack packed_hash as K in show 5 ++ " " ++ shown 6 ++ " " ++ show (Some 5)
  let starred =
    unpack packed_star as S in show 5 ++ " " ++ shown 6 ++ " " ++ show (Some 5)
  let global = show 5 ++ " " ++ shown 6 ++ " " ++ show (Some 5)
  hashed ++ " | " ++ starred ++ " | " ++ global
"##;

    let spec = compile_and_specialize(
        unpack_src,
        &[("App.Main", "run")],
        &PrimitiveTable::target_64(),
        &SpecializeConfig::default(),
    )
    .expect("unpack_dictionary program must specialize");

    let mut ev = FixtureBackend::new(&spec);
    let res = ev.run_root("App.Main", "run");
    assert!(
        matches!(
            res,
            Val::String(ref s)
                if s == "#5 <#6> Some #5 | *5 <*6> Some *5 | 5 <6> Some 5"
        ),
        "unexpected dictionary precedence output: {res:?}"
    );
}

/// AC 4 & AC 5: Pathological non-recursive exponential instantiation hits the
/// instantiation budget naming the offending definition; repeated runs produce
/// byte-for-byte identical outputs across 32-bit and 64-bit primitive tables.
#[test]
fn ac4_budget_diagnostic_and_ac5_determinism() {
    let blowup_src = r"
module App.Main exposing (run)

s5 : a -> Int
let s5 _ = 1

s4 : a -> Int
let s4 x = s5 (x, x) + s5 (Some x)

s3 : a -> Int
let s3 x = s4 (x, x) + s4 (Some x)

s2 : a -> Int
let s2 x = s3 (x, x) + s3 (Some x)

s1 : a -> Int
let s1 x = s2 (x, x) + s2 (Some x)

s0 : a -> Int
let s0 x = s1 (x, x) + s1 (Some x)

run : Unit -> Int
let run () = s0 1
";

    let prog = compile_flick(blowup_src);
    let err = specialize(
        &prog,
        &[("App.Main".into(), "run".into())],
        &PrimitiveTable::target_64(),
        &SpecializeConfig {
            max_instances: 16,
            ..SpecializeConfig::default()
        },
    )
    .expect_err("exponential instantiation must hit max_instances");

    match err {
        SpecializeError::BudgetExceeded {
            def,
            kind: BudgetKind::MaxInstances,
            limit: 16,
            count,
        } => {
            assert!(count > 16);
            assert_eq!(def.0, "App.Main");
            assert!(
                def.1.starts_with('s'),
                "expected offending def s*, got {def:?}"
            );
        }
        other => panic!("expected BudgetExceeded(MaxInstances), got {other:?}"),
    }

    // AC 5: Repeated runs with sufficient budget produce identical outputs.
    for table in [PrimitiveTable::target_32(), PrimitiveTable::target_64()] {
        let cfg = SpecializeConfig {
            max_instances: 256,
            ..SpecializeConfig::default()
        };
        let first = specialize(&prog, &[("App.Main".into(), "run".into())], &table, &cfg)
            .expect("sufficient budget succeeds");
        for _ in 0..4 {
            let again = specialize(&prog, &[("App.Main".into(), "run".into())], &table, &cfg)
                .expect("repeated run succeeds");
            assert_eq!(first, again);
        }
        let mut ev = FixtureBackend::new(&first);
        assert!(matches!(ev.run_root("App.Main", "run"), Val::Int(32)));
    }
}
