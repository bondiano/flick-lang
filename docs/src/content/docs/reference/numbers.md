---
title: "Numbers and operators"
---

Flick has three numeric types, `Int`, `Float` and `BigInt`, and no implicit
conversions between them (see [Schema and standard data types](../../spec/07-schema/)).
Their rules are the same in debug and release on JS and Go. There are no
user-defined operators; the precedence of the fixed ones is in the
[operator table](../syntax/#operator-table).

## Numeric types

| Type | Values | Arithmetic |
| - | - | - |
| `Int` | 32-bit signed integers | wraps around: `2147483647 + 1 == -2147483648`; `/` drops the fraction (`7 / 2 == 3`, `-7 / 2 == -3`); `/` and `%` by zero are `0` |
| `Float` | IEEE 754 binary64, a JS number | `0.0 / 0.0` is NaN, `1.0 / 0.0` is `Infinity`; `==` is SameValueZero |
| `BigInt` | integers of any size | `/` drops the fraction; `/` and `%` by zero are `0n` |

A number literal written as an integer (`3`, `0xFF`, `1_000`) is of any
numeric type its use asks for, and `Int` when nothing does: `let ratio :
Float = 3` and `3 + 0.5` are `Float`, `3 + 4n` is `BigInt`. A literal with a
fraction or an exponent (`3.0`, `1e3`) is a `Float`. An integer literal out
of the range of its type is `FLICK-T003`. When a class constraint is all that
decides a literal's type, it is the first of `Int`, `Float`, `BigInt` with a
matching instance.

Counts, sizes, indices, hashes and calendar parts are `Int` throughout the
standard library (`List.length`, `HashMap.size`, `Hash.hash`, `Queue.size`,
`Schedule.recurs`, the parts of a `DateTime`); milliseconds, epochs, byte
sizes and measurements stay `Float`, as 32 bits do not hold them.

## Overflow and division

Choose the failure behavior at the call site:

| Form | Overflow | Zero divisor |
| --- | --- | --- |
| Ordinary `Int` operators | Wrap in signed 32 bits | `/` and `%` give `0` |
| `Int.checked_add`, `checked_sub`, `checked_mul`, `checked_div`, `checked_rem` | Return `Err Int.ArithmeticError` when the result does not fit | Division/remainder return `Err` |
| `Int.unsafe_div`, `unsafe_rem` | Keep ordinary integer behavior | Raise a defect |
| Ordinary `BigInt` operators | Arbitrary precision within available memory | `/` and `%` give `0n` |
| `BigInt.divide`, `BigInt.remainder` | Arbitrary precision | Return `Err Int.ArithmeticError` |

An arithmetic defect follows normal failure and cleanup rules. It is not a
typed arithmetic error. For fallible input, use a checked form when the caller
needs to handle invalid arithmetic as data.

```flick
wrapped : Int
let wrapped = 2147483647 + 1

checked : Result Int.ArithmeticError Int
let checked = Int.checked_div (-2147483648) (-1)

ordinary : BigInt
let ordinary = 5n / 0n
```

`wrapped` is `-2147483648`; `checked` is an error because positive
`2147483648` does not fit in `Int`; `ordinary` is `0n`.
`Int.checked_rem (-2147483648) (-1)` succeeds with `Ok 0`.

Integer division truncates toward zero, and remainder keeps the dividend's
sign: `-7 / 2` is `-3`, `-7 % 2` is `-1`. `Int` powers wrap; negative powers
truncate, so `2 ** -1` is `0`. A negative `BigInt` exponent is a defect.

`Int.to_float`, `Int.from_float` and `Int.truncate` convert.
`Int.from_float` accepts only finite, integral values within signed 32-bit
range, and is `None` otherwise. `Int.truncate` drops the fraction and wraps,
and gives zero for NaN or infinity. `BigInt.from_number` rejects fractions and
nonfinite values; `BigInt.to_number` rounds to binary64 and can lose integer
precision.

`Int.parse` trims leading and trailing ECMAScript whitespace and line
terminators (`\t`, `\n`, `\v`, `\f`, `\r`, space, U+00A0, U+1680,
U+2000–U+200A, U+2028, U+2029, U+202F, U+205F, U+3000 and U+FEFF; not U+0085),
then requires an optional ASCII `+` or `-` sign followed by one or more ASCII
decimal digits within `-2147483648..2147483647`. Negative zero normalizes to
`0`. Empty or sign-only input, internal whitespace, non-decimal text and
out-of-range values return `None`.

## Floats

`Float` is IEEE 754 binary64, including signed zero, NaN and infinities.
Structural `==` uses SameValueZero: NaN equals itself and the two zeros are
equal. Numeric `<`, `<=`, `>` and `>=` leave NaN unordered; `compare` supplies
a total order with NaN last. `Float.same_value` distinguishes negative zero
from positive zero. `show` preserves `-0`; `Float.round` rounds ties toward
positive infinity, so `Float.round (-2.5)` is `-2`.

These rules also constrain optimization and future backends. Changing a
representation or using a native machine instruction must preserve their
results, defects and cleanup.

## Operators

| Operators | Operands | Result |
| - | - | - |
| `+ - * / % **`, prefix `-` | both `Int`, both `Float` or both `BigInt` | the same type |
| `++` | both `String` or both `List a` | the same type |
| `< <= > >=` | both `Int`, `Float`, `BigInt` or `String`, or both of another type with `Ord` | `Bool` |
| `== !=` | the same type | `Bool` |
| `&& \|\|`, `not` | `Bool` | `Bool` |
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

`1.5 + 1n` and `"a" ++ 1` are `FLICK-T003`. An operand nothing else
determines is `Int` (`String` for `++`). Ranges `a..b`, `a..<b` and `a..`
count in `Int`.

## Numeric classes

One function serves every numeric type through the closed classes of the
module `Numeric`: `Numeric a` (`Int`, `Float`, `BigInt`), `Integer a` (`Int`,
`BigInt`) and `Floating a` (`Float`). On a type variable with one of them,
the operators and integer literals use its methods (`add`, `div`,
`from_int`, …); a literal with a fraction needs `Floating`. The module has
every instance, and a program cannot declare another (`FLICK-C012`).

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
