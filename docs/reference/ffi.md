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
