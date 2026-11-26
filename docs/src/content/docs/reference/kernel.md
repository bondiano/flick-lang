---
title: "Primitive kernel"
---

The primitive kernel is what every backend implements itself: the
operations of the portable IR, the lang items a backend maps by role, and a
small set of `foreign` bindings. Everything else the standard library binds
is either a capability of the host or something Flick code can do over the
kernel, so a new backend implements the kernel and the host capabilities it
supports, and runs the standard library as its bindings move to Flick.

"Kernel" alone means other things in this repository: the fibers, owners
and causes of Flick's own JS runtime (`runtime/src/rt/kernel.ts`), and the
runtime ABI that `FLICK-P015` checks. This page is about the *primitive
kernel* only.

## Version and handshake

The kernel has a version, now `0.1.0`. A build request names it as the
runtime ABI `flick.kernel`, beside `flickrt`, and a backend declares the
versions it implements among the `runtime-abis` of its descriptor, as
`flick/js`, `flick/go` and the `test/literal` fixture do:

```rust
NamedRange { name: "flick.kernel".into(), versions: range("0.1.0", "0.2.0") }
```

The handshake refuses a backend without a matching range before its
`check-target` or `generate` runs, so nothing is written: the build reports
`FLICK-P013` with ``incompatible runtime ABI `flick.kernel` ``. A new
primitive, or a change to what one does, is a new version of the kernel.

## Inventory

`flick-backend-support/src/kernel/manifest.txt` gives each binding that a
`foreign` of `packages/*/src` names, a `(specifier, export)` pair with
`global` for a JS global, one class:

| Class | The binding is |
| - | - |
| `kernel` | a primitive every backend implements |
| `flick` | expressible in Flick over the kernel |
| `host` | a capability of the host, which host packages provide: output, files, processes, the network, clocks, randomness, the locale |
| `adapter` | a binding to the values of one host library, kept by that backend's adapter: JS promises and streams, React, the UI's atoms and forms |

A line `[specifier] class` gives every export of a specifier its class, and
a line `export class` below it gives one export another; a kernel export
always has a line of its own. `cargo test -p flick-backend-support kernel`
fails on a binding the manifest does not classify and on one it names that
the standard library no longer binds, and prints the count of each class
(with `-- --nocapture`). With kernel `0.1.0`, of the 3,084 pairs of 3,275
declarations, 67 are `kernel`, 2,516 `flick`, 366 `host` and 135 `adapter`.

## What a backend maps

A backend finds the built-ins through `Program.lang` by their role (see
[Code generation](../codegen/)). The kernel needs the types `unit`, `bool`,
`int`, `float`, `bigint`, the fixed-width types `int8` to `uint64` and
`float32`, `string`, `list` and `ref`; the constructors `some`, `none`, `ok`
and `err`, and the `Ordering` of the prelude (`LT`, `EQ`, `GT`); and the
classes `eq`, `ord`, `hash` and `show`, whose instances for records and
tuples the backend derives from the hooks below.

The IR operations are primitive too: arithmetic on each numeric type
(`Binary` with `Arith` and `Num`), `==` and `!=` (structural; SameValueZero
on floats), `===`, the comparisons, `++` on strings and on lists, `-` and
`not`, the literals of every type, and tuples, lists, records and
constructors with their fields, updates and patterns.

## Data kernel

### Numbers

Arithmetic, division by zero, wrapping, `**` and the conversions follow the
[numeric policy](https://github.com/bondiano/flick-lang/blob/main/docs/backends/numeric-policy.md).
The bindings:

| Binding | In the standard library | What it does |
| - | - | - |
| `global Number` | `Int.to_float`, `BigInt.to_number` | an `Int` exactly; a `BigInt` rounded to the nearest `Float`, ties to even |
| `global BigInt` | `into : Int -> BigInt` | exactly |
| `global String` | `BigInt.to_string` | decimal digits, `-` for a negative number, no suffix |
| `global Math.floor`, `Math.sqrt` | `Float.floor`, `Float.sqrt` | IEEE 754: `-0` stays `-0`, NaN stays NaN, `sqrt (-1)` is NaN |
| `stringFromNumber` | `Int.to_string`, `Float.to_string` | an `Int` in decimal; a `Float` as `show` writes it |
| `stringToNumber` | `String.to_number` | JS `Number` of the text with JS white space trimmed: decimal, `0x`, `0o`, `0b`, `Infinity`; blank or malformed text is `None`, `NaN` is `Some NaN` |
| `showInt64`, `showUInt64` | `show` of `Int64`, `UInt64` | decimal, without a suffix |
| `intTruncate` | `Int.truncate` | the whole part modulo 2^32; NaN and the infinities are `0` |
| `bigintFromNumber` | `BigInt.from_number` | `Some` for a finite whole `Float`, `-0` giving `0n` |
| `toInt8` … `toUInt64`, `toBigInt`, `toFloat32`, `toFloat64` | `wrapping_from_*`, `to_*`, `into` | wrapping to the width, exactly to `BigInt`, rounded once to binary32 or binary64 |
| `bitAnd`, `bitOr`, `bitXor`, `bitNot`, `shiftLeft`, `shiftRight`, `shiftRightUnsignedInt` | `Int.bit_*`, `Int.shift_*` | on 32-bit two's complement; the count is masked to 5 bits; `shift_right` keeps the sign, `shift_right_unsigned` fills with zeros and reads its bits back as an `Int` |
| `int64BitAnd` … `int64ShiftRightUnsigned` | `Int64.bit_*`, `Int64.shift_*` | the same on 64 bits, the count masked to 6 bits |

All these bindings are of `flick:rt` except the `global` ones. A `Float`
is written as JS writes a number: the shortest digits that read back, an
exponent from `1e21` and below `1e-6`, `-0`, `NaN`, `Infinity` and
`-Infinity`.

### Strings

A string is a sequence of UTF-16 code units, and every length and index
counts code units, as in JS. A scalar outside the BMP is two units; a slice
may split it, its halves are lone surrogates, and they join back into it.
NUL is a unit like any other. UTF-8 is what bytes in and out of a host
carry.

| Binding | In the standard library | What it does |
| - | - | - |
| `flick:String length` | `String.length` | the number of code units |
| `flick:String charCodeAt` | `String.char_code_at` | the unit at an index, `None` outside the string |
| `flick:String slice` | `String.slice` | JS `slice`: a negative index counts from the end, an index past the end is the end, a reversed range is empty |
| `flick:String indexOf`, `lastIndexOf` | `String.index_of`, `String.last_index_of` | the unit index of the first or last occurrence, matching units, so a lone surrogate finds half of a pair; the empty string is at `0` or at the length |
| `flick:rt bytesFromList`, `bytesToList` | `Bytes.from_list`, `Bytes.to_list` | packed bytes of a list, `Err` with the index and value of the first outside `0..255` |
| `flick:rt bytesEncodeUtf8` | `Bytes.encode_utf8` | UTF-8, a lone surrogate as U+FFFD |
| `flick:rt bytesDecodeUtf8` | `Bytes.decode_utf8` | strict UTF-8: invalid, overlong and surrogate sequences give `None`; a byte order mark stays |

With them, `++` joins strings, `==` and the comparisons go by code units,
and `show` quotes a string.

### Arrays

A `List` is an immutable array. `listLength` is its length, `listGet` the
element at an index (`None` outside, a negative index included), and
`listInitialize` builds `[f 0, …, f (n - 1)]`, calling `f` in that order;
`n <= 0` gives `[]`. Literals, `++` and patterns are IR operations.

### Mutable cells

`refMake`, `refGet` and `refSet` make, read and write a cell; `refSame` is
whether two cells are one. `refModify` is one step that no other fiber sees
half done, R7 of the
[runtime contract](https://github.com/bondiano/flick-lang/blob/main/docs/formal/runtime.md).

### Record rows

`recordGet`, `recordExtend` and `recordRemove` read, add and remove a field
by a label `recordLabel` makes; `recordProject` narrows a record to a type
with fewer fields, `recordMergeDisjoint` merges two records with disjoint
labels, and `recordLabels` lists the labels in order.

### Hooks of the built-in classes

The built-in instances of `Eq`, `Ord`, `Hash` and `Show` on primitive
values call `equals`, `compare`, `hash` and `show` of `flick:rt`:

- `equals` is SameValueZero on floats (NaN equals NaN, `-0` equals `0`)
  and compares strings by code units;
- `compare` is total: NaN is above every float, `-0` equals `0`, strings go
  by code units, not code points, and `false` comes before `true`;
- `hash` agrees with `equals`; its values are each runtime's own;
- `show` spells a value as a JS literal: a string quoted as `JSON.stringify`
  quotes it, a lone surrogate as a lowercase `\u` escape, a `BigInt` with
  its `n`, a `Float` as above, an `Ordering` by its constructor.

## Control kernel

Three primitives, which a fiber runtime is built over. Each backend
implements them its own way; the table names those of today's runtimes.

| Primitive | In the standard library | JS (`runtime/src/rt`) | Go (`runtime-go/flickrt`) |
| - | - | - | - |
| suspend a fiber, and resume it once | `Effect.never` suspends it with nothing to resume it; `Deferred` and `Fiber.join` are built on both | the `Suspend` instruction (`fiber.ts`): its start gets the functions that resume the fiber and gives back what cancels the wait | `wait` (`fiber.go`): the fiber blocks until a channel closes or it is interrupted |
| a timer | `Effect.sleep` | `clock().timer`, which gives back its cancel (`duration.ts`) | `clockOf(fb).timer`, a channel and its cancel (`concurrency.go`) |
| a turn of the host's event loop | `Effect.yield_now`, and the root runner that runs `main` to its end | `schedule` (`fiber.ts`) and `runMain` (`entry.ts`) | `runtime.Gosched` in `EffectYieldNow`, and `Main` (`platform.go`) |

The bindings are `sleep`, `flickNever` and `flickYieldNow` of `flick:Effect`.
A delay that is not positive yields; an infinite one never resumes the
fiber; interrupting a waiting fiber ends its wait and cancels its timer, so
the host's event loop does not wait for it. What is built over the control
kernel keeps the laws R1–R9 of the runtime contract.

## Conformance

- `tests/kernel` holds a case for each primitive with an expected value
  derived from the contract, never from what another backend gives. `just
  kernel` runs it on JS, on both runtimes, as a Go executable, and on
  `flick-vm`'s tier-neutral shared primitive library (`flick_vm::stdlib`), in
  debug and release; then it runs the modules of `tests/language` that
  `tests/kernel/language.txt` lists, which pin more of the numeric and
  string primitives (`just language` runs them in debug; `just kernel` runs
  them in release on JS and Go, and in both debug and release with zero-leak
  checks on `flick-vm`).
- The control kernel's cases are those of `tests/run/runtime`, which
  `just conformance` runs; `control_kernel` checks the delays, the order of
  timers, a wait that only an interruption ends and a cancelled timer.
- The `test/literal` fixture's `kernel` case is a backend with nothing but
  the kernel. It runs the pure tests of the test root in the portable
  evaluator with the kernel's primitives alone, and the test runner reports
  them. A test of `tests/kernel` that reaches a binding outside the kernel
  fails its build. The fixture skips effect tests and the tests that reach
  `listInitialize`, a cell, a record row or the control kernel, which the
  evaluator cannot run, and of the listed `tests/language` modules it runs
  only the tests that reach the kernel alone.

- The Lua backend runs the modules of `tests/kernel` and `tests/language`
  that their `lua.txt` lists with `flick test --target lua` on Lua 5.1.5
  and on LuaJIT 2.1 with the JIT on and off (`just kernel-lua`); each report
  must equal the report of the same modules on JS. The list grows as the
  Lua kernel does.

A new backend runs `flick test --target <backend>` in `tests/kernel` and the
`tests/run/runtime` cases; both must pass before it declares `flick.kernel`.
