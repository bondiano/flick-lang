# Runtime adapter

`@flick-lang/runtime` is what compiled code runs on (§7, §11). It is a thin
adapter over Effect `4.0.5`: generated modules import only this package, and
the adapter fixes the contracts below so that they hold whatever Effect does
by default.

## Effect release

The adapter was verified against exactly Effect `4.0.5`. The entry runner
checks the installed release and a capability matrix of the Effect
operations it relies on before `main` starts; another release, or a
missing operation, ends the run with exit code `2`:

```text
error: the Flick runtime requires effect 4.0.5, found 4.1.0
```

## Owners and scopes

Every `scope` has a lexical owner, and the entry runner has the root owner.
An owner holds a group of children, the foreign work they started, and the
resource finalizers of what was acquired in it. When the body ends —
success, typed error, defect or interruption — the owner closes in two
phases:

1. it admits no new children;
2. it interrupts every child and waits until each has ended, supervisors
   included; an uninterruptible child delays the close;
3. it waits for every pending foreign `async` promise to settle, also one
   that ignores interruption;
4. it runs resource finalizers, the last acquired first;
5. the scope ends with the body's exit, joined with any cleanup failure.

Children and resources are separate groups: a resource acquired after a
child started is still released after that child ended.

`acquireRelease` registers a finalizer without typed errors with the
innermost owner; `bracket` releases right after its use and may fail with a
typed error of its own.

## Children

`Fiber.spawn owner supervisor task` starts `task` as a child of `owner`. A
child that fails, including one interrupted by its closing owner, passes its
`Failure` to the supervisor; `failure.shutdown` tells the interruption of a
closing owner apart. A defect of the supervisor goes to the root reporter
and makes the run fail. `Fiber.join` waits for a child and fails with its
original cause. A closing owner admits no new children: spawning into it is
a defect.

## Concurrency

| Operation | Contract |
| - | - |
| `parallel a b` | both results, in input order |
| `parallel_all limit effects` | at most `limit` branches at a time; `limit` must be a positive finite integer, otherwise a defect |
| `parallel_results effects` | each branch's typed failure as an `Err`; a defect or an interruption stops the group |
| `parallel_exits effects` | the opaque `Exit` of every branch |

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
layer, and two `provide`s build it twice. The body sees the services of its
surroundings besides those of the layer, which are gone after it.
Finalizers of the layer run when the body has ended, with the services they
were registered with.

## Entry runner

`main.js` runs `main` under the root owner. SIGINT and SIGTERM interrupt
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
came at the same time.
