---
title: "Numeric boundaries"
---

Flick has three numeric types: `Int`, `Float` and `BigInt`. Their rules are
the same in debug and release on JS and Go. See [types and operators](../types/#numbers-and-operators)
for literals, conversions and numeric classes.

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

`Int.from_float` accepts only finite, integral values within signed 32-bit
range. `Int.truncate` drops the fraction and wraps, and gives zero for NaN or
infinity. `BigInt.from_number` rejects fractions and nonfinite values;
`BigInt.to_number` rounds to binary64 and can lose integer precision.

`Float` is IEEE 754 binary64, including signed zero, NaN and infinities.
Structural `==` uses SameValueZero: NaN equals itself and the two zeros are
equal. Numeric `<`, `<=`, `>` and `>=` leave NaN unordered; `compare` supplies
a total order with NaN last. `Float.same_value` distinguishes negative zero
from positive zero. `show` preserves `-0`; `Float.round` rounds ties toward
positive infinity, so `Float.round (-2.5)` is `-2`.

These rules also constrain optimization and future backends. Changing a
representation or using a native machine instruction must preserve their
results, defects and cleanup.
