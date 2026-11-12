---
title: "Runtime and Effect adapter"
---

By default, compiled code imports `@flick-lang/runtime/rt`, Flick's own runtime
without an Effect dependency. The `--rt` flag also selects it for one build,
run or test of a project configured for Effect.

An RT project that uses a library binding still implemented by Effect imports
`@flick-lang/effect-ts/bridge` for that binding. Add `@flick-lang/effect-ts`
when using such modules. The bridge converts callbacks, effects, services,
scopes, Fibers, Schedules, Durations, collections, schema witnesses and codecs
across the boundary, including codecs with runtime effects. Nested `Option`
and `Result` values retain their meaning in generic library calls. A project
that uses only RT bindings does not load the bridge.

`Schema.number` validates runtime Floats, including non-finite values nested
in lists, records or variants. Its JSON codec still rejects non-finite numbers.

Opaque Stream values stay in the adapter. Native atoms, serializable atoms and
renderers remain on RT, including `Ui.Server.stream`, which renders on RT and
hands its markup to the adapter's Stream, and `Atom.query_stream`, whose atom
lives on RT while the adapter runs its Stream. If a used UI binding needs the
adapter (such as explicit action spans),
atoms, forms, routes and renderers use their matching Effect implementations
together for the build.
Tracing bindings share Effect implementations with their configured exporters.

`Rpc` procedures and groups, and their unary calls over HTTP (`Rpc.call` and
`Rpc.call_with` in `Rpc.ndjson` or `Rpc.json`), run on RT and speak the
protocol of Effect's RPC server: a browser client takes no Effect, and calls
a server built with the adapter. Serving, stream calls, `Rpc.websocket`,
`Rpc.tcp`, the `json_rpc`, `ndjson_rpc` and `binary` serializations,
`Rpc.connect`, `Rpc.with_headers` and in-memory calls stay in the adapter,
as server functions and the rest of Http do for now. `Http.Headers`,
`Http.Cookies`, `Http.Url`, `Http.UrlParams`, `Http.Mime` and `Http.Etag` run
on RT, with the encodings, orders and messages of Effect's modules. Http, RPC and server
functions are one family: a build that uses a binding of it only the adapter
has takes the whole family from the adapter, so a request, a response or a
procedure belongs to one implementation.

Reading request bodies, persisting multipart forms, serving static files and
consuming a server-rendered stream require `Async` in the caller's effect row.
For an HTTP body whose stream requires only `Async`, use `Http.Body.stream_async`
or `Http.ServerResponse.stream_async` / `stream_text_async`.

Projects with `effect = "effect"` use `@flick-lang/effect-ts` (see [Errors,
services, resources and concurrency](../../spec/05-errors-and-concurrency/),
[Compiler architecture](../../contributing/implementation-decisions/#compiler-architecture)).
This adapter targets Effect `4.1.2` and preserves the contracts below.

## Effect release

The adapter was verified against exactly Effect `4.1.2`. The entry runner
checks the installed release before `main` starts; another release ends
the run with exit code `2`. The capability matrix of the Effect operations
the adapter relies on is checked by the runtime's tests and by
`just effect-bump`, not at run time: it needs every Effect module, which
would keep all of Effect in a browser bundle.

```text
error: the Flick Effect adapter requires effect 4.1.2, found 4.1.0
```

## Owners and scopes

Every `scope` has a lexical owner, and the entry runner has the root owner.
An owner holds a group of children, the foreign work they started, and the
resource finalizers of what was acquired in it. When the body ends —
success, typed error, defect or interruption — the owner closes in two
phases:

1. it admits no new children and detaches external event sources;
2. it interrupts every child and waits until each has ended, supervisors
   included; an uninterruptible child delays the close;
3. it waits for every pending foreign `async` promise to settle, also one
   that ignores interruption;
4. it runs resource finalizers, the last acquired first;
5. the scope ends with the body's exit, joined with any cleanup failure.

Detachment failures are retained alongside body and finalizer failures;
they do not skip interruption, joining or resource cleanup.

Children and resources are separate groups: a resource acquired after a
child started is still released after that child ended.

`acquireRelease` registers a finalizer without typed errors with the owner
the call runs in: the innermost one, or the owner of an outer `scope` the
compiler passes with `within` when the call acquires into that scope's
region; `bracket` releases right after its use and may fail with a
typed error of its own.

## Children

`Fiber.spawn owner supervisor task` starts `task` as a child of `owner`. A
child that fails, including one interrupted by its closing owner, passes its
`Failure` to the supervisor; `failure.shutdown` tells the interruption of a
closing owner apart. A defect of the supervisor goes to the root reporter
and makes the run fail; under `flick test` it fails the test it happened
in. `Fiber.join` waits for a child and fails with its
original cause. A closing owner admits no new children: spawning into it is
a defect. The child and its supervisor acquire into the owner's region, so a
task that acquires names `Scope` of that region.

## Concurrency

| Operation | Contract |
| - | - |
| `parallel a b` | both results, in input order |
| `parallel_all limit effects` | at most `limit` branches at a time; `limit` must be a positive finite integer, otherwise a defect |
| `parallel_results effects` | each branch's typed failure as an `Err`; a defect or an interruption stops the group |
| `parallel_exits effects` | the opaque `Exit` of every branch |

Programs reach them through the Prelude, and outcomes through the modules
`Exit`, `Failure` and `RawCause`:

```text
parallel : (Eff r e a)^c -> ((Eff r e b)^c ->{r; e} (a, b))^c
parallel_all : Float -> List ((Eff r e a)^c) ->{r; e} List a
parallel_results : List ((Eff r e a)^c) ->{r; none} List (Result (Errors e) a)
parallel_exits : List ((Eff r e a)^c) ->{r; none} List (Exit e a)
race_success : (Eff r e a)^c -> ((Eff r e a)^c ->{r; e} a)^c
race_first : (Eff r e a)^c -> ((Eff r e a)^c ->{r; e} a)^c
timeout : Float -> (Eff r e a)^c ->{r; TimeoutError | e} a
timeout_option : Float -> (Eff r e a)^c ->{r; e} Option a

Exit.value : Exit e a -> Option a
Exit.failure : Exit e a -> Option (Failure e)
Failure.primary : Failure e -> Option (Errors e)
Failure.raw_cause : Failure e -> RawCause
Failure.inspect : Failure e ->{} CauseReport
Failure.rethrow : Failure e ->{; e} a
RawCause.inspect : RawCause ->{} CauseReport
```

A task may hold what the caller's scopes own (`^c`): the combinators run it
before they return.

On a failure no new branch starts, the running branches are interrupted and
awaited, and the result keeps the failures observed so far.

| Race | Contract |
| - | - |
| `race_success a b` | the first success; failures of the loser are not added |
| `race_first a b` | the first branch to end, however it ends |
| `timeout millis e` | fails with `TimeoutError` when time runs out first |
| `timeout_option millis e` | `None` when time runs out first; other failures stay |

Losers are interrupted and awaited before the result is returned. A branch
that is ready at once may win before the other one starts at all.

## Outcomes

`try … with` handles the first typed failure of a cause; a cause that also
holds a defect is not recovered and its failures become defects, and a cause
that is only interruption has no error. Without a matching arm the original
cause propagates unchanged:

| Cause | Recovery |
| - | - |
| `Fail(A)` | `A` |
| `Fail(A), Fail(B)` | `A`, the first |
| `Fail(A), Interrupt` | `A` |
| `Fail(A), Die(D)` | none: the failures become defects |
| `Interrupt` | none |
| `Fail(B), Fail(A)`, arms for `A` only | none: the cause propagates |

`Exit e a`, `Failure e` and `RawCause` are opaque. `Failure.primary` is the
error recovery would select; `Failure.rethrow` fails with the original cause;
`Failure.inspect` and `RawCause.inspect` give a `CauseReport`: frozen data
without a prototype, listing each reason's kind (`fail`, `die`,
`interrupt`), a `_tag` or an error message read from own data properties,
and otherwise a placeholder. Building it never runs code of a payload —
getters, `toString`, `toJSON` or Proxy traps.

## Layers

`provide layer { body }` is a lexical owner too. The layer is built before
the body; if building fails, the body never runs. One build shares each
layer, and so does a `provide` nested in the body: it takes the outer
build of the same layer value. Two `provide`s side by side build it twice.
The body sees the services of its
surroundings besides those of the layer, which are gone after it.
Finalizers of the layer run when the body has ended, with the services they
were registered with.

## Entry runner

`flick-main.js` runs `main` under the root owner. SIGINT and SIGTERM interrupt
`main` and close the root owner by the same protocol; the process keeps
running until cleanup has finished, and reports on stderr when that takes
longer than two seconds (`FLICK_SHUTDOWN_NOTICE_MS` sets the delay):

```text
shutting down: waiting for cleanup to finish
```

| Exit code | When |
| - | - |
| `0` | `main` succeeded and cleanup too |
| `1` | a typed failure (`error: <message>` for `AppError`), a defect (its cause is printed), a failed cleanup, a defect of a supervisor, or an interruption not caused by a signal |
| `2` | the installed Effect is not the one the runtime supports |
| `130` / `143` | only SIGINT / SIGTERM stopped `main`, and cleanup succeeded |

A failure of the application or of cleanup keeps code `1` even when a signal
came at the same time. A printed cause lists each failure and defect: an
`Error` by its stack up to the first frame of the Effect runloop, another
value by its `message` or as JSON.
