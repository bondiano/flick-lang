//! Offline Go module fixtures, Flick test programs, and report assertions for
//! `go_ffi_command`.

pub(super) const GO_APP: &str = "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[build]\ntarget = \"go\"\n";

pub(super) const STRINGS_MAIN_FL: &str = r#"module App.Main exposing (main)

import Ffi.Strings.Generated as S

main : Unit ->{Host.Console; AppError} Unit
let main () =
  let upper = S.to_upper "flick-go"
  let cut = match S.cut_prefix "hello:world" "hello:" with
    | Some rest -> rest
    | None -> "none"
  let parts = S.split "a,b,c" ","
  let joined = S.join parts "-"
  let repl = S.new_replacer ["foo", "bar", "baz", "qux"]
  let replaced = S.replacer_replace repl "foo and baz"
  let rdr = S.new_reader "initial"
  S.reader_reset rdr "reset"
  Platform.stdout_line (upper ++ "|" ++ cut ++ "|" ++ joined ++ "|" ++ replaced)
"#;

pub(super) const MATH_MAIN_FL: &str = r#"module App.Main exposing (main)

import Ffi.Math.Generated as M

main : Unit ->{Host.Console; AppError} Unit
let main () =
  let h = M.hypot 3.0 4.0
  let a = M.abs (0.0 - 7.5)
  let s = M.sqrt 81.0
  let nan_check = M.is_na_n (M.na_n ())
  let pi_ok = M.pi > 3.14 && M.pi < 3.15
  Platform.stdout_line (show h ++ "|" ++ show a ++ "|" ++ show s ++ "|" ++ show nan_check ++ "|" ++ show pi_ok)
"#;

pub(super) fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Returns the `/// <provenance> sha256:...` line directly above `name` in `Generated.fl`.
pub(super) fn provenance_of(generated: &str, name: &str) -> String {
    let lines: Vec<&str> = generated.lines().collect();
    let at = lines
        .iter()
        .position(|l| {
            l.starts_with(&format!("{name} :"))
                || l.starts_with(&format!("foreign blocked {name} "))
        })
        .unwrap_or_else(|| panic!("binding `{name}` not found in:\n{generated}"));
    lines[at - 1].trim_start_matches("/// ").to_owned()
}

pub(super) const FIXTURE_V1_GO: &str = r#"package fix

import (
	"context"
	"errors"
	"time"
)

const Answer = 42
const Huge = 1 << 50

func Add(a, b int32) int32 { return a + b }

func EchoBytes(b []byte) []byte {
	out := make([]byte, len(b))
	copy(out, b)
	return out
}

func SumAll(xs ...int32) int32 {
	var total int32
	for _, x := range xs {
		total += x
	}
	return total
}

func Lookup(key string) (string, bool) {
	if key == "ok" {
		return "found", true
	}
	return "", false
}

func MaybeDouble(p *int32) *int32 {
	if p == nil {
		return nil
	}
	v := *p * 2
	return &v
}

func Divide(a, b int32) (int32, error) {
	if b == 0 {
		return 0, errors.New("divide by zero")
	}
	return a / b, nil
}

func CheckPositive(n int32) error {
	if n <= 0 {
		return errors.New("non-positive")
	}
	return nil
}

func WaitEcho(ctx context.Context, ms int32, msg string) (string, error) {
	select {
	case <-time.After(time.Duration(ms) * time.Millisecond):
		return "echo:" + msg, nil
	case <-ctx.Done():
		return "", ctx.Err()
	}
}

type Counter struct{ n int32 }

func NewCounter(start int32) *Counter { return &Counter{n: start} }
func (c *Counter) Add(delta int32) int32 {
	c.n += delta
	return c.n
}
func (c *Counter) Value() int32 { return c.n }

type Greeter interface {
	Greet(name string) string
}

type prefixGreeter struct{ prefix string }

func (g prefixGreeter) Greet(name string) string { return g.prefix + name }

func NewGreeter(prefix string) Greeter { return prefixGreeter{prefix: prefix} }

func GenericId[T any](x T) T { return x }

type Box[T any] struct{ V T }

func (b *Box[T]) Get() T { return b.V }

func Watch(ch <-chan int32) {}

func LogAny(args ...any) {}

type secret struct{ x int32 }

func TouchSecret(s secret) {}
"#;

pub(super) const FIXTURE_V1_1_GO: &str = r#"package fix

import "errors"

func Add(a, b int32) int32 { return a + b }

// Signature changed from (int32, int32) (int32, error) to (float64, float64) (float64, error).
func Divide(a, b float64) (float64, error) {
	if b == 0 {
		return 0, errors.New("divide by zero")
	}
	return a / b, nil
}

type Counter struct{ n int32 }

func NewCounter(start int32) *Counter { return &Counter{n: start} }
func (c *Counter) Add(delta int32) int32 {
	c.n += delta
	return c.n
}
func (c *Counter) Value() int32 { return c.n }
"#;

pub(super) const FIXTURE_POLICY: &str = r#"[exports.Add]
mode = "pure"

[exports.MaybeDouble]
nullable = true
"#;

pub(super) const FIXTURE_MAIN_FL: &str = r#"module App.Main exposing (main)

import Bytes
import Ffi.Fix as Reviewed
import Ffi.Fix.Generated as G
import Ffi.Fix.Generated exposing (GoError(..))

main : Unit ->{Host.Console, Async; AppError} Unit
let main () =
  let a = G.add 10 G.answer
  let b = Option.with_default "" (Bytes.decode_utf8 (G.echo_bytes (Bytes.encode_utf8 "bytes-ok")))
  let s = G.sum_all [1, 2, 3, 4]
  let l1 = Option.with_default "miss" (G.lookup "ok")
  let l2 = Option.with_default "miss" (G.lookup "no")
  let p1 = match G.maybe_double (Some 21) with
    | Some n -> show n
    | None -> "none"
  let p2 = match G.maybe_double None with
    | Some n -> show n
    | None -> "none"
  let d1 = try show (Reviewed.divide 20 4) with
    | GoError { message } -> "err:" ++ message
  let d2 = try show (Reviewed.divide 20 0) with
    | GoError { message } -> "err:" ++ message
  let cp = try
      Reviewed.check_positive 0
      "ok"
    with
    | GoError { message } -> "err:" ++ message
  let c = G.new_counter 5
  let c1 = G.counter_add c 7
  let c2 = G.counter_value c
  let g = G.new_greeter "hi, "
  let gr = G.greeter_greet g "flick"
  let w = try G.wait_echo 1 "async" with
    | GoError { message } -> "err:" ++ message
  Platform.stdout_line (show a ++ "|" ++ b ++ "|" ++ show s ++ "|" ++ l1 ++ "," ++ l2 ++ "|" ++ p1 ++ "," ++ p2)
  Platform.stdout_line (d1 ++ "|" ++ d2 ++ "|" ++ cp ++ "|" ++ show c1 ++ "," ++ show c2 ++ "|" ++ gr ++ "|" ++ w)
"#;

pub(super) fn assert_fixture_v1_report(report: &str) {
    for expected in [
        "supported    add — Int -> Int -> Int",
        "supported    answer — Int",
        "supported    echo_bytes — Bytes ->{} Bytes",
        "supported    sum_all — List Int ->{} Int",
        "supported    lookup — String ->{} Option String",
        "supported    maybe_double — Option Int ->{} Option Int",
        "supported    divide — Int -> Int ->{; GoError} Int",
        "supported    check_positive — Int ->{; GoError} Unit",
        "supported    wait_echo — Int -> String ->{Async; GoError} String",
        "supported    new_counter — Int ->{} Counter",
        "supported    counter_add — Counter -> Int ->{} Int",
        "supported    counter_value — Counter ->{} Int",
        "supported    new_greeter — String ->{} Greeter",
        "supported    greeter_greet — Greeter -> String ->{} String",
    ] {
        assert!(
            report.contains(expected),
            "missing `{expected}` in:\n{report}"
        );
    }
    for (name, reason) in [
        ("generic_id", "generic Go function"),
        ("box_get", "generic Go type"),
        ("watch", "channel"),
        ("log_any", "variadic `any`"),
        ("touch_secret", "unexported"),
        ("huge", "overflows 32-bit `Int`"),
    ] {
        assert!(
            report
                .lines()
                .any(|l| l.contains("unsupported") && l.contains(name) && l.contains(reason)),
            "expected unsupported `{name}` with `{reason}` in:\n{report}"
        );
    }
}

pub(super) fn reviewed_fixture_source(
    divide_prov: &str,
    check_pos_prov: &str,
    with_catch: bool,
) -> String {
    format!(
        "module Ffi.Fix exposing (divide, check_positive)\n\n\
         import Ffi.Fix.Generated exposing (GoError, from_go_error)\n\n\
         divide : Int -> Int ->{{; GoError}} Int\n\
         foreign divide from \"example.com/flicktest/fix\" \"Divide\"\n  \
         arity 2\n{}  \
         reviewed \"{divide_prov}\"\n\n\
         check_positive : Int ->{{; GoError}} Unit\n\
         foreign check_positive from \"example.com/flicktest/fix\" \"CheckPositive\"\n  \
         arity 1\n  \
         catch from_go_error\n  \
         reviewed \"{check_pos_prov}\"\n",
        if with_catch {
            "  catch from_go_error\n"
        } else {
            ""
        }
    )
}
