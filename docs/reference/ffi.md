# Foreign functions

A `foreign` declaration binds a JS function to a Flick signature (§8). The
signature is the contract; the mode says how the call runs, and metadata
lines below the declaration adapt the call.

```flick
hypot : Number -> Number -> Number
foreign pure hypot from global "Math.hypot"
  arity 2

now_millis : Unit ->{} Number
foreign sync now_millis from global "Date.now"
  arity 0

read_text : Path ->{; FsError} String
foreign async read_text from "./fs.js" "readText"
  arity 1
  catch FsError.from_js
  cancel signal
```

## Modes

| Mode | The call |
| - | - |
| `pure` | a direct JS call: trusted to be pure, also at module load |
| `sync` | runs when the runtime stage runs; a throw is a defect |
| `async` | the promise is created when the call runs, not when a `task` is built; a rejection is a defect, and the innermost owner waits for the promise to settle even after an interruption |
| `effect` | the JS factory is called when the call runs (`Effect.suspend`), and its `Effect` runs in the current fiber |

## Metadata

| Metadata | Meaning |
| - | - |
| `arity n` | `n` positional JS arguments, adapting `n` stages; `arity 0` takes the `()` of a first `Unit` parameter and calls JS without arguments, after evaluating that argument |
| `this` | the first argument is the receiver: `target.call(a0, a1, …)` |
| `catch f` | a throw or rejection becomes the typed error `f value`, with `f : JsValue -> E` and `E` in the error row; `E` may hold no capability of a scope (`FLICK-F003`) |
| `cancel signal` | `foreign async` only: an `AbortSignal` is passed last and aborted on interruption; the owner still waits for the promise |
| `nullable` | the result is `Option a`: `null` and `undefined` are `None` |
| `optional i` | argument `i` (from 0) is `Option a`: `None` is passed as `undefined` |
| `variadic` | the last argument is a `List a` spread into the call |
| `retention call` / `retention owner` | what JS keeps of an argument that may hold a scope's capabilities: nothing past the call, or until the owner of the region closes |
| `reviewed "<pkg>@<ver> sha256:…"` | the binding was checked against that declaration of a package export; `flick ffi check` compares it with the installed one |

A metadata line that does not fit the signature or the mode is `FLICK-F001`.
A result of type `Unit` is `()` whatever JS returns. A parameter whose type
may hold a scope's capabilities — a region, as in `File s`, or a capture
qualifier `^c` — needs a `retention` line (`FLICK-F002`): without one the
compiler cannot know how long JS keeps it. A bare callback type holds no
captures at all, so a closure over a scoped value is refused where it is
passed (`FLICK-G001`).

## Callbacks

A Flick function passed to JS is adapted to the JS arity of its type:
`(a -> b -> c)` arrives as `(a, b) => c`. There are four families:

| Family | JS gets | Runs |
| - | - | - |
| pure | `(a, b) => c` | directly, on each call |
| Effect | `(a) => Effect<b, E, R>` for a runtime callback | when the receiving Effect code runs it, in its fiber and environment |
| Promise | `Js.promise_callback f`: `(a) => Promise<b>` | as a child of the scope where it was made, with the services there; calls after that scope began to close are rejected, and the scope waits for running calls |
| scoped event | `Events.listen owner policy supervisor handler` | each call queues an event; one worker, a child of `owner`, runs the handler task by task |

`Events.attach listener add remove` registers a listener and, when the
owner of its region begins to close, ignores later events at once and calls
`remove` with the very function `add` got; then the worker ends and the
resources are released. An event that finds the queue full
(`Events.queue capacity`) is dropped and reported to the supervisor, as is
every failure of the handler. The handler never runs inside the event
source's call.

## JS values

`JsValue` is any JS value as foreign code gives it. `Js.snapshot` copies
supported data — primitives, plain objects, dense arrays — into an
`Unknown`, frozen, at most 64 deep and 100000 values (`Js.snapshot_with`
sets both). It reads properties through their descriptors, so a getter never
runs; a getter, a function, a cycle, an object of another prototype, a hole
in an array, a throwing proxy or a value past the limits is a
`SnapshotError` naming the path. A subtree shared by two places is copied
once.

## Importing a package's `.d.ts`

`flick ffi import <package> --out <dir>` reads the declarations of an
installed package with TypeScript (through the Node helper
`@flick-lang/dts-importer`, or the script `FLICK_DTS_IMPORTER` names) and
writes `<dir>/<Pkg>/Generated.fl`, module `<Dir>.<Pkg>.Generated`
(`date-fns` → `DateFns`), with a report of every selected export.
`--select a,b` imports only those exports. Each export gets one status:

| Status | Generated |
| - | - |
| supported | a `foreign` binding, `/// <pkg>@<ver> sha256:…` above it |
| needs policy | `foreign blocked name "needs policy: …"`: the `.d.ts` cannot state the contract |
| unsupported | `foreign blocked name "unsupported: …"`: outside the subset |

Using a `foreign blocked` value is `FLICK-F005`; the other exports of the
package stay usable, and asking for a blocked export with `--select` is an
error too. Nothing is inferred that a declaration cannot show: a call is
`sync` (`Unit ->{} Number` for `nextId(): number`), never `pure`.

| TypeScript | Flick |
| - | - |
| `string`, `number`, `boolean`, `bigint` | `String`, `Number`, `Bool`, `BigInt` |
| `void`, `undefined` result | `Unit` |
| `any`, `unknown` | `JsValue`; refused with `strict = true` |
| `T[]`, `[A, B]` | `List T`, `(A, B)` |
| `x?: T`, `x: T \| undefined` | `Option T` with `optional i` |
| `...xs: T[]` | `List T` with `variadic` |
| unconstrained `<T>` | `forall t.`, if `T` appears in a parameter |
| a class instance, `Date`, a named object type | an opaque `type` of the generated module |
| a callback `(a: A) => B` | a pure `(A -> B)`; needs `retention` |
| `Promise<T>` result | `foreign async`; needs `catch` |
| `T \| undefined` or `T \| null` result | `Option T` with `nullable`; needs `undefined = "none"` |
| overloads with different numbers of parameters | one binding each, `name_<n>` |

Unions, literal types, intersections, bounded or result-only type
parameters, `this` parameters, `null` arguments, object arguments of
unnamed shape and functions returned are unsupported, each with its reason.
A trailing optional parameter of such a type is left out, with a note.

Policies sit in `<dir>/<Pkg>/policy.toml`, which the import reads and never
writes:

```toml
strict = false             # true: `any` and `unknown` are refused

[exports.lookup]
undefined = "none"         # an undefined or null result is None
catch = "JsError"          # a throw or rejection becomes JsError
cancel = "signal"          # an async call gets an AbortSignal
# mode = "pure"            # trust the call to be pure
# retention = "call"       # or "owner": how long JS keeps a callback
```

For `export declare function lookup(id: string): Promise<string | undefined>`
this policy gives:

```flick
lookup : String ->{; JsError} Option String
foreign async lookup from "lookup-lib" "lookup"
  arity 1
  catch JsError.from_js
  nullable
```

`JsError.from_js` keeps what a thrown value says — `name: message` of an
`Error` — never the value.

### Reviewed bindings and `ffi check`

The import writes only `Generated.fl`. A binding you check by hand goes in
a module of your own, usually `<dir>/<Pkg>.fl` re-exporting the rest, with
`reviewed` naming the declaration it was checked against — the line the
generated module shows above the export:

```flick
lookup : String ->{; JsError} Option String
foreign async lookup from "lookup-lib" "lookup"
  arity 1
  catch JsError.from_js
  nullable
  reviewed "lookup-lib@1.0.0 sha256:6fa1…"
```

`flick ffi check` (the project's source root, or the paths given) imports
each package again and reports a generated module that differs from the
new import (`FLICK-F006`), a reviewed binding whose declaration changed or
disappeared, or that lacks what its shape needs — `catch` for a Promise,
`nullable` for an empty result, `retention` for a callback (`FLICK-F007`)
— and a policy for an export the package does not have (`FLICK-F008`).
