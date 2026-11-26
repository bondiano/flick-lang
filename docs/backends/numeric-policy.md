# Numeric behavior required of backends

Flick's numeric policy covers the default numeric types (`Int`, `Float`,
`BigInt`) and the fixed-width numeric types (`Int8`, `Int16`, `Int32`, `Int64`,
`UInt8`, `Byte`, `UInt16`, `UInt32`, `UInt64`, `Float32`). Across every fixed-width
integer type, ordinary arithmetic wraps modulo 2^W and division/remainder by zero
returns zero; checked operations return `Result Int.ArithmeticError T`; unsafe
division/remainder raises a defect on zero divisor. Backend extraction and target
codegen never replace any of these behaviors with a target's default trap.

## Accepted type set and relationship to `Int`

| Type | Width / Kind | Range / Format | Literal suffix | Relationship |
| --- | --- | --- | --- | --- |
| `Int8` | 8-bit signed | `-128..=127` | `i8` | Distinct nominal built-in (`@lang int8`) |
| `Int16` | 16-bit signed | `-32768..=32767` | `i16` | Distinct nominal built-in (`@lang int16`) |
| `Int` / `Int32` | 32-bit signed | `-2147483648..=2147483647` | none or `i32` | `type alias Int32 = Int`; identical in types, ABI, and runtime values |
| `Int64` | 64-bit signed | `-9223372036854775808..=9223372036854775807` | `i64` | Distinct nominal built-in (`@lang int64`) |
| `UInt8` / `Byte` | 8-bit unsigned | `0..=255` | `u8` | `type alias Byte = UInt8`; identical in types, ABI, and runtime values |
| `UInt16` | 16-bit unsigned | `0..=65535` | `u16` | Distinct nominal built-in (`@lang uint16`) |
| `UInt32` | 32-bit unsigned | `0..=4294967295` | `u32` | Distinct nominal built-in (`@lang uint32`) |
| `UInt64` | 64-bit unsigned | `0..=18446744073709551615` | `u64` | Distinct nominal built-in (`@lang uint64`) |
| `Float32` | 32-bit IEEE 754 | binary32 | `f32` | Distinct nominal built-in (`@lang float32`) |
| `Float` | 64-bit IEEE 754 | binary64 | none (fractional) | Default floating type (`@lang float`) |
| `BigInt` | Arbitrary precision | unbounded signed integers | `n` | Arbitrary-precision integer (`@lang bigint`) |

- `Int` remains the default signed 32-bit wrapping integer type; `Int32` is a
  transparent type alias (`type alias Int32 = Int`), and `Byte` is a transparent
  type alias (`type alias Byte = UInt8`).
- There are **no implicit conversions** between any two numeric types (including
  widening, signed/unsigned, or `Float32` <-> `Float`).
- Lossless widenings are provided explicitly through `Into` (e.g. `Int8 -> Int16`,
  `UInt8 -> Int16`, `UInt32 -> Int64`, `Int64 -> BigInt`, `Float32 -> Float`).
- Narrowing and cross-sign conversions are explicit module functions: checked
  (`from_* -> Option T`), wrapping (`wrapping_from_* -> T`), saturating
  (`saturating_from_* -> T`), and float truncation (`truncate : Float -> T`).
- Radix literals accept integer suffixes (`0xFFu8`, `0b1010_0101u16`, `0o77i8`).
  In hexadecimal literals, `f` is a hex digit so `0x1f32` remains an unsuffixed
  hex integer (`7986`). Integer suffixes on fractions/exponents and `f32` on
  non-decimal radices are lexical errors (`FLICK-S006`). Negative minimum boundary
  literals (`-128i8`, `-32768i16`, `-2147483648i32`, `-9223372036854775808i64`)
  are valid; out-of-range literals (including `-1u8`) are rejected (`FLICK-T003`).

## Observable rules

These rules apply in debug and release, whether an expression is folded,
inlined, specialized or executed by a runtime helper.

| Operation | Required result | Independent examples |
| --- | --- | --- |
| Signed fixed-width `+`, `-`, `*`, unary `-` (`Int8`, `Int16`, `Int`/`Int32`, `Int64`) | Two's-complement W-bit wrapping | `127i8 + 1i8 = -128i8`, `-(-128i8) = -128i8`, `max + 1 = min`, `65536 * 65536 = 0` |
| Unsigned fixed-width `+`, `-`, `*`, unary `-` (`UInt8`/`Byte`, `UInt16`, `UInt32`, `UInt64`) | Modular W-bit wrapping (`-x = (2^W - x) mod 2^W`) | `255u8 + 1u8 = 0u8`, `0u8 - 1u8 = 255u8`, `-(1u8) = 255u8`, `4294967295u32 * 4294967295u32 = 1u32` |
| Fixed-width integer division `/` | Truncate toward zero; zero divisor gives `0`; signed `min / -1 = min` | `-7i8 / 2i8 = -3i8`, `5i8 / 0i8 = 0i8`, `-128i8 / -1i8 = -128i8`, `18446744073709551615u64 / 2u64 = 9223372036854775807u64` |
| Fixed-width integer remainder `%` | Dividend's sign; zero divisor gives `0`; signed `min % -1 = 0` | `-7i8 % 2i8 = -1i8`, `7i8 % -2i8 = 1i8`, `-128i8 % -1i8 = 0i8` |
| Fixed-width integer exponentiation `**` | Full-width W-bit wrapping binary exponentiation; negative signed powers truncate (`1 -> 1`, `-1 -> ±1`, else `0`); `0 ** 0 = 1` | `2u64 ** 4294967296u64 = 0u64`, `(-1i64) ** 4294967297i64 = -1i64`, `2i8 ** -1i8 = 0i8`, `0u8 ** 0u8 = 1u8` |
| Checked fixed-width arithmetic (`checked_add/sub/mul/div/rem`, `divide`, `remainder`) | `Result Int.ArithmeticError T`; reject overflow, underflow, zero divisor, and signed `min / -1` | `Int8.checked_div (-128i8) (-1i8)` is `Err`; `Int8.checked_rem (-128i8) (-1i8)` is `Ok 0i8` |
| Unsafe fixed-width division/remainder (`unsafe_div`, `unsafe_rem`) | Defect for zero divisor (with normal cleanup); otherwise the ordinary wrapping result | `Int.unsafe_div min -1 = min`; zero division runs finalizers before its defect is observed |
| Fixed-width shifts (`shift_left`, `shift_right`, `shift_right_unsigned`) | Count masked modulo bit-width `W` (`n & (W - 1)`); signed arithmetic right shift sign-extends, unsigned/logical right shift zero-fills within `W` bits | `Int8.shift_left 1i8 8 = 1i8`, `Int8.shift_right_unsigned (-128i8) 7 = 1i8`, `Int64.shift_right_unsigned Int64.min_value 63 = 1i64` |
| `T.from_float` / `T.from_float32` | `Some` only for finite, integral values in target range; normalize `-0.0` | `-42.0` -> `Some (-42i8)`; fractions, NaN, infinities, out-of-range -> `None` |
| `T.truncate` | Truncate toward zero, wrap modulo 2^W; nonfinite values give `0` | `Int8.truncate 128.9 = -128i8`; NaN and infinities give `0` |
| `T.saturating_from_float` / `saturating_from_*` | Clamp out-of-range to `[min_value, max_value]`; NaN gives `0` | `UInt64.saturating_from_float 1e30 = UInt64.max_value`; `Int8.saturating_from_int 500 = 127i8` |
| `BigInt` arithmetic | Arbitrary precision within available resources | `2n ** 128n = 340282366920938463463374607431768211456n` |
| `BigInt` division/remainder | Truncate toward zero / retain dividend's sign; zero divisor gives `0n` | `-7n / 2n = -3n`, `-7n % 2n = -1n`, `5n / 0n = 0n` |
| `BigInt.divide` / `BigInt.remainder` | Checked `Result`; zero divisor gives `Err ArithmeticError` | Both checked forms reject zero |
| Negative `BigInt` exponent | Defect with normal cleanup | No unchecked host trap or skipped finalizer |
| `BigInt.from_number` / `to_number` / `Float32.from_bigint` | Reject nonintegral/nonfinite input; single round-to-nearest-even to binary64 / binary32 (no double rounding via binary64 for `Float32`) | `Float32.from_bigint 18014399583223809n = 18014400656965632.0f32` (`2^54 + 2^31`) |
| `Float` and `Float32` arithmetic | IEEE 754 binary64 / binary32, including NaN, infinities, subnormals and signed zero | `16777216.0f32 + 1.0f32 = 16777216.0f32`; `1.0f32 / -0.0f32 = -Infinity` |
| `Float` and `Float32` equality and ordering | Structural `==` uses SameValueZero; `< <= > >=` leave NaN unordered; `compare` is total with NaN last | `NaN == NaN`; `-0.0f32 == 0.0f32`; `Float32.same_value` distinguishes zeros; `compare Infinity NaN = LT` |
| `Show` / `Display` formatting | `Int8..Int64` and `UInt8..UInt64` print decimal without suffixes (`Int64`/`UInt64` never append `n`); `Float` and `Float32` use shortest round-trip JS spelling retaining `-0` | `show (-9223372036854775808i64) = "-9223372036854775808"`; `show 18446744073709551615u64 = "18446744073709551615"` |

Numeric class methods (`Numeric`, `Integer`, `Floating`) and specialized
operators must implement the exact same rules:
- `Numeric`: `Int`, `Float`, `BigInt`, `Int8`, `Int16`, `Int64`, `UInt8`, `UInt16`, `UInt32`, `UInt64`, `Float32`.
- `Integer`: `Int`, `BigInt`, `Int8`, `Int16`, `Int64`, `UInt8`, `UInt16`, `UInt32`, `UInt64`.
- `Floating`: `Float`, `Float32`.

## Backend representation strategy

Every target preserves the exact observable rules above while picking an
idiomatic native representation:

| Target | `Int8` / `Int16` / `Int` (`Int32`) | `UInt8` (`Byte`) / `UInt16` / `UInt32` | `Int64` / `UInt64` | `Float32` | Guards & notes |
| --- | --- | --- | --- | --- | --- |
| **JS / TS** | JS `number` (`((x) << 24) >> 24`, `((x) << 16) >> 16`, `(x) \| 0`); TS `number` | JS `number` (`(x) & 0xff`, `(x) & 0xffff`, `(x) >>> 0`; `Math.imul(a, b) >>> 0` for `UInt32` `*`); TS `number` | JS `BigInt` (`BigInt.asIntN(64, ...)`, `BigInt.asUintN(64, ...)`); TS `bigint` | JS `number` rounded via `Math.fround(...)`; TS `number` | Runtime helpers for `/`, `%`, `**`, `showInt64`, `showUInt64`, 64-bit bitwise/shifts, and single-rounded `toFloat32` |
| **Go** | Native `int8`, `int16`, `int32` | Native `uint8`, `uint16`, `uint32` | Native `int64`, `uint64` | Native `float32` | `flickrt` helpers guard zero divisor, signed `min / -1`, full-width `**`, SameValueZero `EqFloat32`, total `Compare` (NaN last), and `Hash` parity with JS/Effect |
| **C** | `<stdint.h>` `int8_t`, `int16_t`, `int32_t` | `uint8_t`, `uint16_t`, `uint32_t` | `int64_t`, `uint64_t` | IEEE 754 `float` | Perform signed wrapping `+`/`-`/`*`/unary `-` in unsigned `uintN_t` to avoid C signed-overflow UB; guard zero divisor and `INTN_MIN / -1`; mask shift counts modulo `W`; use IEEE `SameValueZero` / total-`Ord` helpers |
| **WebAssembly** | `i32` with width extension (`i32.extend8_s`, `i32.extend16_s`) | `i32` with width masking (`& 0xff`, `& 0xffff`) | `i64` | `f32` | Guard `i32.div_s`/`i32.div_u`/`i64.div_s`/`i64.div_u` and `rem_*` against zero divisor and `MIN / -1` traps; mask sub-32-bit shift counts modulo `W`; use `f32` `SameValueZero` and total-`Ord` helpers |
| **Lua 5.1 / LuaJIT** | Lua `number` (exact for all 32-bit ints) with sign-extension/masking (`bit.tobit`/shifts on LuaJIT or modular arithmetic on stock Lua 5.1) | Lua `number` with bitmasking (`& 0xff`, `& 0xffff`, `mod 2^32`) | every dialect: the kernel's `BigInt` value (a sign and limbs of 24 bits) wrapped to 64 bits after each operation, as JS keeps a `bigint`; no FFI, never a lossy binary64 `number` | Lua `number` rounded to IEEE binary32 after each operation | Guard zero divisor and `min / -1`; avoid 53-bit `number` precision loss on `UInt32` multiplication and all 64-bit operations |

## Implementation and evidence

- Shared literal parsing, arithmetic, consteval, folding, specialization, and layout:
  `flick-syntax/src/syntax/{lexer/numbers,literal}.rs`,
  `flick-backend-support/src/{numeric,consteval}.rs`,
  `flick-backend-support/src/opt/{fold,specialize,eval/}.rs`, and
  `flick-backend-support/src/layout/catalog.rs`.
- Portable IR: `flick-ir` `0.16.0` adds `Lang`, `Num`, and `Lit` variants for
  `Int8`, `Int16`, `Int64`, `UInt8`, `UInt16`, `UInt32`, `UInt64`, and `Float32`,
  with JSON and CBOR round-trip tests (`fixed_width_literals_survive_both_forms`).
- JS backend & runtimes: `backends/js/src/codegen/js/{literal,expr,binary}.rs`,
  `runtime/src/core/fixed_width.ts`, and `packages/effect-ts/src/index.ts`.
- Go backend & runtime: `backends/go/src/codegen/{shape,arith,expr}.rs`,
  `runtime-go/flickrt/{fixed_width,data,numbers,dict}.go`, and
  `packages/std/src/go/runtime.txt`.
- Core & stdlib: `packages/core/src/{Prelude,Numeric,Instances}.fl` and
  `packages/std/src/{Int8,Int16,Int32,Int64,UInt8,Byte,UInt16,UInt32,UInt64,Float32}.fl`.
- Reference model & Lean: `flick-model` executes all fixed-width literals,
  operators, comparisons, hashes, conversions, and 64-bit bitwise operations in
  both the small-step λFlick interpreter and the counted memory VM.
  `formal/Formal/FixedWidth.lean` proves modular wrapping, unsigned additive
  inverses, total div/rem (`min / -1`, div-by-0), shift masking, checked/saturating/
  lossless widening conversion soundness, fixed-width interval preorders, and
  total `Ord` with `NaN` greatest, pinned in `formal/Formal/Axioms.lean` with
  zero `sorry` and no extra axioms. As with `Formal/Refine/Numbers.lean`, these
  Lean theorems verify the specification contract, not the Rust compiler or
  JS/Go runtime implementations.
- Independent tests:
  - Type-checker pass/fail: `tests/check/pass/types/fixed_width.fl`,
    `tests/check/fail/types/fixed_width.fl`,
    `tests/check/fail/syntax/fixed_width_suffix.fl`.
  - Language boundary suite (JS, Go, RT): `tests/language/test/Language/{Ints,BigInts,Floats,Equality,NumericBoundaries,FixedWidth}.fl`.
  - Constant-folded vs runtime parity (JS, Go, RT, model, debug & release):
    `tests/run/codegen/ints.fl`, `tests/run/codegen/bigints.fl`,
    `tests/run/codegen/fixed_width.fl`.

## Component and future target obligations

A backend's target analysis may choose another internal representation or native
instruction when it implements this contract. It must insert guards or helpers
for target division, conversion, overflow, shift masking, and floating-point
equality/ordering where needed. Effect order, error handling, and cleanup remain
language requirements. An unsupported operation belongs in `check-target`;
silently using a trapping integer division or another Float/Float32 equality is
incompatible.
