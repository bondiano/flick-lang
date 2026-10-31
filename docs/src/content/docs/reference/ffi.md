---
title: "Foreign functions"
---

A `foreign` declaration binds a JS function to a Flick signature (see [TypeScript generation and the FFI boundary](../../spec/08-typescript-and-ffi/)). The
signature is the contract; the mode says how the call runs, and metadata
lines below the declaration adapt the call.

```flick
hypot : Float -> Float -> Float
foreign pure hypot from global "Math.hypot"
  arity 2

now_millis : Unit ->{} Float
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
`(a -> b -> c)` arrives as `(a, b) => c`. Only an argument of the binding is adapted: a function
inside a record, a list or an `Option` arrives curried, as Flick holds it. There are four families:

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

Flick data crosses a binding outside `@flick-lang/runtime` as a copy: a
`List`, tuple, record or constructed value in an argument, the result, the
value of a `Promise` or `Effect`, and what a callback is given or returns.
JS may sort the array it got in place or change an object it returned
earlier; the Flick value stays as it was. Primitives, `JsValue`, opaque types
and functions pass as they are, and a callback without data keeps its
identity.

## The go target

On the go target a binding to a module of the program — a relative path,
`./timers.mjs` — calls the Go function of the same name in `timers.go`
beside it, which the build copies into the Go module; without that file the
binding is `FLICK-P012`, naming the file to add. The file is `package main`
and imports `flick.app/flickrt`. Its functions take and return
`flickrt.Value`, in the representation of the go target (see
[Code generation](../codegen/#go-target)), and take every argument of the
binding at once; the `()` of `arity 0` adds no Flick argument:

| Mode | The Go function |
| - | - |
| `pure`, `sync` | `f(fb *flickrt.Fiber, a0, a1, …)` |
| `effect` | `f(fb *flickrt.Fiber, a0, …)`: it runs in the calling fiber, and acquires into the scope around the call with `flickrt.AcquireRelease` |
| `async` | `f(ctx flickrt.Context, a0, …)` in a goroutine of its own: `ctx` is cancelled on interruption, and the innermost owner waits for the call to return before it releases resources, as it waits for a promise |

Pure calls also receive the executing fiber: context-sensitive rendering must
not retain the fiber that created a function. `flickrt.Fn` takes
`(*flickrt.Fiber, flickrt.Value)`, and `flickrt.Poly` takes
`(*flickrt.Fiber, ...flickrt.Value)`; both return `flickrt.Value`.
Pass the invocation's fiber to `flickrt.Apply(fb, f, arg)` and
`flickrt.Inst(fb, f, dictionaries...)`. `nil` is only for module initialization
or immutable schema metadata outside program execution. This hidden context
does not make a pure Flick stage effectful.

A panic is a defect. `catch`, `this`, `optional`, `variadic` and adapted
callbacks are `FLICK-P012` on the go target for now. The Go representation
of values is not a stable interface yet: a Go file follows the compiler it
is built with.

## Importing a package's `.d.ts`

`flick ffi import <package> --out <dir>` reads the declarations of an
installed package with TypeScript (through the Node helper
`@flick-lang/dts-importer`, or the script `FLICK_DTS_IMPORTER` names) and
writes `<dir>/<Pkg>/Generated.fl`, module `<Dir>.<Pkg>.Generated`
(`date-fns` → `DateFns`), with a report of every selected export.
Package subpaths and npm aliases remain the exact import specifier supplied
to the command; resolved package identity supplies version provenance, not
a replacement import path.
`--select a,b` imports only those exports. Each export gets one status:

| Status | Generated |
| - | - |
| supported | a `foreign` binding, `/// <pkg>@<ver> sha256:…` above it |
| needs policy | `foreign blocked name "needs policy: …"`: the `.d.ts` cannot state the contract |
| unsupported | `foreign blocked name "unsupported: …"`: outside the subset |

Using a `foreign blocked` value is `FLICK-F005`; the other exports of the
package stay usable, and asking for a blocked export with `--select` is an
error too. Nothing is inferred that a declaration cannot show: a call is
`sync` (`Unit ->{} Float` for `nextId(): number`), never `pure`.

| TypeScript | Flick |
| - | - |
| `string`, `number`, `boolean`, `bigint` | `String`, `Float`, `Bool`, `BigInt` |
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
unnamed shape, functions returned, the class itself (`typeof C`) and a type
name the package declares twice (two `Options` in different files) are
unsupported, each with its reason.
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
