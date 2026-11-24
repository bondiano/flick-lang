//! End-to-end Go tests for closed record Go structs: typed 0-alloc path,
//! erased boundary <=1-alloc path matching a hand-written struct in `any`,
//! direct field access, open-row/runtime `RecordView` interop, source-order
//! evaluation, constructors over records, and Go FFI round-trips.

use std::fs;
use std::process::Command;

use super::CONFIG;
use super::common::{flick_in, project};

const ALLOC_FLICK: &str = r#"module App exposing (main, make_record, read_x, bump_x)

type alias Point = { x : Float; y : Float }

make_record : Float -> Float -> Point
let make_record x y = { x = x; y = y }

read_x : Point -> Float
let read_x p = p.x

bump_x : Point -> Point
let bump_x p = { p with x = p.x + 1.0 }

keep : a -> Int
foreign pure keep from inline "func(_ *flickrt.Fiber, _ flickrt.Value) flickrt.Value { return int32(0) }"

main : Unit ->{Host.Console} Unit
let main () =
  let _ = keep (make_record, read_x, bump_x)
  let p = bump_x (make_record 1.25 2.25)
  Platform.stdout_line (show (read_x p))
"#;

const ALLOC_GO_TEST: &str = r#"package main

import (
	"runtime"
	"testing"

	"flick.app/flickrt"
)

type handPoint struct {
	x float64
	y float64
}

var (
	typedSink  = F_App__make_record(nil, 0, 0)
	floatSink  float64
	erasedSink flickrt.Value
	handSink   any
)

func makeHandPoint(x, y float64) handPoint {
	return handPoint{x: x, y: y}
}

func TestCompiledClosedRecordAllocs(t *testing.T) {
	// Typed path: construct `{ x : Float; y : Float }`, return from a known call,
	// store by value in a typed global -> 0 allocs/op.
	x := 1.25
	typedAllocs := testing.AllocsPerRun(1000, func() {
		y := x + 1.0
		typedSink = F_App__make_record(nil, x, y)
		x += 0.5
	})
	if typedAllocs != 0 {
		t.Fatalf("typed make_record allocs/op = %v, want 0", typedAllocs)
	}
	if F_App__read_x(nil, typedSink) != x-0.5 {
		t.Fatalf("typedSink.x = %v, want %v", F_App__read_x(nil, typedSink), x-0.5)
	}

	// Typed update + direct field read -> 0 allocs/op.
	x = 1.25
	pipelineAllocs := testing.AllocsPerRun(1000, func() {
		y := x + 1.0
		floatSink = F_App__read_x(nil, F_App__bump_x(nil, F_App__make_record(nil, x, y)))
		x += 0.5
	})
	if pipelineAllocs != 0 {
		t.Fatalf("typed make_record+bump_x+read_x allocs/op = %v, want 0", pipelineAllocs)
	}
	if floatSink != (x-0.5)+1.0 {
		t.Fatalf("floatSink = %v, want %v", floatSink, (x-0.5)+1.0)
	}

	// Hand-written struct stored in global `any` (reference baseline).
	x = 1.25
	handAllocs := testing.AllocsPerRun(1000, func() {
		y := x + 1.0
		handSink = makeHandPoint(x, y)
		x += 0.5
	})
	if handAllocs > 1 {
		t.Fatalf("hand-written struct in any allocs/op = %v, want <= 1", handAllocs)
	}

	// Erased boundary: compiled record stored in global `flickrt.Value` -> <= 1 alloc/op,
	// matching the hand-written struct stored in global `any`.
	x = 1.25
	erasedAllocs := testing.AllocsPerRun(1000, func() {
		y := x + 1.0
		erasedSink = F_App__make_record(nil, x, y)
		x += 0.5
	})
	if erasedAllocs > 1 {
		t.Fatalf("erased make_record in flickrt.Value allocs/op = %v, want <= 1", erasedAllocs)
	}
	if erasedAllocs != handAllocs {
		t.Fatalf("erased allocs/op = %v != hand-written allocs/op = %v", erasedAllocs, handAllocs)
	}
	if flickrt.Field(erasedSink, "x") != x-0.5 || flickrt.Field(erasedSink, "y") != (x-0.5)+1.0 {
		t.Fatalf("erasedSink fields mismatch: x=%v y=%v", flickrt.Field(erasedSink, "x"), flickrt.Field(erasedSink, "y"))
	}

	// Capture B/op via testing.Benchmark for comparable nonconstant runs.
	bx := 1.25
	typedRes := testing.Benchmark(func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			y := bx + 1.0
			typedSink = F_App__make_record(nil, bx, y)
			bx += 0.5
		}
	})
	ex := 1.25
	erasedRes := testing.Benchmark(func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			y := ex + 1.0
			erasedSink = F_App__make_record(nil, ex, y)
			ex += 0.5
		}
	})
	hx := 1.25
	handRes := testing.Benchmark(func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			y := hx + 1.0
			handSink = makeHandPoint(hx, y)
			hx += 0.5
		}
	})
	t.Logf(
		"env=%s %s/%s typed=(%v allocs/op, %d B/op) pipeline=(%v allocs/op) erased=(%v allocs/op, %d B/op) hand=(%v allocs/op, %d B/op)",
		runtime.Version(), runtime.GOOS, runtime.GOARCH,
		typedAllocs, typedRes.AllocedBytesPerOp(),
		pipelineAllocs,
		erasedAllocs, erasedRes.AllocedBytesPerOp(),
		handAllocs, handRes.AllocedBytesPerOp(),
	)
}
"#;

const SEMANTICS_FLICK: &str = r##"module App exposing (main)

opaque type alias UserId = Int

type alias Point = { x : Float; y : Float }
  deriving Schema

type Shape = Dot | Box { w : Int; h : Int; name : String }

type Wrap a = Wrap { value : a; count : Int }

go_roundtrip : a -> a
foreign pure go_roundtrip from "./rec_ffi.mjs" "goRoundtrip"

go_make_point : Float -> Float -> Point
foreign pure go_make_point from "./rec_ffi.mjs" "goMakePoint"

go_read_view : a -> String
foreign pure go_read_view from "./rec_ffi.mjs" "goReadView"

open_x : { x : Float | r } -> Float
let open_x r = r.x

open_bump_x : { x : Float | r } -> { x : Float | r }
let open_bump_x r = { r with x = r.x + 10.0 }

eval_order_literal : Unit -> (Point, String)
let eval_order_literal () =
  let mut log = ""
  let p : Point = {
    y =
      log = log ++ "y"
      2.5
    x =
      log = log ++ "x"
      1.5
  }
  (p, log)

eval_order_update : Unit -> (Point, String)
let eval_order_update () =
  let mut log = ""
  let p = {
    (match () with
    | () ->
      log = log ++ "b"
      { x = 1.0; y = 2.0 }) with
    y =
      log = log ++ "y"
      9.0
    x =
      log = log ++ "x"
      8.0
  }
  (p, log)

read_box : Shape -> String
let read_box s =
  match s with
  | Dot -> "dot"
  | Box { w; h; name } -> name ++ ":" ++ show (w * h)

grow_box : Shape -> Shape
let grow_box s =
  match s with
  | Dot -> Dot
  | Box b -> Box { b with h = b.h + 1 }

read_wrap : Wrap String -> String
let read_wrap w =
  match w with
  | Wrap { value; count } -> value ++ "#" ++ show count

read_uid_rec : { id : UserId } -> Int
let read_uid_rec r = r.id

read_field : Record.Field "x" Int -> Int
let read_field r = r.x

main : Unit ->{Host.Console} Unit
let main () =
  let lit : Point = { x = 1.5; y = 2.5 }
  let rev : Point = { y = 2.5; x = 1.5 }
  let upd : Point = { lit with x = 3.5 }
  let rt_rec : Point = go_make_point 1.5 2.5
  let codec = Codec.json Point.schema
  let encoded =
    match Codec.encode_result codec lit with
    | Ok j -> Json.stringify j
    | Err _ -> "err"
  let decoded : Point =
    match Json.parse encoded with
    | Ok j ->
      match Codec.decode_result codec j with
      | Ok p -> p
      | Err _ -> { x = -1.0; y = -1.0 }
    | Err _ -> { x = -2.0; y = -2.0 }
  let arb_samples =
    try
      Arbitrary.sample
        { count = Some 3; size = None; maxDiscards = None; seed = Some 42.0 }
        (Arbitrary.from_schema Point.schema)
    with
    | Arbitrary.SampleError _ -> []
  let arb_ok = List.length arb_samples == 3 && List.all (fun p -> open_x p == p.x) arb_samples
  let (lit_p, lit_log) = eval_order_literal ()
  let (upd_p, upd_log) = eval_order_update ()
  let b1 = Box { name = "r"; h = 3; w = 4 }
  let b2 = grow_box b1
  let w1 = Wrap { value = "hi"; count = 2 }
  let opt_p : Option Point = Some lit
  let opt_x =
    match opt_p with
    | Some p -> p.x
    | None -> 0.0
  let uid_r : { id : UserId } = { id = 99 }
  let fld : Record.Field "x" Int = { x = 77 }
  let empty = {}
  let lines = [
    "open:" ++ show (open_x lit) ++ "|" ++ show (open_x upd) ++ "|" ++ show (open_x rt_rec) ++ "|" ++ show (open_x decoded) ++ "|" ++ show (open_x (open_bump_x lit)),
    "eq:" ++ show (lit == rev) ++ "|" ++ show (lit == rt_rec) ++ "|" ++ show (lit == decoded) ++ "|" ++ show (lit == upd),
    "show:" ++ show lit ++ "|" ++ show rt_rec ++ "|" ++ show decoded,
    "ord:" ++ show (compare lit upd) ++ "|" ++ show (compare rt_rec lit),
    "hash:" ++ show (hash lit == hash rev) ++ "|" ++ show (hash lit == hash rt_rec) ++ "|" ++ show (hash lit == hash decoded),
    "codec:" ++ encoded ++ "|" ++ show arb_ok,
    "order:" ++ lit_log ++ ":" ++ show lit_p ++ "|" ++ upd_log ++ ":" ++ show upd_p,
    "ctor:" ++ read_box b1 ++ "|" ++ read_box b2 ++ "|" ++ read_wrap w1,
    "ffi:" ++ go_read_view lit ++ "|" ++ show (go_roundtrip lit == lit),
    "misc:" ++ show opt_x ++ "|" ++ show (read_uid_rec uid_r) ++ "|" ++ show (read_field fld) ++ "|" ++ show empty,
  ]
  Platform.stdout_line (String.join "\n" lines)
"##;

const REC_FFI_GO: &str = r#"package main

import (
	"fmt"

	"flick.app/flickrt"
)

func goRoundtrip(_ *flickrt.Fiber, v flickrt.Value) flickrt.Value {
	return v
}

func goMakePoint(_ *flickrt.Fiber, x, y flickrt.Value) flickrt.Value {
	return flickrt.Rec("x", x, "y", y)
}

func goReadView(_ *flickrt.Fiber, v flickrt.Value) flickrt.Value {
	rv := flickrt.AsRecord(v)
	return fmt.Sprintf("len=%d,keys=%v,x=%v,y=%v", rv.Len(), rv.Keys(), rv.Get("x"), rv.Get("y"))
}
"#;

#[test]
#[ignore = "requires Go"]
fn compiled_closed_record_allocations_meet_targets_in_debug_and_release() {
    let config = format!("{CONFIG}[tools]\n\"flick/go\" = [\"go\"]\n");
    for profile in ["debug", "release"] {
        let root = project(
            &format!("go_closed_record_allocs_{profile}"),
            &[("flick.toml", &config), ("src/App.fl", ALLOC_FLICK)],
        );
        let build = flick_in(
            &root,
            &["build", "--target", "go", "--profile", profile, "--offline"],
        );
        assert!(build.status.success(), "{profile}: {build:?}");
        let go_dir = root.join("dist/go/main");
        let app_go = fs::read_to_string(go_dir.join("m_App.go")).expect("read m_App.go");
        assert!(
            app_go.contains("return l_p.f0"),
            "{profile}: expected direct field access l_p.f0:\n{app_go}"
        );
        assert!(
            !app_go.contains("flickrt.Field(") && !app_go.contains("flickrt.Shaped("),
            "{profile}: unexpected boxed record lookup/shape:\n{app_go}"
        );
        fs::write(go_dir.join("record_alloc_test.go"), ALLOC_GO_TEST).expect("write go test");
        let test = Command::new("go")
            .args(["test", "-v", "-run", "TestCompiledClosedRecordAllocs", "."])
            .current_dir(&go_dir)
            .output()
            .expect("go test");
        let stdout = String::from_utf8_lossy(&test.stdout);
        let stderr = String::from_utf8_lossy(&test.stderr);
        assert!(
            test.status.success(),
            "{profile} go test failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        eprintln!("[{profile}]\n{stdout}");
    }
}

#[test]
#[ignore = "requires Go"]
fn closed_record_open_row_codecs_eval_order_ctors_and_ffi_run_cleanly() {
    let config = format!("{CONFIG}[tools]\n\"flick/go\" = [\"go\"]\n");
    let root = project(
        "go_closed_record_semantics",
        &[
            ("flick.toml", &config),
            ("src/App.fl", SEMANTICS_FLICK),
            ("src/rec_ffi.go", REC_FFI_GO),
        ],
    );
    for profile in ["debug", "release"] {
        let run = flick_in(
            &root,
            &["run", "--target", "go", "--profile", profile, "--offline"],
        );
        assert!(run.status.success(), "{profile}: {run:?}");
        let expected = concat!(
            "open:1.5|3.5|1.5|1.5|11.5\n",
            "eq:true|true|true|false\n",
            "show:{ x = 1.5, y = 2.5 }|{ x = 1.5, y = 2.5 }|{ x = 1.5, y = 2.5 }\n",
            "ord:LT|EQ\n",
            "hash:true|true|true\n",
            "codec:{\"x\":1.5,\"y\":2.5}|true\n",
            "order:yx:{ x = 1.5, y = 2.5 }|byx:{ x = 8, y = 9 }\n",
            "ctor:r:12|r:16|hi#2\n",
            "ffi:len=2,keys=[x y],x=1.5,y=2.5|true\n",
            "misc:1.5|99|77|{  }\n",
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            expected,
            "profile={profile}"
        );
    }
}
