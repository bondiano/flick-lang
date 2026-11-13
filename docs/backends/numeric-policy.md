# Numeric behavior required of backends

The numeric review confirms the existing policy: ordinary integer arithmetic
wraps and division by zero returns zero. Checked operations return an error;
unsafe operations can raise a defect. Backend extraction does not replace any
of these behaviors with a target's default trap. No new numeric type or
execution rule is introduced by this review.

## Observable rules

These rules apply in debug and release, whether an expression is folded,
inlined, specialized or executed by a runtime helper.

| Operation | Required result | Independent examples |
| --- | --- | --- |
| `Int` addition, subtraction, multiplication and negation | Signed 32-bit wrapping | `max + 1 = min`, `min - 1 = max`, `65536 * 65536 = 0`, `-min = min` |
| `Int` division | Truncate toward zero; zero divisor gives `0` | `-7 / 2 = -3`, `5 / 0 = 0`, `min / -1 = min` |
| `Int` remainder | Dividend's sign; zero divisor gives `0` | `-7 % 2 = -1`, `7 % -2 = 1`, `min % -1 = 0` |
| `Int` exponentiation | Wrap each integer product; truncate negative powers | `3 ** 40 = 689956897`, `2 ** -1 = 0`, `(-1) ** -3 = -1`, `0 ** 0 = 1` |
| Checked `Int` arithmetic | `Result Int.ArithmeticError Int`; reject overflow and zero division/remainder | `checked_div min -1` is `Err`; `checked_rem min -1` is `Ok 0` |
| Unsafe `Int` division/remainder | Defect for zero divisor; otherwise the ordinary integer result | `unsafe_div min -1 = min`; zero division runs finalizers before its defect is observed |
| `Int.from_float` | `Some` only for finite, integral values in signed 32-bit range; normalize negative zero | `-42.0` gives `Some -42`; fractions, NaN and infinities give `None` |
| `Int.truncate` | Truncate, then wrap to signed 32 bits; nonfinite values give `0` | `-2.7` gives `-2`; `2147483648.0` gives `min`; NaN gives `0` |
| `Int` shifts | Count modulo 32; signed results retain 32-bit representation | `shift_left 1 33 = 2`; unsigned right shift returns the low signed 32-bit result |
| `BigInt` arithmetic | Arbitrary precision within available resources | `2n ** 128n = 340282366920938463463374607431768211456n` |
| `BigInt` division/remainder | Truncate toward zero / retain dividend's sign; zero divisor gives `0n` | `-7n / 2n = -3n`, `-7n % 2n = -1n`, `5n / 0n = 0n` |
| `BigInt.divide` / `BigInt.remainder` | Checked `Result`; zero divisor gives `Err ArithmeticError` | Both checked forms reject zero |
| Negative `BigInt` exponent | Defect with normal cleanup | No unchecked host trap or skipped finalizer |
| `BigInt.from_number` / `to_number` | Reject nonintegral/nonfinite input / round to binary64 | NaN and infinity give `None`; `2n ** 53n + 1n` converts to `9007199254740992.0` |
| `Float` arithmetic | IEEE 754 binary64, including NaN, infinities, subnormals and signed zero | `0.1 + 0.2 = 0.30000000000000004`; `1.0 / 0.0 = Infinity` |
| Float equality and ordering | Structural `==` uses SameValueZero; comparisons keep NaN unordered; `compare` is total with NaN last | NaN equals itself with `==`; `-0.0 == 0.0`; `Float.same_value` distinguishes the zeros |
| Float formatting | Shortest round trip with the existing exponent thresholds; retain `-0` | `1e21` prints `1e+21`, `1e-6` prints `0.000001`, negative zero prints `-0` |
| Float rounding | Existing half-toward-positive-infinity `round` and signed-zero behavior | `round -2.5 = -2`; `round -0.5` prints `-0` |

`min` and `max` above denote the signed 32-bit limits. There are no implicit
conversions between the three numeric types. Numeric class methods and
specialized operators must implement the same rules.

## Implementation and evidence

The compiler's literal/folding policy is in `flick-core/src/numeric.rs` and
`opt/fold.rs`. JS uses `codegen/js/binary.rs` and
`runtime/src/core/text.ts`; Go uses `codegen/go/arith.rs` and
`runtime-go/flickrt/{number,effnum}.go`. Float formatting also serves the
reference interpreter through `flick-model/src/native/show.rs`; its existing
`opt::js_number` name does not make this behavior target-specific.

The independent expected values are in
`tests/language/test/Language/{Ints,BigInts,Floats,Equality,NumericBoundaries}.fl`.
`Effects.fl` also distinguishes typed errors and defects. `NumericBoundaries`
adds the checked `min / -1` and remainder boundaries, nonfinite conversions,
negative integer powers, and cleanup after unsafe division and negative BigInt
exponentiation. These tests assert prescribed values and finalizer completion,
not agreement between emitters. The language runner compares JS, Go and RT
reports; both profiles must be exercised for numeric review.

`tests/run/codegen/ints.fl` additionally pins folded and runtime expressions
with an independent `.stdout` under the existing JS/TS/Go debug/release
harness. `numeric::tests` and the runtime number tests exercise lower layers.
No existing fixture expectation or model exclusion is changed by this review.

The reference interpreter covers Int and Float primitives, but does not model
BigInt values or every foreign numeric helper. Lean's safety and cleanup
theorems do not prove the Rust optimizer's numerical equivalence, IEEE
rounding, or either runtime implementation. The runtime and execution tests
remain necessary.

## Component and future target obligations

The backend's target analysis may choose another representation or use native
instructions when they implement this contract. It must insert guards or
helpers for target division, conversion and failure behavior where needed.
Effect order, error handling and cleanup remain language requirements.

W and N must implement these same rules and pass the numeric corpus when they
are introduced. An unsupported operation belongs in `check-target`; silently
using a trapping integer division or another Float equality is incompatible.
Memory/ownership changes require a separately agreed IR version before those
targets, rather than a numeric-policy change hidden in an emitter option.
