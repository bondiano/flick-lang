# Standard library

The standard library is Flick code over the runtime adapter (§6, §9, §12).
Its modules are built into the compiler: a program names them qualified
without an import (`List.map`, `Ref.make`), or imports names from them like
from any module (`import File exposing (FileSystem, OpenError)`). The
Prelude is seen by every module without qualification. A service or type of
a program shadows a built-in module of the same name for its operations:
with `service Log` declared, `Log.write` is the operation.

Signatures the specification gives are declared word for word;
`flick-core/tests/stdlib_signatures.rs` checks it.

## Prelude

| Name | What it is |
| - | - |
| `Eq`, `Ord`, `Hash`, `Show` | canonical equality, the total order, hashing, display |
| `Functor`, `Foldable`, `Traversable`, `Applicative`, `Monad` | classes over `f : Type -> Type`: `map`, `fold`, `traverse`, `pure`, `flat_map` |
| `bind`, `bind_eff` | sequencing for any monad, and for `Eff` with the rows joined |
| `from_result` | `Result (Errors e) a ->{; e} a` |
| `parallel`, `parallel_all`, `parallel_results`, `parallel_exits`, `race_success`, `race_first`, `timeout`, `timeout_option` | structured concurrency, see [runtime.md](runtime.md) |
| `Failure`, `Exit`, `RawCause`, `CauseReport` | opaque outcomes |

`Option`, `Result`, `List` and `Eff r e` have instances of the classes that
fit them: `Functor`, `Foldable`, `Traversable` (sequential), `Applicative`
and `Monad`.

## Modules

| Module | Contents |
| - | - |
| `List` | `map` (pure callback), `map_effect` (runtime callback, one element after another), `map_indexed`, `each_effect`, folds, `filter`, `find`, `sort` (stable, by `Ord`), `sort_by`, `get`, `slice`, `range`, `zip`, … |
| `Option`, `Result` | `map`, `bind`, `with_default` and conversions |
| `NonEmpty` | lists with at least one element |
| `Chunk` | Effect's `Chunk`, for repeated concatenation |
| `Js.Array` | mutable JS arrays, behind runtime operations; lists are copied in and out |
| `String` | `split`, `join`, `slice`, `index_of`, `to_number`, … over UTF-16 code units |
| `Number` | `strict_equal` (`===`), `same_value` (`Object.is`), rounding, `parse`, bitwise `bit_*` and `shift_*`, checked integer operations `add_int`, `div_int`, … returning `Result ArithmeticError Number` |
| `BigInt` | conversions and checked `divide`, `remainder` |
| `Ref` | `make`, `get`, `set`, `modify`, and `same` for identity; no `Eq` or `Hash` |
| `Redacted` | `make`, `reveal` (runtime); `show` prints `<redacted>` |
| `BigDecimal` | Effect's `BigDecimal`: `from_string`, exact `add`, `subtract`, checked `multiply`, `divide_to_scale` |
| `Duration` | Effect's `Duration`: checked `nanos`, `millis`, `seconds`, `minutes`, explicit `infinity` |
| `EqHash`, `HashSet`, `HashMap` | hash collections that keep their witness |
| `Loop` | `loop` over a `Step` and `Trampoline`, for stack-safe pure recursion |
| `Platform` | `stdout_line`, `stderr_line`, `stdout_write`, `args`, `env` |
| `Log` | the `Log` service with `info`, `warn`, `error`, `debug`, and `Log.console` |
| `File` | the `FileSystem` service: `open_scoped`, `read_all`, `read_text`, `write_text`, and `File.node` |
| `Resource` | `acquire_release` into a scope's owner, and `bracket` |
| `Events` | `queue` policies and the `log_failures` supervisor |
| `Ui`, `Ui.Event` | user interfaces rendered with React, see [ui.md](ui.md) |
| `JsError` | `JsError.from_js`, the error of a foreign throw or rejection, see [ffi.md](ffi.md) |
| `Fiber`, `Failure`, `Exit`, `RawCause` | children of a scope and outcomes |
| `Test`, `Gen` | tests and generators, see below |

## Numbers

`Ord Number` is a total order: `-Infinity` < finite numbers < `Infinity` <
`NaN`, signed zeros equal and all NaNs equal, so `List.sort` puts `NaN` last.
The operators `<`, `<=`, `>`, `>=` keep their JS meaning.

`BigDecimal.divide_to_scale scale mode dividend divisor` computes the
quotient with `scale` digits after the point from the coefficients as BigInt,
without a global precision:

| `mode` | `5 / 2` | `-5 / 2` | `11 / 4` | `-11 / 4` |
| - | - | - | - | - |
| `TowardZero` | 2 | -2 | 2 | -2 |
| `Floor` | 2 | -3 | 2 | -3 |
| `Ceiling` | 3 | -2 | 3 | -2 |
| `HalfEven` | 2 | -2 | 3 | -3 |
| `HalfAwayFromZero` | 3 | -3 | 3 | -3 |
| `Exact` | `DecimalError` | `DecimalError` | `DecimalError` | `DecimalError` |

A zero divisor and a scale that is not a safe integer are `DecimalError`
too. `Duration.millis (-1)`, `Duration.millis Number.nan` and a conversion
that overflows are `Err (DurationError …)`; an unbounded duration is
`Duration.infinity`, and `Duration.to_millis` gives `None` for it.

## Hash collections

A `HashSet w a` or `HashMap w k v` keeps the witness of equality and hashing
it was made with, and its identity `w` is part of the type. `HashSet.empty`
uses the canonical instances (`w` is `EqHash.Canonical`); `EqHash.with eq
hash body` makes a witness of a fresh identity and passes it to `body`,
which cannot return anything mentioning it. Insertion and lookup use the
set's own witness, and `HashSet.union` of sets of two witnesses is a type
error: `HashSet.rehash witness set` moves a set to another witness first.

```flick
case_blind : Number
let case_blind =
  EqHash.with
    (fun a b -> String.to_lower a == String.to_lower b)
    (fun s -> hash (String.to_lower s))
    (fun witness -> HashSet.size (HashSet.insert "ADA" (HashSet.insert "ada" (HashSet.with witness))))
```

## Tests

A module of the test root exports `tests : List Test.Test`:

```flick
module Test.Text exposing (tests)

tests : List Test.Test
let tests =
  [
    Test.test "trims" (fun () -> Test.equal "a" (String.trim " a ")),
    Test.effect_test "reads" (task { Test.expect (Test.equal 1 1) }),
    Test.property "reverse twice" (Gen.list 10 (Gen.int 0 9)) (fun xs ->
      Test.equal xs (List.reverse (List.reverse xs))),
    Test.group "numbers" [Test.test "adds" (fun () -> Test.equal 2 (1 + 1))]
  ]
```

| Function | Test |
| - | - |
| `Test.test name check` | pure: `check ()` is an `Assertion` |
| `Test.effect_test name task` | the task runs in a scope of its own, which owns what it acquires; `Test.expect` fails it on an assertion |
| `Test.property name gen check` | `check` on 100 values of `gen`, drawn from the run's seed |
| `Test.group name tests` | `name >` before the names of `tests` |

`flick test` builds the project with its test root into `.flick/test/` and
runs every test; the full name of a test is `Module > group > name`.

| Option | Meaning |
| - | - |
| `--filter <text>` | only tests whose full name contains `text` |
| `--jobs <n>` | at most `n` tests at a time, `1` by default |
| `--seed <n>` | the seed of the generators; a failed run prints the one it used |
| `--reporter human\|json` | lines per test and a summary, or one JSON object |
| `--watch` | run again when a file of the project changes |

Each result is `passed` or `failed` with what failed: `assertion`, `failure`
(a typed error), `defect`, `cleanup` (the test's scope did not close
cleanly; a test whose body passed fails too) or `interrupted`. The exit code
is `0` when every test passed, `1` otherwise. SIGINT interrupts the running
tests, and every one of them closes its scope before the runner exits; under
`--watch` a change stops the run the same way (the watcher closes the
runner's stdin, so it works on every platform) and starts the next run only
after that.
