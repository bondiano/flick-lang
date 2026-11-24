//! Go and Flick source fixtures for `go_direct_ffi` end-to-end tests.

pub(crate) const ABI_GO_SRC: &str = r#"package abi

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

func MaybeJson(present bool) any {
	if !present {
		return nil
	}
	return "json-ok"
}

func NestedJson(o struct {
	Val struct {
		Val any
		Ok  bool
	}
	Ok bool
}) (struct {
	Val any
	Ok  bool
}, bool) {
	if !o.Ok {
		return struct {
			Val any
			Ok  bool
		}{}, false
	}
	return o.Val, true
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

pub(crate) const V2_GO_SRC: &str =
    "package triple\n\nfunc Triple(x int32) int32 { return x * 3 }\n";

pub(crate) const MAIN_DIRECT_FL: &str = r#"module App.Main exposing (main)

import Bytes exposing (Bytes)
import Duration
import Helper
import Json

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

maybe_json : Bool -> Option Json
foreign pure maybe_json from "example.com/flicktest/abi" "MaybeJson"
  nullable

nested_json : Option (Option Json) -> Option (Option Json)
foreign pure nested_json from "example.com/flicktest/abi" "NestedJson"

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

show_nested_json : Option (Option Json) -> String
let show_nested_json o =
  match o with
  | None -> "None"
  | Some None -> "Some(None)"
  | Some (Some j) -> "Some(Some(" ++ Json.stringify j ++ "))"

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

  let json_none = match maybe_json false with | None -> "none" | Some j -> Json.stringify j
  let json_some = maybe_json true
  let json_some_str = match json_some with | None -> "none" | Some j -> Json.stringify j
  let json_nested =
    show_nested_json (nested_json None)
      ++ "|"
      ++ show_nested_json (nested_json (Some None))
      ++ "|"
      ++ show_nested_json (nested_json (Some json_some))

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
  Platform.stdout_line ("json_opt=" ++ json_none ++ "|" ++ json_some_str ++ "|" ++ json_nested)
  Platform.stdout_line ("async=" ++ async_ok ++ "|" ++ async_err ++ "|" ++ async_cancel ++ "|" ++ Int.to_string cancels)
  Platform.stdout_line ("v2_lib=" ++ Int.to_string (Helper.triple 14))
"#;
