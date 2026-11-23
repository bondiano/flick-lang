---
title: "Numbers and operators"
---

Flick provides default numeric types (`Int`, `Float`, `BigInt`) alongside
fixed-width integer and single-precision floating-point types (`Int8`, `Int16`,
`Int32`, `Int64`, `UInt8`, `Byte`, `UInt16`, `UInt32`, `UInt64`, `Float32`),
with no implicit conversions between any of them (see [Schema and standard data types](../../spec/07-schema/)).
Their rules are identical in debug and release on JS and Go. There are no
user-defined operators; the precedence of the fixed ones is in the
[operator table](../syntax/#operator-table).

## Numeric types

| Type | Values | Suffix | Arithmetic |
| - | - | - | - |
| `Int8` | 8-bit signed (`-128..=127`) | `i8` | wraps mod 2⁸; `/` truncates toward zero; `/` and `%` by zero are `0i8`; `-128i8 / -1i8 == -128i8` |
| `Int16` | 16-bit signed (`-32768..=32767`) | `i16` | wraps mod 2¹⁶; `/` and `%` by zero are `0i16`; `-32768i16 / -1i16 == -32768i16` |
| `Int` (`Int32`) | 32-bit signed (`-2147483648..=2147483647`) | none or `i32` | `type alias Int32 = Int`; wraps mod 2³² (`2147483647 + 1 == -2147483648`); `/` and `%` by zero are `0` |
| `Int64` | 64-bit signed (`-9223372036854775808..=9223372036854775807`) | `i64` | wraps mod 2⁶⁴; `/` and `%` by zero are `0i64`; `min_value / -1i64 == min_value` |
| `UInt8` (`Byte`) | 8-bit unsigned (`0..=255`) | `u8` | `type alias Byte = UInt8`; wraps mod 2⁸ (`-(1u8) == 255u8`); `/` and `%` by zero are `0u8` |
| `UInt16` | 16-bit unsigned (`0..=65535`) | `u16` | wraps mod 2¹⁶; `/` and `%` by zero are `0u16` |
| `UInt32` | 32-bit unsigned (`0..=4294967295`) | `u32` | wraps mod 2³²; `/` and `%` by zero are `0u32` |
| `UInt64` | 64-bit unsigned (`0..=18446744073709551615`) | `u64` | wraps mod 2⁶⁴; `/` and `%` by zero are `0u64` |
| `BigInt` | arbitrary-precision signed integers | `n` | exact; `/` truncates toward zero; `/` and `%` by zero are `0n` |
| `Float32` | IEEE 754 binary32 | `f32` | `0.0f32 / 0.0f32` is NaN, `1.0f32 / 0.0f32` is `Infinity`; `==` is SameValueZero |
| `Float` | IEEE 754 binary64 | none (fractional) | `0.0 / 0.0` is NaN, `1.0 / 0.0` is `Infinity`; `==` is SameValueZero |

`Int32` is a transparent alias for `Int` (`type alias Int32 = Int`), and `Byte`
is a transparent alias for `UInt8` (`type alias Byte = UInt8`). They have
identical types, ABI representations, and runtime values.

An unsuffixed integer literal (`3`, `0xFF`, `1_000`) adapts to any numeric type
its context asks for, defaulting to `Int` when nothing constrains it:
`let ratio : Float = 3` and `3 + 0.5` are `Float`, `3 + 4n` is `BigInt`, and
`let b : UInt8 = 255` is `UInt8`. An unsuffixed fractional or exponent literal
(`3.0`, `1e3`) adapts to any `Floating` type (`Float` or `Float32`), defaulting
to `Float`.

A suffixed literal fixes its type immediately: `127i8`, `-32768i16`, `42i32`,
`-9223372036854775808i64`, `0xFFu8`, `0b1010_0101u16`, `0o77u32`,
`18446744073709551615u64`, `1.5f32`, `1e-3f32`. Radix literals accept integer
suffixes; in hexadecimal (`0x…`), `f` is a hex digit so `0x1f32` remains an
unsuffixed hex integer (`7986`). Integer suffixes on fractions/exponents and
`f32` on non-decimal radices are `FLICK-S006`. Any integer literal outside the
range of its type — including negative literals for unsigned types such as
`-1u8` — is `FLICK-T003`. When only a class constraint constrains an unsuffixed
literal, it defaults to the first of `Int`, `Float`, `BigInt` with a matching
instance.

Counts, sizes, indices, hashes and calendar parts are `Int` throughout the
standard library (`List.length`, `HashMap.size`, `Hash.hash`, `Queue.size`,
`Schedule.recurs`, the parts of a `DateTime`); milliseconds, epochs, byte
sizes and measurements stay `Float`, as 32 bits do not hold them.

## Overflow, division and conversions

Choose the failure behavior at the call site:

| Form | Overflow | Zero divisor |
| --- | --- | --- |
| Ordinary fixed-width integer operators (`Int8`..`UInt64`, `Int`) | Wrap modulo 2^W (including unary `-`) | `/` and `%` give `0` |
| `T.checked_add`, `checked_sub`, `checked_mul`, `checked_div`, `checked_rem`, `divide`, `remainder` | Return `Err Int.ArithmeticError` when the result does not fit (including signed `min / -1`) | Division/remainder return `Err` |
| `T.unsafe_div`, `unsafe_rem` | Keep ordinary wrapping integer behavior | Raise a defect |
| Ordinary `BigInt` operators | Arbitrary precision within available memory | `/` and `%` give `0n` |
| `BigInt.divide`, `BigInt.remainder` | Arbitrary precision | Return `Err Int.ArithmeticError` |

An arithmetic defect follows normal failure and cleanup rules. It is not a
typed arithmetic error. For fallible input, use a checked form when the caller
needs to handle invalid arithmetic as data.

```flick
import Int8
import UInt8

wrapped : Int
let wrapped = 2147483647 + 1

checked : Result Int.ArithmeticError Int
let checked = Int.checked_div (-2147483648) (-1)

ordinary : BigInt
let ordinary = 5n / 0n

byte_wrap : UInt8
let byte_wrap = 255u8 + 1u8

widened : Int64
let widened = into (-128i8)

clamped : Int8
let clamped = Int8.saturating_from_int 500

checked_u8 : Option UInt8
let checked_u8 = UInt8.from_int 256
```

`wrapped` is `-2147483648`; `checked` is an error because positive
`2147483648` does not fit in `Int`; `ordinary` is `0n`; `byte_wrap` is `0u8`;
`widened` is `-128i64`; `clamped` is `127i8`; `checked_u8` is `None`.
`Int.checked_rem (-2147483648) (-1)` succeeds with `Ok 0`.

Integer division truncates toward zero, and remainder keeps the dividend's
sign: `-7 / 2` is `-3`, `-7 % 2` is `-1`. Fixed-width powers wrap; negative
signed powers truncate (`2 ** -1` is `0`, `(-1i64) ** -3i64` is `-1i64`). A
negative `BigInt` exponent is a defect. Bitwise shifts (`shift_left`,
`shift_right`, `shift_right_unsigned`) mask the shift count modulo the type's
bit-width (`n & (W - 1)`).

Conversions between numeric types are always explicit:

- **Lossless widening (`Into`)**: `into` converts any integer to a strictly
  wider integer of the same sign, any unsigned integer to a strictly wider
  signed integer, any fixed-width integer to `BigInt`, `Int8`/`Int16`/`UInt8`/
  `UInt16` to `Float32`, `Int8`..`UInt32` to `Float`, and `Float32` to `Float`.
- **Checked narrowing/cross-sign (`T.from_*`)**: returns `Some x` when exact and
  in range, `None` otherwise (`Int.from_float`, `UInt8.from_int`,
  `Int64.from_uint64`, `BigInt.from_number`, …).
- **Wrapping (`T.wrapping_from_*`, `T.truncate`)**: wraps modulo 2^W; float
  truncation drops the fraction and maps nonfinite values (`NaN`, `±Infinity`)
  to `0`.
- **Saturating (`T.saturating_from_*`)**: clamps out-of-range values to
  `min_value`/`max_value`; `NaN` maps to `0`.
- **Float rounding**: `Float32.from_float`, `Float32.from_bigint`,
  `Int64.to_float`, `BigInt.to_number` round to nearest even (with single
  rounding directly to binary32 for `Float32.from_bigint`).

`Int.parse` trims leading and trailing ECMAScript whitespace and line
terminators (`\t`, `\n`, `\v`, `\f`, `\r`, space, U+00A0, U+1680,
U+2000–U+200A, U+2028, U+2029, U+202F, U+205F, U+3000 and U+FEFF; not U+0085),
then requires an optional ASCII `+` or `-` sign followed by one or more ASCII
decimal digits within `-2147483648..2147483647`. Negative zero normalizes to
`0`. Empty or sign-only input, internal whitespace, non-decimal text and
out-of-range values return `None`. The fixed-width modules `Int8`..`UInt64` and
`Float32` expose matching `parse` and `to_string` helpers; `show` and `display`
on `Int64`/`UInt64` print plain decimal digits without an `n` suffix.

## Floats

`Float` is IEEE 754 binary64 and `Float32` is IEEE 754 binary32, both including
signed zero, subnormals, NaN and infinities. Structural `==` uses SameValueZero:
NaN equals itself and the two zeros are equal. Numeric `<`, `<=`, `>` and `>=`
leave NaN unordered; `compare` supplies a total order with NaN last.
`Float.same_value` and `Float32.same_value` distinguish negative zero from
positive zero. `show` preserves `-0`; `Float.round` and `Float32.round` round
ties toward positive infinity, so `Float.round (-2.5)` is `-2`.

These rules also constrain optimization and future backends. Changing a
representation or using a native machine instruction must preserve their
results, defects and cleanup.

## Operators

| Operators | Operands | Result |
| - | - | - |
| `+ - * / % **`, prefix `-` | both of the same `Numeric` type (`Int8`..`UInt64`, `Int`, `BigInt`, `Float32`, `Float`) | the same type |
| `++` | both `String` or both `List a` | the same type |
| `< <= > >=` | both of the same numeric type or `String`, or both of another type with `Ord` | `Bool` |
| `== !=` | the same type | `Bool` |
| `&& \|\|`, `not` | `Bool` | `Bool` |
| `option orelse fallback` | `Option a` and `a` | `a` (evaluates `fallback` only on `None`) |
| `x \|> f`, `f <\| x` | `f x` | the result of `f` |
| `f >> g`, `g << f` | functions | the function that applies `f`, then `g` |

`x |> f` evaluates `x` first, `f <| x` evaluates `f` first, as `f x` does;
`f a <| x` is the call `f a x`. To pipe into another argument position, write
an expression hole `_`: `x |> f a _ c` is `(fun h -> f a h c) x`, evaluating
`x` first and then the capture body; `f a _ c <| x` applies the same capture to
`x`. A composition is the closure `let f' = f;
let g' = g; fun x -> g' (f' x)`: it evaluates its operands once, left to
right, where it is written, and is typed as that closure. Its arrow has the
rows of the calls it makes: two pure functions compose into a pure one, and
`show >> Platform.stdout_line` is a runtime function. It holds what its
operands hold ([regions](../regions/)). In release the closure is gone where
it is applied: `(f >> g) x` is `g (f x)`.

`1.5 + 1n`, `1i8 + 1i16`, and `"a" ++ 1` are `FLICK-T003`. An operand nothing
else determines is `Int` (`String` for `++`). Ranges `a..b`, `a..<b` and `a..`
count in `Int`.

`left orelse right` is lazy `Option` fallback, equivalent to
`match left with | Some value -> value | None -> right`. It evaluates `left`
once and evaluates `right` at most once, only when `left` is `None`. If `left`
fails with a typed error or defect, `right` does not run. It unwraps exactly
one `Option` layer (`Some None orelse Some 1` is `None : Option Int`), never
starts a cold `Eff` or `Lazy` value, and lets `right` fail into the enclosing
error row (for example `opt orelse Fail.fail Missing`). Unlike
`Option.with_default`, which evaluates its default argument eagerly before the
call, `orelse` skips expensive or effectful fallbacks on `Some`.

```flick
fallback : Option String -> String
let fallback host = host orelse "localhost"

first_valid : Option Int -> Option Int -> Int
let first_valid a b = a orelse b orelse 8080

nested_layer : Option (Option Int) -> Option Int
let nested_layer value = value orelse Some 1
```

## Numeric classes

One function serves every numeric type through the closed classes of the
module `Numeric`:

- `Numeric a`: `Int`, `Float`, `BigInt`, `Int8`, `Int16`, `Int64`, `UInt8`, `UInt16`, `UInt32`, `UInt64`, `Float32`.
- `Integer a`: `Int`, `BigInt`, `Int8`, `Int16`, `Int64`, `UInt8`, `UInt16`, `UInt32`, `UInt64`.
- `Floating a`: `Float`, `Float32`.

On a type variable with one of them, the operators and integer literals use its
methods (`add`, `div`, `from_int`, …); a literal with a fraction needs
`Floating`. The module has every instance, and a program cannot declare another
(`FLICK-C012`).

```flick
import Numeric exposing (Floating, Numeric)

square : Numeric a => a -> a
let square x = x * x

mean : Floating a => List a -> a
let mean xs = List.sum xs / Numeric.from_int (List.length xs)
```

`List.sum` and `List.product` take any `Numeric`. Without the constraint,
`x + 1` on a type variable is `FLICK-C001`. `< <= > >=` on a type variable
with a numeric class use its `lt`, `le`, `gt`, `ge`, which mean what the
operators mean on its type (NaN is unordered); with only `Ord a`, they
compare by `compare`. On any other type, such as `Bool`, a derived type, a
record or a list, they compare by its `Ord` instance's `compare`; a type
without one is `FLICK-C001`.
