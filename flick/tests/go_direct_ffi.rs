//! End-to-end tests for direct Go package bindings, the Go value ABI table
//! (`flick.go-ffi@0.1.0`), and `[target.go.dependencies]` (`go.mod` / `go.sum`).
//!
//! Ignored by default because building and executing Go binaries needs `go` on `PATH`;
//! runs in `just e2e-go` and `just e2e-tests`.

mod common;
#[path = "go_direct_ffi/proxy.rs"]
mod proxy;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{APP, flick_in, project};
use flick_core::build::go::executable;

fn flick_go(dir: &Path, proxy_dir: &Path, gomodcache: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env_remove("NO_COLOR")
        .env("GOPROXY", proxy::file_url(proxy_dir))
        .env("GONOSUMDB", "*")
        .env("GOSUMDB", "off")
        .env("GOMODCACHE", gomodcache)
        .output()
        .expect("flick runs")
}

const ABI_GO_SRC: &str = r#"package abi

import (
	"context"
	"errors"
	"sync/atomic"
	"time"
)

var sharedBytes = []byte("orig-bytes")
var sharedInts = []int32{10, 20, 30}
var unitTicks atomic.Int32
var cancelCount atomic.Int32

type OpaqueBox struct {
	Value int32
}

func AddInt(a, b int32) int32 { return a + b }

func XorBool(a, b bool) bool { return a != b }

func TickUnit() { unitTicks.Add(1) }

func ReadTicks() int32 { return unitTicks.Load() }

func FailUnit(fail bool) error {
	if fail {
		return errors.New("unit-boom")
	}
	return nil
}

func MutateBytes(b []byte) []byte {
	if len(b) > 0 {
		b[0] = '!'
	}
	out := sharedBytes
	return out
}

func CorruptSharedBytes() {
	sharedBytes[0] = 'X'
}

func MutateInts(xs []int32) []int32 {
	if len(xs) > 0 {
		xs[0] = 999
	}
	return sharedInts
}

func CorruptSharedInts() {
	sharedInts[0] = -1
}

func SumNested(xss [][]int32) [][]int32 {
	out := make([][]int32, len(xss))
	for i, row := range xss {
		var s int32
		cp := make([]int32, len(row))
		for j, v := range row {
			s += v
			cp[j] = v * 2
		}
		if len(row) > 0 {
			row[0] = -777
		}
		out[i] = append(cp, s)
	}
	return out
}

func SumVariadic(nums ...int32) int32 {
	var total int32
	for _, n := range nums {
		total += n
	}
	return total
}

func OptIntPtr(p *int32) *int32 {
	if p == nil {
		return nil
	}
	v := *p + 1
	return &v
}

func OptBoolPtr(p *bool) *bool {
	if p == nil {
		return nil
	}
	v := !*p
	return &v
}

func OptIntCommaOk(x int32) (int32, bool) {
	if x < 0 {
		return 0, false
	}
	return x * 2, true
}

func NestedOptRoundTrip(o struct {
	Val *int32
	Ok  bool
}) (*int32, bool) {
	if !o.Ok {
		return nil, false
	}
	if o.Val == nil {
		return nil, true
	}
	v := *o.Val + 10
	return &v, true
}

func NewOpaque(v int32) *OpaqueBox { return &OpaqueBox{Value: v} }

func ReadOpaque(b any) int32 { return b.(*OpaqueBox).Value }

func MaybeOpaque(present bool, v int32) *OpaqueBox {
	if !present {
		return nil
	}
	return &OpaqueBox{Value: v}
}

func WaitAndReport(ctx context.Context, delayMs int32) (string, error) {
	if delayMs < 0 {
		return "", errors.New("negative-delay")
	}
	select {
	case <-ctx.Done():
		cancelCount.Add(1)
		return "", ctx.Err()
	case <-time.After(time.Duration(delayMs) * time.Millisecond):
		return "completed", nil
	}
}

func CanceledCount() int32 { return cancelCount.Load() }
"#;

const V2_GO_SRC: &str = "package triple\n\nfunc Triple(x int32) int32 { return x * 3 }\n";

const MAIN_DIRECT_FL: &str = r#"module App.Main exposing (main)

import Bytes exposing (Bytes)
import Duration
import Helper

type error FsErr = FsErr String
type error AbiErr = AbiErr String

from_fs_err : String -> FsErr
let from_fs_err msg = FsErr msg

from_abi_err : String -> AbiErr
let from_abi_err msg = AbiErr msg

type OpaqueBox

hypot : Float -> Float -> Float
foreign pure hypot from "math" "Hypot"

to_upper : String -> String
foreign pure to_upper from "strings" "ToUpper"

cut_prefix : String -> String -> Option String
foreign pure cut_prefix from "strings" "CutPrefix"

read_file : String ->{; FsErr} Bytes
foreign read_file from "os" "ReadFile"
  catch from_fs_err

lookup_env : String ->{} Option String
foreign lookup_env from "os" "LookupEnv"

add_int : Int -> Int -> Int
foreign pure add_int from "example.com/flicktest/abi" "AddInt"

xor_bool : Bool -> Bool -> Bool
foreign pure xor_bool from "example.com/flicktest/abi" "XorBool"

tick_unit : Unit ->{} Unit
foreign tick_unit from "example.com/flicktest/abi" "TickUnit"
  arity 0

read_ticks : Unit ->{} Int
foreign read_ticks from "example.com/flicktest/abi" "ReadTicks"
  arity 0

fail_unit : Bool ->{; AbiErr} Unit
foreign fail_unit from "example.com/flicktest/abi" "FailUnit"
  catch from_abi_err

mutate_bytes : Bytes ->{} Bytes
foreign mutate_bytes from "example.com/flicktest/abi" "MutateBytes"

corrupt_shared_bytes : Unit ->{} Unit
foreign corrupt_shared_bytes from "example.com/flicktest/abi" "CorruptSharedBytes"
  arity 0

mutate_ints : List Int ->{} List Int
foreign mutate_ints from "example.com/flicktest/abi" "MutateInts"

corrupt_shared_ints : Unit ->{} Unit
foreign corrupt_shared_ints from "example.com/flicktest/abi" "CorruptSharedInts"
  arity 0

sum_nested : List (List Int) ->{} List (List Int)
foreign sum_nested from "example.com/flicktest/abi" "SumNested"

sum_variadic : List Int -> Int
foreign pure sum_variadic from "example.com/flicktest/abi" "SumVariadic"
  variadic

opt_int_ptr : Option Int -> Option Int
foreign pure opt_int_ptr from "example.com/flicktest/abi" "OptIntPtr"
  nullable

opt_bool_ptr : Option Bool -> Option Bool
foreign pure opt_bool_ptr from "example.com/flicktest/abi" "OptBoolPtr"
  nullable

opt_int_ok : Int -> Option Int
foreign pure opt_int_ok from "example.com/flicktest/abi" "OptIntCommaOk"

nested_opt : Option (Option Int) -> Option (Option Int)
foreign pure nested_opt from "example.com/flicktest/abi" "NestedOptRoundTrip"

new_opaque : Int -> OpaqueBox
foreign pure new_opaque from "example.com/flicktest/abi" "NewOpaque"

read_opaque : OpaqueBox -> Int
foreign pure read_opaque from "example.com/flicktest/abi" "ReadOpaque"

maybe_opaque : Bool -> Int -> Option OpaqueBox
foreign pure maybe_opaque from "example.com/flicktest/abi" "MaybeOpaque"
  nullable

wait_and_report : Int ->{Async; AbiErr} String
foreign wait_and_report from "example.com/flicktest/abi" "WaitAndReport"
  catch from_abi_err

canceled_count : Unit ->{} Int
foreign canceled_count from "example.com/flicktest/abi" "CanceledCount"
  arity 0

show_nested : Option (Option Int) -> String
let show_nested o =
  match o with
  | None -> "None"
  | Some None -> "Some(None)"
  | Some (Some n) -> "Some(Some(" ++ Int.to_string n ++ "))"

main : Unit ->{Host.Console, Host.Clock, Async; AppError} Unit
let main () =
  let h = hypot 3.0 4.0
  let u = to_upper "flick-go"
  let cut = cut_prefix "go:ok" "go:"
  let env_miss = lookup_env "FLICK_UNSET_DIRECT_FFI_VAR_99999"

  let ok_read =
    try
      match Bytes.decode_utf8 (read_file "sample.txt") with
      | Some text -> text
      | None -> "bad-utf8"
    with
    | FsErr msg -> "err:" ++ msg

  let miss_read =
    try
      let _ = read_file "does-not-exist-anywhere.txt"
      "unexpected-ok"
    with
    | FsErr msg ->
        if String.length msg > 0 then "caught-missing" else "empty-err"

  tick_unit ()
  tick_unit ()
  let ticks = read_ticks ()
  let unit_err =
    try
      fail_unit true
      "no-err"
    with
    | AbiErr msg -> msg

  let orig_bytes = Bytes.encode_utf8 "hello"
  let ret_bytes = mutate_bytes orig_bytes
  corrupt_shared_bytes ()
  let orig_b_str = Option.with_default "" (Bytes.decode_utf8 orig_bytes)
  let ret_b_str = Option.with_default "" (Bytes.decode_utf8 ret_bytes)

  let orig_list = [1, 2, 3]
  let ret_list = mutate_ints orig_list
  corrupt_shared_ints ()

  let orig_mat = [[1, 2], [3, 4]]
  let ret_mat = sum_nested orig_mat

  let opq_none = match maybe_opaque false 9 with | None -> "none" | Some _ -> "some"
  let opq_some = match maybe_opaque true 42 with | None -> -1 | Some b -> read_opaque b

  let async_ok =
    try wait_and_report 1 with
    | AbiErr msg -> "err:" ++ msg
  let async_err =
    try wait_and_report (-1) with
    | AbiErr msg -> "err:" ++ msg
  let async_cancel =
    scope {
      match Duration.millis 40.0 with
      | Ok d ->
          Effect.timeout_or_else
            d
            (task { "timed-out" })
            (task {
              try wait_and_report 5000 with
              | AbiErr msg -> "err:" ++ msg
            })
      | Err _ -> "bad-duration"
    }
  let cancels = canceled_count ()

  Platform.stdout_line ("hypot=" ++ Float.to_string h)
  Platform.stdout_line ("upper=" ++ u)
  Platform.stdout_line ("cut=" ++ Option.with_default "none" cut)
  Platform.stdout_line ("env=" ++ Option.with_default "none" env_miss)
  Platform.stdout_line ("read_ok=" ++ ok_read)
  Platform.stdout_line ("read_err=" ++ miss_read)
  Platform.stdout_line ("int=" ++ Int.to_string (add_int 19 23))
  Platform.stdout_line ("bool=" ++ show (xor_bool true false) ++ "," ++ show (xor_bool true true))
  Platform.stdout_line ("unit=" ++ Int.to_string ticks ++ "," ++ unit_err)
  Platform.stdout_line ("bytes=" ++ orig_b_str ++ "|" ++ ret_b_str)
  Platform.stdout_line ("list=" ++ show orig_list ++ "|" ++ show ret_list)
  Platform.stdout_line ("nested_list=" ++ show orig_mat ++ "|" ++ show ret_mat)
  Platform.stdout_line ("variadic=" ++ Int.to_string (sum_variadic [10, 20, 12]))
  Platform.stdout_line (
    "opt_int_ptr="
      ++ show (opt_int_ptr None)
      ++ "|"
      ++ show (opt_int_ptr (Some 0))
      ++ "|"
      ++ show (opt_int_ptr (Some 41))
  )
  Platform.stdout_line (
    "opt_bool_ptr="
      ++ show (opt_bool_ptr None)
      ++ "|"
      ++ show (opt_bool_ptr (Some false))
      ++ "|"
      ++ show (opt_bool_ptr (Some true))
  )
  Platform.stdout_line ("opt_ok=" ++ show (opt_int_ok (-1)) ++ "|" ++ show (opt_int_ok 21))
  Platform.stdout_line (
    "nested_opt="
      ++ show_nested (nested_opt None)
      ++ "|"
      ++ show_nested (nested_opt (Some None))
      ++ "|"
      ++ show_nested (nested_opt (Some (Some 32)))
  )
  Platform.stdout_line (
    "opaque=" ++ Int.to_string (read_opaque (new_opaque 7)) ++ "|" ++ opq_none ++ "|" ++ Int.to_string opq_some
  )
  Platform.stdout_line ("async=" ++ async_ok ++ "|" ++ async_err ++ "|" ++ async_cancel ++ "|" ++ Int.to_string cancels)
  Platform.stdout_line ("v2_lib=" ++ Int.to_string (Helper.triple 14))
"#;

fn setup_direct_abi_fixture(base: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let proxy_dir = base.join("proxy");
    let gomodcache = base.join("gomodcache");
    proxy::remove_readonly_tree(&gomodcache);
    fs::create_dir_all(&gomodcache).expect("gomodcache");

    proxy::write_module(
        &proxy_dir,
        "example.com/flicktest/abi",
        "v1.0.0",
        &[("abi.go", ABI_GO_SRC)],
    );
    proxy::write_module(
        &proxy_dir,
        "example.com/flicktest/triple/v2",
        "v2.0.0",
        &[("triple.go", V2_GO_SRC)],
    );

    let lib_dir = base.join("helper");
    fs::create_dir_all(lib_dir.join("src")).expect("lib src");
    fs::write(
        lib_dir.join("flick.toml"),
        "schema-version = 1\n\
         [project]\n\
         kind = \"lib\"\n\
         [package]\n\
         name = \"helper\"\n\
         version = \"0.1.0\"\n\
         [package.abi]\n\
         \"flick.go-ffi\" = \"^0.1.0\"\n\
         [target.go.dependencies]\n\
         \"example.com/flicktest/triple/v2\" = \"v2.0.0\"\n",
    )
    .expect("lib toml");
    fs::write(
        lib_dir.join("src/Helper.fl"),
        "module Helper exposing (triple)\n\
         triple : Int -> Int\n\
         foreign pure triple from \"example.com/flicktest/triple/v2\" \"Triple\"\n",
    )
    .expect("helper fl");

    let app_dir = base.join("app");
    fs::create_dir_all(app_dir.join("src/App")).expect("app src");
    fs::write(
        app_dir.join("flick.toml"),
        format!(
            "{APP}\n\
             [build]\n\
             target = \"go\"\n\n\
             [dependencies]\n\
             helper = {{ path = \"../helper\" }}\n\n\
             [target.go.dependencies]\n\
             \"example.com/flicktest/abi\" = \"v1.0.0\"\n"
        ),
    )
    .expect("app toml");
    fs::write(app_dir.join("src/App/Main.fl"), MAIN_DIRECT_FL).expect("main fl");
    fs::write(app_dir.join("sample.txt"), "hello-from-disk").expect("sample.txt");
    (proxy_dir, gomodcache, app_dir)
}

#[test]
#[ignore = "needs Go"]
fn direct_go_bindings_cover_full_abi_table_cancellation_and_go_mod_sum() {
    let base = project("go_direct_abi_full", &[]);
    let (proxy_dir, gomodcache, app_dir) = setup_direct_abi_fixture(&base);

    let build = flick_go(&app_dir, &proxy_dir, &gomodcache, &["build"]);
    assert!(
        build.status.success(),
        "build failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr),
    );

    // Generated go.mod and go.sum contain both root and library dependencies.
    let gen_mod =
        fs::read_to_string(app_dir.join("dist/go/main/go.mod")).expect("generated go.mod");
    assert!(
        gen_mod.contains("example.com/flicktest/abi v1.0.0"),
        "{gen_mod}"
    );
    assert!(
        gen_mod.contains("example.com/flicktest/triple/v2 v2.0.0"),
        "{gen_mod}"
    );
    let gen_sum =
        fs::read_to_string(app_dir.join("dist/go/main/go.sum")).expect("generated go.sum");
    assert!(
        gen_sum.contains("example.com/flicktest/abi v1.0.0 h1:"),
        "{gen_sum}"
    );
    assert!(
        gen_sum.contains("example.com/flicktest/triple/v2 v2.0.0 h1:"),
        "{gen_sum}"
    );

    let run = Command::new(app_dir.join("dist").join(executable("main")))
        .current_dir(&app_dir)
        .output()
        .expect("run compiled Go binary");
    assert!(run.status.success(), "{run:?}");
    let out = String::from_utf8(run.stdout).expect("utf-8 stdout");
    let expected = [
        "hypot=5",
        "upper=FLICK-GO",
        "cut=ok",
        "env=none",
        "read_ok=hello-from-disk",
        "read_err=caught-missing",
        "int=42",
        "bool=true,false",
        "unit=2,unit-boom",
        "bytes=hello|orig-bytes",
        "list=[1, 2, 3]|[10, 20, 30]",
        "nested_list=[[1, 2], [3, 4]]|[[2, 4, 3], [6, 8, 7]]",
        "variadic=42",
        "opt_int_ptr=None|Some 1|Some 42",
        "opt_bool_ptr=None|Some true|Some false",
        "opt_ok=None|Some 42",
        "nested_opt=None|Some(None)|Some(Some(42))",
        "opaque=7|none|42",
        "async=completed|err:negative-delay|timed-out|1",
        "v2_lib=42",
    ]
    .join("\n");
    assert_eq!(out.trim_end(), expected, "unexpected stdout:\n{out}");

    // Supplying project root go.sum carries checksums into the build cleanly.
    fs::write(app_dir.join("go.sum"), &gen_sum).expect("write project go.sum");
    let clean_rebuild = flick_go(
        &app_dir,
        &proxy_dir,
        &gomodcache,
        &["build", "--out-dir", "dist2"],
    );
    assert!(
        clean_rebuild.status.success(),
        "rebuild with project go.sum failed: {}",
        String::from_utf8_lossy(&clean_rebuild.stderr)
    );
    let gen_sum_2 = fs::read_to_string(app_dir.join("dist2/go/main/go.sum")).expect("dist2 go.sum");
    assert!(
        gen_sum_2.contains("example.com/flicktest/abi v1.0.0 h1:"),
        "{gen_sum_2}"
    );
    proxy::remove_readonly_tree(&gomodcache);
}

#[test]
#[ignore = "needs Go"]
fn out_of_table_go_types_fail_with_f013_and_legacy_sibling_go_keeps_working() {
    let bad_dir = project(
        "go_direct_f013",
        &[
            ("flick.toml", &format!("{APP}\n[build]\ntarget = \"go\"\n")),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\
                 bad : { x : Int } -> Int\n\
                 foreign pure bad from \"example.com/pkg\" \"Bad\"\n\
                 main : Unit -> Int\n\
                 let main () = bad { x = 1 }\n",
            ),
        ],
    );
    let check = flick_in(&bad_dir, &["check"]);
    assert!(!check.status.success(), "expected F013 failure");
    let stderr = String::from_utf8_lossy(&check.stderr);
    assert!(stderr.contains("FLICK-F013"), "{stderr}");
    assert!(
        stderr.contains("structural records have no stable Go struct layout"),
        "{stderr}"
    );
    assert!(stderr.contains("App/Main.fl:3:"), "{stderr}");

    // Legacy sibling `.go` binding keeps working with `flickrt.Value` for records.
    let legacy_dir = project(
        "go_legacy_sibling",
        &[
            ("flick.toml", &format!("{APP}\n[build]\ntarget = \"go\"\n")),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\
                 read_x : { x : Int } -> Int\n\
                 foreign pure read_x from \"./legacy.mjs\" \"ReadX\"\n\
                 main : Unit ->{Host.Console; AppError} Unit\n\
                 let main () = Platform.stdout_line (Int.to_string (read_x { x = 77 }))\n",
            ),
            (
                "src/App/legacy.mjs",
                "export function ReadX(r) { return r.x; }\n",
            ),
            (
                "src/App/legacy.go",
                "package main\nimport \"flick.app/flickrt\"\n\
                 func ReadX(_ *flickrt.Fiber, r flickrt.Value) flickrt.Value {\n\
                   return flickrt.Field(r, \"x\")\n\
                 }\n",
            ),
        ],
    );
    let run = flick_in(&legacy_dir, &["run"]);
    assert!(
        run.status.success(),
        "legacy sibling .go run failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "77\n");
}

#[test]
#[ignore = "needs Go"]
fn target_go_dependencies_reject_invalid_versions_and_conflicts() {
    let bad_ver = project(
        "go_deps_bad_ver",
        &[
            (
                "flick.toml",
                &format!(
                    "{APP}\n[build]\ntarget = \"go\"\n\
                     [target.go.dependencies]\n\
                     \"example.com/mod\" = \"1.0.0\"\n"
                ),
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    let out = flick_in(&bad_ver, &["install"]);
    assert!(!out.status.success(), "expected P032 for missing v prefix");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FLICK-P032"), "{stderr}");

    let conflict = project(
        "go_deps_conflict",
        &[
            (
                "flick.toml",
                &format!(
                    "{APP}\n[build]\ntarget = \"go\"\n\
                     [target.go.dependencies]\n\
                     \"example.com/mod\" = \"v1.2.0\"\n"
                ),
            ),
            (
                "go.mod",
                "module app\n\ngo 1.23\n\nrequire example.com/mod v1.1.0\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    let out = flick_in(&conflict, &["install"]);
    assert!(!out.status.success(), "expected P040 for go.mod conflict");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FLICK-P040"), "{stderr}");
}
