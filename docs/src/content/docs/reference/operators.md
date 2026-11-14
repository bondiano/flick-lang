---
title: "Numbers and operators"
---

## Numbers and operators

There are three numeric types, and no implicit conversions between them (see
[Schema and standard data types](../../spec/07-schema/)):

| Type | Values | Arithmetic |
| - | - | - |
| `Int` | 32-bit signed integers | wraps around: `2147483647 + 1 == -2147483648`; `/` drops the fraction (`7 / 2 == 3`, `-7 / 2 == -3`); `/` and `%` by zero are `0` |
| `Float` | IEEE 754 binary64, a JS number | `0.0 / 0.0` is NaN, `1.0 / 0.0` is `Infinity`; `==` is SameValueZero |
| `BigInt` | integers of any size | `/` drops the fraction; `/` and `%` by zero are `0n` |

The same rules hold on every backend. `Int.checked_add`, `Int.checked_div`
and the others return `Result Int.ArithmeticError Int` instead of wrapping or
giving `0`; `Int.unsafe_div` and `Int.unsafe_rem` are a defect on a zero
divisor. `Int.to_float`, `Int.from_float` (`None` unless whole and in range)
and `Int.truncate` convert.

A number literal written as an integer (`3`, `0xFF`, `1_000`) is of any
numeric type its use asks for, and `Int` when nothing does: `let ratio :
Float = 3` and `3 + 0.5` are `Float`, `3 + 4n` is `BigInt`. A literal with a
fraction or an exponent (`3.0`, `1e3`) is a `Float`. An integer literal out
of the range of its type is `FLICK-T003`. When a class constraint is all that
decides a literal's type, it is the first of `Int`, `Float`, `BigInt` with a
matching instance.

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
`f a <| x` is the call `f a x`. A composition is the closure `let f' = f;
let g' = g; fun x -> g' (f' x)`: it evaluates its operands once, left to
right, where it is written, and is typed as that closure. Its arrow has the
rows of the calls it makes: two pure functions compose into a pure one, and
`show >> Platform.stdout_line` is a runtime function. It holds what its
operands hold ([regions](../regions/)). In release the closure is gone where
it is applied: `(f >> g) x` is `g (f x)`.

`1.5 + 1n` and `"a" ++ 1` are `FLICK-T003`. An operand nothing else
determines is `Int` (`String` for `++`). Ranges `a..b`, `a..<b` and `a..`
count in `Int`.

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

Counts, sizes, indices, hashes and calendar parts are `Int` throughout the
standard library (`List.length`, `HashMap.size`, `Hash.hash`, `Queue.size`,
`Schedule.recurs`, the parts of a `DateTime`); milliseconds, epochs, byte
sizes and measurements stay `Float`, as 32 bits do not hold them.
