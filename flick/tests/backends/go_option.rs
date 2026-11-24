//! End-to-end Go tests for allocation-free concrete `Option` representations,
//! separate erased boxing measurements, erased polymorphic observation, and
//! `flickrt.Value` Go FFI round-trips.

use std::fs;
use std::process::Command;

use super::CONFIG;
use super::common::{flick_in, project};

const ALLOC_FLICK: &str = r#"module App exposing (main)

type Mark = Mark Int

opt_int_step : Bool -> Int -> Int
let opt_int_step ok n =
  let o : Option Int = if ok then Some n else None
  match o with
  | Some v -> v + 1
  | None -> 0

opt_bool_step : Bool -> Bool -> Int
let opt_bool_step ok b =
  let o : Option Bool = if ok then Some b else None
  match o with
  | Some true -> 2
  | Some false -> 1
  | None -> 0

opt_ref_step : Bool -> Mark -> Int
let opt_ref_step ok m =
  let o : Option Mark = if ok then Some m else None
  match o with
  | Some (Mark n) -> n + 1
  | None -> 0

opt_nested_step : Int -> Int -> Int
let opt_nested_step tag n =
  let o : Option (Option Int) =
    if tag == 0 then None
    else if tag == 1 then Some None
    else Some (Some n)
  match o with
  | Some (Some v) -> v + 1
  | Some None -> -1
  | None -> -2

make_opt_int : Int -> Option Int
let make_opt_int n = Some n

make_opt_ref : Mark -> Option Mark
let make_opt_ref m = Some m

make_mark : Int -> Mark
let make_mark n = Mark n

main : Unit ->{Host.Console} Unit
let main () =
  let m = make_mark 7
  let _ = (opt_int_step true 1, opt_bool_step true false, opt_ref_step true m, opt_nested_step 2 5)
  let _ = (make_opt_int 3, make_opt_ref m)
  Platform.stdout_line "ok"
"#;

const ALLOC_GO_TEST: &str = r#"package main

import (
	"testing"

	"flick.app/flickrt"
)

var (
	benchBool  = true
	benchFlag  = false
	benchInt   = int32(41)
	benchTag   = int32(2)
	benchMark  = F_App__make_mark(nil, 41)
	benchSink  int32
	benchValue flickrt.Value
)

func TestCompiledTypedOptionZeroAllocs(t *testing.T) {
	if allocs := testing.AllocsPerRun(1000, func() {
		benchSink = F_App__opt_int_step(nil, benchBool, benchInt)
	}); allocs != 0 || benchSink != 42 {
		t.Fatalf("compiled Option Int (flag): got allocs=%v sink=%v, want 0 and 42", allocs, benchSink)
	}
	if allocs := testing.AllocsPerRun(1000, func() {
		benchSink = F_App__opt_bool_step(nil, benchBool, benchFlag)
	}); allocs != 0 || benchSink != 1 {
		t.Fatalf("compiled Option Bool (sentinel): got allocs=%v sink=%v, want 0 and 1", allocs, benchSink)
	}
	if allocs := testing.AllocsPerRun(1000, func() {
		benchSink = F_App__opt_ref_step(nil, benchBool, benchMark)
	}); allocs != 0 || benchSink != 42 {
		t.Fatalf("compiled Option Mark (ref sentinel): got allocs=%v sink=%v, want 0 and 42", allocs, benchSink)
	}
	if allocs := testing.AllocsPerRun(1000, func() {
		benchSink = F_App__opt_nested_step(nil, benchTag, benchInt)
	}); allocs != 0 || benchSink != 42 {
		t.Fatalf("compiled Option (Option Int) (nested flag): got allocs=%v sink=%v, want 0 and 42", allocs, benchSink)
	}
	// Measure conversion to erased flickrt.Value separately.
	boxIntAllocs := testing.AllocsPerRun(1000, func() {
		benchValue = flickrt.BoxOptInt(F_App__make_opt_int(nil, benchInt))
	})
	if !flickrt.IsSome(benchValue) || flickrt.AsInt(flickrt.UnwrapSome(benchValue)) != 41 {
		t.Fatalf("BoxOptInt round-trip mismatch: %#v", benchValue)
	}
	boxRefAllocs := testing.AllocsPerRun(1000, func() {
		benchValue = flickrt.BoxOptRef(F_App__make_opt_ref(nil, benchMark))
	})
	if !flickrt.IsSome(benchValue) || !flickrt.Same(flickrt.UnwrapSome(benchValue), benchMark) {
		t.Fatalf("BoxOptRef round-trip mismatch: %#v", benchValue)
	}
	t.Logf("erased conversion allocs/op: BoxOptInt=%v BoxOptRef=%v", boxIntAllocs, boxRefAllocs)
}
"#;

const OBSERVER_FLICK: &str = r#"module App exposing (main)

observe : a -> String
foreign pure observe from "./obs.mjs" "observe"

roundtrip : a -> a
foreign pure roundtrip from "./obs.mjs" "roundtrip"

poly_observe : a -> String
let poly_observe x = observe x

poly_id : a -> a
let poly_id x = x

describe2 : Option (Option Int) -> String
let describe2 o =
  match o with
  | None -> "none"
  | Some None -> "some(none)"
  | Some (Some n) -> "some(some(" ++ show n ++ "))"

main : Unit ->{Host.Console} Unit
let main () =
  let raw : Int = 1
  let one : Option Int = Some 1
  let two : Option (Option Int) = Some (Some 1)
  let s_none : Option (Option Int) = Some None
  let s_s_none : Option (Option (Option Int)) = Some (Some None)
  let rt_raw = roundtrip raw
  let rt_one = roundtrip one
  let rt_two = roundtrip two
  let rt_s_none = roundtrip s_none
  let rt_bool = roundtrip (Some false)
  let lines = [
    "direct:" ++ observe raw ++ "|" ++ observe one ++ "|" ++ observe two,
    "poly:" ++ poly_observe raw ++ "|" ++ poly_observe one ++ "|" ++ poly_observe two,
    "nesting:" ++ observe (None : Option Int) ++ "|" ++ observe s_none ++ "|" ++ observe s_s_none,
    "rt:" ++ show rt_raw ++ "|" ++ show rt_one ++ "|" ++ describe2 rt_two ++ "|" ++ describe2 rt_s_none ++ "|" ++ show rt_bool,
    "poly_id:" ++ describe2 (poly_id two) ++ "|" ++ describe2 (poly_id s_none),
  ]
  Platform.stdout_line (String.join "\n" lines)
"#;

const OBSERVER_GO: &str = r#"package main

import (
	"fmt"

	"flick.app/flickrt"
)

func describeErased(v flickrt.Value) string {
	if flickrt.IsNone(v) {
		tag := flickrt.TagOf(v).(string)
		fields := v.(flickrt.Constructed).Fields()
		return fmt.Sprintf("None[tag=%s,ctor=%s,fields=%d]", tag, flickrt.ConstructorName(tag), len(fields))
	}
	if flickrt.IsSome(v) {
		tag := flickrt.TagOf(v).(string)
		fields := v.(flickrt.Constructed).Fields()
		return fmt.Sprintf("Some[tag=%s,ctor=%s,fields=%d](%s)", tag, flickrt.ConstructorName(tag), len(fields), describeErased(flickrt.UnwrapSome(v)))
	}
	return fmt.Sprintf("Raw(%T:%v)", v, v)
}

func observe(_ *flickrt.Fiber, v flickrt.Value) flickrt.Value {
	return describeErased(v)
}

func roundtrip(_ *flickrt.Fiber, v flickrt.Value) flickrt.Value {
	return v
}
"#;

#[test]
#[ignore = "requires Go"]
fn compiled_go_typed_option_zero_allocs_and_separate_boxing() {
    let config = format!("{CONFIG}[tools]\n\"flick/go\" = [\"go\"]\n");
    let root = project(
        "go_typed_option_allocs",
        &[("flick.toml", &config), ("src/App.fl", ALLOC_FLICK)],
    );
    let build = flick_in(&root, &["build", "--target", "go", "--offline"]);
    assert!(build.status.success(), "{build:?}");
    let go_dir = root.join("dist/go/main");
    fs::write(go_dir.join("option_alloc_test.go"), ALLOC_GO_TEST).expect("write go test");
    let test = Command::new("go")
        .args([
            "test",
            "-v",
            "-run",
            "TestCompiledTypedOptionZeroAllocs",
            ".",
        ])
        .current_dir(&go_dir)
        .output()
        .expect("go test");
    assert!(
        test.status.success(),
        "go test failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&test.stdout),
        String::from_utf8_lossy(&test.stderr)
    );
}

#[test]
#[ignore = "requires Go"]
fn erased_observer_and_ffi_roundtrip_distinguish_option_nesting() {
    let config = format!("{CONFIG}[tools]\n\"flick/go\" = [\"go\"]\n");
    let root = project(
        "go_erased_option_observer",
        &[
            ("flick.toml", &config),
            ("src/App.fl", OBSERVER_FLICK),
            ("src/obs.go", OBSERVER_GO),
        ],
    );
    let run = flick_in(&root, &["run", "--target", "go", "--offline"]);
    assert!(run.status.success(), "{run:?}");
    let expected = concat!(
        "direct:Raw(int32:1)|Some[tag=Some,ctor=Some,fields=1](Raw(int32:1))|Some[tag=Some,ctor=Some,fields=1](Some[tag=Some,ctor=Some,fields=1](Raw(int32:1)))\n",
        "poly:Raw(int32:1)|Some[tag=Some,ctor=Some,fields=1](Raw(int32:1))|Some[tag=Some,ctor=Some,fields=1](Some[tag=Some,ctor=Some,fields=1](Raw(int32:1)))\n",
        "nesting:None[tag=None,ctor=None,fields=0]|Some[tag=Some,ctor=Some,fields=1](None[tag=None,ctor=None,fields=0])|Some[tag=Some,ctor=Some,fields=1](Some[tag=Some,ctor=Some,fields=1](None[tag=None,ctor=None,fields=0]))\n",
        "rt:1|Some 1|some(some(1))|some(none)|Some false\n",
        "poly_id:some(some(1))|some(none)\n",
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
}

const EDGE_CASES_FLICK: &str = r#"module App exposing (main)

opaque type alias UserId = String
opaque type alias Id a = a
opaque type alias MaybeInt = Option Int
opaque type alias Stream = Option (Float, Stream)

type Node = Wrap (Option Node)

nil_unknown : Unit -> Unknown
foreign pure nil_unknown from "./nil_unk.mjs" "nilUnknown"

read_uid : Option UserId -> String
let read_uid o =
  match o with
  | Some s -> s
  | None -> "none"

read_str : Option String -> String
let read_str o =
  match o with
  | Some s -> s
  | None -> "none"

read_id : Option (Id Int) -> Int
let read_id o =
  match o with
  | Some n -> n
  | None -> 0

read_maybe : Option MaybeInt -> Int
let read_maybe o =
  match o with
  | Some (Some n) -> n
  | Some None -> -1
  | None -> -2

read_stream : Option Stream -> Float
let read_stream o =
  match o with
  | Some (Some (x, _)) -> x
  | _ -> 0.0

read_unknown : Option Unknown -> String
let read_unknown o =
  match o with
  | Some _ -> "some"
  | None -> "none"

nested_or : Option Node -> Option Node
let nested_or o =
  let (Some (Wrap x) | (x | x)) = o
  x

describe_node : Option Node -> String
let describe_node o =
  match o with
  | None -> "none"
  | Some (Wrap None) -> "wrap(none)"
  | Some (Wrap (Some _)) -> "wrap(some)"

read_field : Option (Record.Field "x" Int) -> Int
let read_field o =
  match o with
  | Some r -> r.x
  | None -> 0

read_rec : Option { x : Int } -> Int
let read_rec o =
  match o with
  | Some r -> r.x
  | None -> 0

make_field : Int -> Option (Record.Field "x" Int)
let make_field n = Some { x = n }

read_field_poly : Record.HasField l (Record.Field l a) a => Record.Label l -> Option (Record.Field l a) -> a -> a
let read_field_poly lbl o fallback =
  match o with
  | Some r -> Record.get lbl r
  | None -> fallback

main : Unit ->{Host.Console} Unit
let main () =
  let s : Option String = Some "alice"
  let uid : Option UserId = Some "bob"
  let oi : Option Int = Some 11
  let mi : Option (Option Int) = Some (Some 42)
  let st : Option Stream = Some (Some (2.5, None))
  let nil_u : Option Unknown = Some (nil_unknown ())
  let rec_opt : Option { x : Int } = Some { x = 10 }
  let fld_opt : Option (Record.Field "x" Int) = Some { x = 20 }
  let from_fn : Option { x : Int } = make_field 30
  let lbl = (Record.label : Record.Label "x")
  let lines = [
    "uid:" ++ read_uid s ++ "|" ++ read_str uid,
    "id:" ++ show (read_id oi) ++ "|maybe:" ++ show (read_maybe mi) ++ "|stream:" ++ show (read_stream st),
    "unk_nil:" ++ read_unknown nil_u ++ "|" ++ read_unknown None,
    "nested_or:" ++ describe_node (nested_or None) ++ "|" ++ describe_node (nested_or (Some (Wrap None))) ++ "|" ++ describe_node (nested_or (Some (Wrap (Some (Wrap None))))),
    "field:" ++ show (read_field rec_opt) ++ "|" ++ show (read_rec fld_opt) ++ "|" ++ show (read_rec from_fn) ++ "|" ++ show (read_field_poly lbl rec_opt 0) ++ "|" ++ show (read_field_poly lbl fld_opt 0) ++ "|" ++ show (read_field None),
  ]
  Platform.stdout_line (String.join "\n" lines)
"#;

const NIL_UNK_GO: &str = r#"package main

import "flick.app/flickrt"

func nilUnknown(_ *flickrt.Fiber, _ flickrt.Value) flickrt.Value {
	return nil
}
"#;

#[test]
#[ignore = "requires Go"]
fn opaque_alias_nil_unknown_and_nested_or_run_cleanly_on_go() {
    let config = format!("{CONFIG}[tools]\n\"flick/go\" = [\"go\"]\n");
    let root = project(
        "go_option_edge_cases",
        &[
            ("flick.toml", &config),
            ("src/App.fl", EDGE_CASES_FLICK),
            ("src/nil_unk.go", NIL_UNK_GO),
        ],
    );
    let run = flick_in(&root, &["run", "--target", "go", "--offline"]);
    assert!(run.status.success(), "{run:?}");
    let expected = concat!(
        "uid:alice|bob\n",
        "id:11|maybe:42|stream:2.5\n",
        "unk_nil:some|none\n",
        "nested_or:none|none|wrap(none)\n",
        "field:10|20|30|10|20|0\n",
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
}
