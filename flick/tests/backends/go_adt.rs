//! End-to-end Go tests for by-value closed ADTs and concrete `Result`:
//! typed 0-alloc path across known cross-module calls and aggregate storage,
//! erased `flickrt.Value` boundary measurements against hand-written Go, and
//! generic runtime / FFI / derived / codec round-trips in debug and release.

use std::fs;
use std::process::Command;

use super::CONFIG;
use super::common::{flick_in, project};

const ALLOC_TYPES_FL: &str = r"module Types exposing (Shape(..), Entry)

type Shape = Dot | Circle Float | Rect Float Float
  deriving Eq, Ord, Hash, Show

type alias Entry = { id : Int; shape : Shape; outcome : Result String Int }
";

const ALLOC_WORKER_FL: &str = r#"module Worker exposing (make_shape, make_result, make_entry, eval_entry, same_shape, same_result)

import Types exposing (Shape(..), Entry)

make_shape : Float -> Float -> Shape
let make_shape w h =
  if w <= 0.0 then Dot
  else if h <= 0.0 then Circle w
  else Rect w h

make_result : Int -> Result String Int
let make_result n =
  if n >= 0 then Ok (n + 1) else Err "neg"

make_entry : Int -> Float -> Float -> Entry
let make_entry id w h =
  { id = id; shape = make_shape w h; outcome = make_result id }

eval_entry : Entry -> Float
let eval_entry e =
  let a =
    match e.shape with
    | Dot -> 0.0
    | Circle r -> r * r
    | Rect w h -> w * h
  let b =
    match e.outcome with
    | Ok v -> Int.to_float v
    | Err _ -> -1.0
  a + b

same_shape : Shape -> Shape -> Bool
let same_shape a b = a == b

same_result : Result String Int -> Result String Int -> Bool
let same_result a b = a == b
"#;

const ALLOC_APP_FL: &str = r#"module App exposing (main)

import Types
import Worker

keep : a -> Int
foreign pure keep from inline "func(_ *flickrt.Fiber, _ flickrt.Value) flickrt.Value { return int32(0) }"

main : Unit ->{Host.Console} Unit
let main () =
  let _ = keep (Worker.make_shape, Worker.make_result, Worker.make_entry, Worker.eval_entry, Worker.same_shape, Worker.same_result)
  let e = Worker.make_entry 3 2.0 4.0
  Platform.stdout_line (show (Worker.eval_entry e))
"#;

const ALLOC_GO_TEST: &str = r#"package main

import (
	"runtime"
	"testing"

	"flick.app/flickrt"
)

type handShape struct {
	f0  float64
	f1  float64
	tag uint8
}

type handResult struct {
	f0  string
	f1  int32
	tag uint8
}

var (
	typedShapeSink  = F_Worker__make_shape(nil, 0, 0)
	typedResultSink = F_Worker__make_result(nil, 0)
	typedEntrySink  = F_Worker__make_entry(nil, 0, 0, 0)
	floatSink       float64
	boolSink        bool
	erasedShapeSink flickrt.Value
	erasedResSink   flickrt.Value
	handShapeSink   any
	handResSink     any
)

func makeHandShape(w, h float64) handShape {
	if w <= 0 {
		return handShape{}
	}
	if h <= 0 {
		return handShape{tag: 1, f0: w}
	}
	return handShape{tag: 2, f0: w, f1: h}
}

func makeHandResult(n int32) handResult {
	if n >= 0 {
		return handResult{tag: 1, f1: n + 1}
	}
	return handResult{tag: 0, f0: "neg"}
}

func TestCompiledClosedAdtAllocs(t *testing.T) {
	// 1. User sum: nonconstant construct + cross-module call + typed global store -> 0 allocs/op.
	w := 2.0
	shapeAllocs := testing.AllocsPerRun(1000, func() {
		h := w + 1.0
		typedShapeSink = F_Worker__make_shape(nil, w, h)
		w += 0.5
	})
	if shapeAllocs != 0 {
		t.Fatalf("typed make_shape allocs/op = %v, want 0", shapeAllocs)
	}
	lastW := w - 0.5
	wantShape := F_Worker__make_shape(nil, lastW, lastW+1.0)
	if !F_Worker__same_shape(nil, typedShapeSink, wantShape) {
		t.Fatalf("typedShapeSink mismatch: got %+v want %+v", typedShapeSink, wantShape)
	}

	// 2. Concrete Result String Int: nonconstant construct + cross-module call + typed global store -> 0 allocs/op.
	n := int32(1)
	resAllocs := testing.AllocsPerRun(1000, func() {
		typedResultSink = F_Worker__make_result(nil, n)
		n = -n + 1
		if n == 0 {
			n = 2
		}
	})
	if resAllocs != 0 {
		t.Fatalf("typed make_result allocs/op = %v, want 0", resAllocs)
	}

	// 3. Aggregate storage + cross-module match & compare -> 0 allocs/op.
	id := int32(1)
	w = 1.5
	aggAllocs := testing.AllocsPerRun(1000, func() {
		e := F_Worker__make_entry(nil, id, w, w+1.0)
		typedEntrySink = e
		floatSink = F_Worker__eval_entry(nil, e)
		boolSink = F_Worker__same_shape(nil, e.f2, e.f2) && F_Worker__same_result(nil, e.f1, e.f1)
		id++
		w += 0.25
	})
	if aggAllocs != 0 {
		t.Fatalf("typed aggregate+match+compare allocs/op = %v, want 0", aggAllocs)
	}
	if !boolSink {
		t.Fatalf("expected self-equality true")
	}
	lastId := id - 1
	lastW = w - 0.25
	wantEval := lastW*(lastW+1.0) + float64(lastId+1)
	if floatSink != wantEval {
		t.Fatalf("floatSink = %v, want %v", floatSink, wantEval)
	}

	// 4. Measure typed, erased flickrt.Value, and hand-written Go in `any` with same destination/lifetime.
	bw := 2.0
	typedShapeRes := testing.Benchmark(func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			typedShapeSink = F_Worker__make_shape(nil, bw, bw+1.0)
			bw += 0.5
		}
	})
	bn := int32(1)
	typedResultRes := testing.Benchmark(func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			typedResultSink = F_Worker__make_result(nil, bn)
			bn++
		}
	})
	ba := int32(1)
	baw := 1.5
	typedAggRes := testing.Benchmark(func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			e := F_Worker__make_entry(nil, ba, baw, baw+1.0)
			typedEntrySink = e
			floatSink = F_Worker__eval_entry(nil, e)
			ba++
			baw += 0.25
		}
	})

	ew := 2.0
	erasedShapeRes := testing.Benchmark(func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			erasedShapeSink = Box_A_Types__Shape(F_Worker__make_shape(nil, ew, ew+1.0))
			ew += 0.5
		}
	})
	en := int32(1)
	erasedResultRes := testing.Benchmark(func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			erasedResSink = Box_AI_Prelude__Result__b1387822a2a71e80(F_Worker__make_result(nil, en))
			en++
		}
	})
	hw := 2.0
	handShapeRes := testing.Benchmark(func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			handShapeSink = makeHandShape(hw, hw+1.0)
			hw += 0.5
		}
	})
	hn := int32(1)
	handResultRes := testing.Benchmark(func(b *testing.B) {
		for i := 0; i < b.N; i++ {
			handResSink = makeHandResult(hn)
			hn++
		}
	})

	// Sanity check erased values round-trip cleanly.
	if !F_Worker__same_shape(nil, As_A_Types__Shape(erasedShapeSink), F_Worker__make_shape(nil, ew-0.5, ew+0.5)) {
		t.Fatalf("erasedShapeSink round-trip mismatch")
	}

	t.Logf(
		"env=%s %s/%s typed_shape=(%d allocs/op, %d B/op) typed_result=(%d allocs/op, %d B/op) typed_agg=(%d allocs/op, %d B/op) erased_shape=(%d allocs/op, %d B/op) erased_result=(%d allocs/op, %d B/op) hand_shape=(%d allocs/op, %d B/op) hand_result=(%d allocs/op, %d B/op)",
		runtime.Version(), runtime.GOOS, runtime.GOARCH,
		typedShapeRes.AllocsPerOp(), typedShapeRes.AllocedBytesPerOp(),
		typedResultRes.AllocsPerOp(), typedResultRes.AllocedBytesPerOp(),
		typedAggRes.AllocsPerOp(), typedAggRes.AllocedBytesPerOp(),
		erasedShapeRes.AllocsPerOp(), erasedShapeRes.AllocedBytesPerOp(),
		erasedResultRes.AllocsPerOp(), erasedResultRes.AllocedBytesPerOp(),
		handShapeRes.AllocsPerOp(), handShapeRes.AllocedBytesPerOp(),
		handResultRes.AllocsPerOp(), handResultRes.AllocedBytesPerOp(),
	)
}
"#;

const SEMANTICS_TYPES_FL: &str = r#"module Types exposing (Color(..), Shape(..), Note, Envelope(..), Custom(..))

type Color = Red | Green | Blue
  deriving Eq, Ord, Hash, Show, Schema

type Shape = Dot | Circle Float | Rect Float Float
  deriving Eq, Ord, Hash, Show, Schema

type alias Note = { color : Color; shape : Shape; outcome : Result String Int }

type Envelope = Tag Color | Nested (Option Int) Shape
  deriving Eq, Ord, Hash, Show

type Custom = Custom Int

instance Eq Custom =
  let eq _ _ = true

instance Show Custom =
  let show _ = "custom-value"
"#;

const SEMANTICS_APP_FL: &str = r#"module App exposing (main)

import Types exposing (Color(..), Shape(..), Note, Envelope(..), Custom(..))

id_poly : a -> a
let id_poly x = x

go_roundtrip : a -> a
foreign pure go_roundtrip from "./adt_ffi.mjs" "goRoundtrip"

go_inspect : a -> String
foreign pure go_inspect from "./adt_ffi.mjs" "goInspect"

go_make_rect : Float -> Float -> Shape
foreign pure go_make_rect from "./adt_ffi.mjs" "goMakeRect"

go_make_raw_rect : Float -> Float -> Shape
foreign pure go_make_raw_rect from "./adt_ffi.mjs" "goMakeRawRect"

go_make_dot : Unit -> Shape
foreign pure go_make_dot from "./adt_ffi.mjs" "goMakeDot"

go_make_ok : Int -> Result String Int
foreign pure go_make_ok from "./adt_ffi.mjs" "goMakeOk"

go_make_err : String -> Result String Int
foreign pure go_make_err from "./adt_ffi.mjs" "goMakeErr"

describe_shape : Shape -> String
let describe_shape s =
  match s with
  | Dot -> "dot"
  | Circle r -> "circle:" ++ show r
  | Rect w h -> "rect:" ++ show w ++ "x" ++ show h

describe_res : Result String Int -> String
let describe_res r =
  match r with
  | Ok n -> "ok:" ++ show n
  | Err e -> "err:" ++ e

main : Unit ->{Host.Console} Unit
let main () =
  let nan = 0.0 / 0.0
  let neg0 = -1.0 * 0.0
  let inf = 1.0 / 0.0
  let c_pos = Circle 0.0
  let c_neg = Circle neg0
  let c_nan1 = Circle nan
  let c_nan2 = Circle nan
  let c_inf = Circle inf
  let rect = Rect 2.0 3.5

  // Round-trip through polymorphic Flick, generic runtime, and Go FFI.
  let rt_rect = go_roundtrip rect
  let ffi_rect = go_make_rect 2.0 3.5
  let raw_rect = go_make_raw_rect 2.0 3.5
  let ffi_dot = go_make_dot ()
  let poly_rect = id_poly rect

  let ok_v : Result String Int = Ok 42
  let err_v : Result String Int = Err "boom"
  let rt_ok = go_roundtrip ok_v
  let rt_err = go_roundtrip err_v
  let ffi_ok = go_make_ok 42
  let ffi_err = go_make_err "boom"
  let mapped_ok = Result.map (fun n -> n + 8) ok_v

  let ok_nan1 : Result Float Int = Err nan
  let ok_nan2 : Result Float Int = Err nan
  let ok_pos0 : Result Float Int = Err 0.0
  let ok_neg0 : Result Float Int = Err neg0

  // Record aggregate and nested ADT storage.
  let note : Note = { color = Green; shape = ffi_rect; outcome = ffi_ok }
  let env1 = Nested (Some 7) rect
  let env2 = Nested None Dot

  // HashMap (generic erased runtime container) agrees with typed hash & eq.
  let smap =
    HashMap.insert c_neg "zero" (HashMap.insert c_nan1 "nan" (HashMap.insert rect "rect" HashMap.empty))
  let rmap =
    HashMap.insert ok_v "forty-two" (HashMap.insert err_v "failed" HashMap.empty)

  // Codec.json round-trip on deriving Schema closed ADT.
  let codec = Codec.json Shape.schema
  let enc_rect =
    match Codec.encode_result codec rect with
    | Ok j -> Json.stringify j
    | Err _ -> "enc-err"
  let enc_dot =
    match Codec.encode_result codec Dot with
    | Ok j -> Json.stringify j
    | Err _ -> "enc-err"
  let dec_rect =
    match Json.parse enc_rect with
    | Ok j ->
      match Codec.decode_result codec j with
      | Ok s -> describe_shape s ++ "|" ++ show (s == rect)
      | Err _ -> "dec-err"
    | Err _ -> "parse-err"
  let dec_dot =
    match Json.parse enc_dot with
    | Ok j ->
      match Codec.decode_result codec j with
      | Ok s -> describe_shape s ++ "|" ++ show (s == Dot)
      | Err _ -> "dec-err"
    | Err _ -> "parse-err"

  let lines = [
    "match:" ++ describe_shape rt_rect ++ "|" ++ describe_shape ffi_rect ++ "|" ++ describe_shape raw_rect ++ "|" ++ describe_shape ffi_dot ++ "|" ++ describe_shape poly_rect,
    "res:" ++ describe_res rt_ok ++ "|" ++ describe_res rt_err ++ "|" ++ describe_res ffi_ok ++ "|" ++ describe_res ffi_err ++ "|" ++ describe_res mapped_ok,
    "eq:" ++ show (c_pos == c_neg) ++ "|" ++ show (c_nan1 == c_nan2) ++ "|" ++ show (eq c_pos c_neg) ++ "|" ++ show (eq c_nan1 c_nan2) ++ "|" ++ show (rect == ffi_rect) ++ "|" ++ show (Red == Red) ++ "|" ++ show (Red == Blue),
    "res_eq:" ++ show (ok_v == ffi_ok) ++ "|" ++ show (err_v == ffi_err) ++ "|" ++ show (ok_nan1 == ok_nan2) ++ "|" ++ show (ok_pos0 == ok_neg0) ++ "|" ++ show (eq ok_nan1 ok_nan2),
    "ord:" ++ show (compare Dot c_pos) ++ "|" ++ show (compare c_pos c_neg) ++ "|" ++ show (compare c_inf c_nan1) ++ "|" ++ show (Dot < rect) ++ "|" ++ show (err_v < ok_v) ++ "|" ++ show (ok_pos0 <= ok_neg0) ++ "|" ++ show (compare err_v ok_v),
    "hash:" ++ show (hash c_pos == hash c_neg) ++ "|" ++ show (hash c_nan1 == hash c_nan2) ++ "|" ++ show (hash rect == hash (id_poly rect)) ++ "|" ++ show (hash ok_v == hash (id_poly ok_v)) ++ "|" ++ show (hash err_v == hash (id_poly err_v)) ++ "|" ++ show (hash ok_v == hash 42) ++ "|" ++ show (hash err_v == 1 + hash "boom"),
    "show:" ++ show Dot ++ "|" ++ show c_neg ++ "|" ++ show c_nan1 ++ "|" ++ show rect ++ "|" ++ show ok_v ++ "|" ++ show err_v ++ "|" ++ show env1 ++ "|" ++ show env2,
    "map:" ++ show (HashMap.get c_pos smap) ++ "|" ++ show (HashMap.get c_nan2 smap) ++ "|" ++ show (HashMap.get ffi_rect smap) ++ "|" ++ show (HashMap.get ffi_ok rmap) ++ "|" ++ show (HashMap.get ffi_err rmap),
    "note:" ++ show note.color ++ "|" ++ describe_shape note.shape ++ "|" ++ describe_res note.outcome,
    "ffi:" ++ go_inspect Dot ++ "|" ++ go_inspect rect ++ "|" ++ go_inspect ok_v ++ "|" ++ go_inspect err_v,
    "codec:" ++ dec_rect ++ "|" ++ dec_dot,
    "custom:" ++ show (eq (Custom 1) (Custom 2)) ++ "|" ++ show (Custom 1),
  ]
  Platform.stdout_line (String.join "\n" lines)
"#;

const ADT_FFI_GO: &str = r##"package main

import (
	"fmt"
	"strings"

	"flick.app/flickrt"
)

func goRoundtrip(_ *flickrt.Fiber, v flickrt.Value) flickrt.Value {
	return v
}

func goInspect(_ *flickrt.Fiber, v flickrt.Value) flickrt.Value {
	c := v.(flickrt.Constructed)
	name, _, _ := strings.Cut(c.TagName(), "#")
	return fmt.Sprintf("%s/%d", name, len(c.Fields()))
}

func goMakeRect(_ *flickrt.Fiber, w, h flickrt.Value) flickrt.Value {
	tag := flickrt.TagField(Box_A_Types__Shape(A_Types__Shape{tag: 2})).(string)
	return flickrt.Construct(tag, w, h)
}

func goMakeRawRect(_ *flickrt.Fiber, w, h flickrt.Value) flickrt.Value {
	// Also exercise the flickrt.Constructed fallback in As_A_* via raw flickrt.NewData.
	tag := flickrt.TagField(Box_A_Types__Shape(A_Types__Shape{tag: 2})).(string)
	return flickrt.NewData(tag, w, h)
}

func goMakeDot(_ *flickrt.Fiber, _ flickrt.Value) flickrt.Value {
	tag := flickrt.TagField(Box_A_Types__Shape(A_Types__Shape{})).(string)
	return flickrt.Construct(tag)
}

func goMakeOk(_ *flickrt.Fiber, v flickrt.Value) flickrt.Value {
	return flickrt.NewData("Success", v)
}

func goMakeErr(_ *flickrt.Fiber, v flickrt.Value) flickrt.Value {
	return flickrt.NewData("Failure", v)
}
"##;

#[test]
#[ignore = "requires Go"]
fn compiled_closed_adt_and_result_allocations_meet_targets_in_debug_and_release() {
    let config = format!("{CONFIG}[tools]\n\"flick/go\" = [\"go\"]\n");
    for profile in ["debug", "release"] {
        let root = project(
            &format!("go_closed_adt_allocs_{profile}"),
            &[
                ("flick.toml", &config),
                ("src/Types.fl", ALLOC_TYPES_FL),
                ("src/Worker.fl", ALLOC_WORKER_FL),
                ("src/App.fl", ALLOC_APP_FL),
            ],
        );
        let build = flick_in(
            &root,
            &["build", "--target", "go", "--profile", profile, "--offline"],
        );
        assert!(build.status.success(), "{profile}: {build:?}");
        let go_dir = root.join("dist/go/main");
        let worker_go = fs::read_to_string(go_dir.join("m_Worker.go")).expect("read m_Worker.go");
        let adts_go = fs::read_to_string(go_dir.join("adt_types.go")).expect("read adt_types.go");

        // Verify typed cross-module functions use no `flickrt.Value` / `any` / heap allocation.
        assert!(
            !worker_go.contains("flickrt.Value")
                && !worker_go.contains("Box_A")
                && !worker_go.contains("New_T_")
                && !worker_go.contains("flickrt.NewData"),
            "{profile}: unexpected boxing/allocation in m_Worker.go:\n{worker_go}"
        );
        assert!(
            worker_go.contains("A_Types__Shape") && worker_go.contains("AI_Prelude__Result__"),
            "{profile}: missing by-value ADT types in m_Worker.go:\n{worker_go}"
        );

        // Extract exact generated Result box helper name so the Go benchmark stays hash-resilient.
        let res_box = adts_go
            .lines()
            .find_map(|l| {
                let rest = l.strip_prefix("func ")?;
                rest.starts_with("Box_AI_Prelude__Result__")
                    .then(|| rest.split('(').next().expect("fn name"))
            })
            .expect("Box_AI_Prelude__Result__ helper");
        let go_test = ALLOC_GO_TEST.replace("Box_AI_Prelude__Result__b1387822a2a71e80", res_box);
        fs::write(go_dir.join("adt_alloc_test.go"), go_test).expect("write go test");

        let test = Command::new("go")
            .args(["test", "-v", "-run", "TestCompiledClosedAdtAllocs", "."])
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
fn closed_adt_and_result_generic_runtime_ffi_derived_and_codec_agree_in_debug_and_release() {
    let config = format!("{CONFIG}[tools]\n\"flick/go\" = [\"go\"]\n");
    let root = project(
        "go_closed_adt_semantics",
        &[
            ("flick.toml", &config),
            ("src/Types.fl", SEMANTICS_TYPES_FL),
            ("src/App.fl", SEMANTICS_APP_FL),
            ("src/adt_ffi.go", ADT_FFI_GO),
        ],
    );
    for profile in ["debug", "release"] {
        let run = flick_in(
            &root,
            &["run", "--target", "go", "--profile", profile, "--offline"],
        );
        assert!(run.status.success(), "{profile}: {run:?}");
        let expected = concat!(
            "match:rect:2x3.5|rect:2x3.5|rect:2x3.5|dot|rect:2x3.5\n",
            "res:ok:42|err:boom|ok:42|err:boom|ok:50\n",
            "eq:true|true|true|true|true|true|false\n",
            "res_eq:true|true|true|true|true\n",
            "ord:LT|EQ|LT|true|true|true|LT\n",
            "hash:true|true|true|true|true|true|true\n",
            "show:Dot|Circle -0|Circle NaN|Rect 2 3.5|Ok 42|Err \"boom\"|Nested (Some 7) (Rect 2 3.5)|Nested None Dot\n",
            "map:Some \"zero\"|Some \"nan\"|Some \"rect\"|Some \"forty-two\"|Some \"failed\"\n",
            "note:Green|rect:2x3.5|ok:42\n",
            "ffi:Dot/0|Rect/2|Success/1|Failure/1\n",
            "codec:rect:2x3.5|true|dot|true\n",
            "custom:true|custom-value\n",
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            expected,
            "profile={profile}"
        );
    }
}
