---
title: "Values of computations and their execution"
source_hash: 20bee6b0e7cfcb83
---

## Two different kinds of functions

```text
make_request : UserId -> Eff {Users} {NotFound} User
get_user : UserId ->{Users; NotFound} User
```

`make_request` purely builds a description. `get_user` performs the operation when called, in the source semantics. Both functions may have the same TS representation `(id) => Effect<User, E, R>`, so the typed IR and module interfaces keep this distinction. It cannot be recovered from the generated TS type alone. A Sync function (`->{Atom.Read;}`, [Sync stages](../01-direct-calls/#sync-stages)) is a third kind: it performs its reads when called, yet its representation is `(id) => User`, without an Effect.

## Task and running a stored value

```flick
# type UserId = UserId String
# type alias User = { id : UserId; name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
#
load_user : UserId ->{Users; NotFound} User
let load_user user_id =
  let request = task {
    Users.get user_id
  }

  let same_request = identity request

  await same_request
```

`task` defers the body and returns an `Eff`. `identity request` merely passes the value along. `await` runs the stored computation in the current fiber and unwraps one layer: if the successful result was itself another `Eff`, it remains a value. Creating a task does not start a new fiber by itself and does not remember the result: two `await`s usually mean two executions of the body. Ordinary lexical values are captured at construction time; service lookup happens at execution time in the current environment, unless the author explicitly saved a separate service. Captured scoped values additionally restrict the lifetime of the task itself.

Starting the application is a separate external boundary. `await` inside the language does not turn into a nested `Effect.runPromise`. Initialization of a source module stays pure; IO begins at the entry point. Top-level `task` values are allowed because their bodies are deferred. A `foreign` import trusts that the external module initializes purely: ESM may execute code on import.

The evaluation order is strict, left to right, taking each currying stage into account. `f x y` means `(f x) y`: first `f` and `x` are evaluated, then the application `f x` is performed, then `y` is evaluated and the second application is performed. If the first arrow is runtime, its effect completes before the second argument is evaluated. The body of a `task` is evaluated at run time; running it again goes through its body anew. Resource sharing, caching and memoization are explicit operations, such as `Effect.cached`. Purity does not guarantee termination or the absence of a defect: moving a pure expression across a run boundary also requires a proof that behavior is preserved.

### Re-running and the value of await

```flick
# type UserId = UserId String
# type alias User = { id : UserId; name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
#
load_twice :
  UserId ->{Users; NotFound} (User, User)
let load_twice id =
  let request = task { Users.get id }
  let first = await request
  let second = await request
  (first, second)
```

The call `Users.get` happens twice. Storing `request` does not make it a cache and does not create a Promise. To obtain a result once and reuse it, write `let user = Users.get id` and then use the ready `user`. Memoizing a computation is a separate runtime operation with explicit sharing.

`await` is needed only for an already existing `Eff` value. An ordinary runtime call `let user = Users.get id` stays direct and needs no additional `await`.

## Purity is separate from error and service rows

The checker tells a pure expression from a runtime computation independently of the rows: empty `r` and `e` do not make a runtime computation pure.

`none` denotes an empty row of requirements or errors. The absence of services and typed failures does not rule out IO, state, a defect or interruption. `provide` supplies a dependency but does not erase the very fact of execution through the runtime.

## Unit, Never and empty rows

`Unit` is an ordinary type with the single value `()`: the operation completed, with no useful result. `Never` is an ordinary type with no values. A separate source-level `Void` is not introduced. `none` is a literal for an empty row, not a result type and not the constructor `Option.None`.

| Notation | Meaning |
| - | - |
| `a -> Unit` | A pure call with the result `()` |
| `Unit ->{} a` | Takes `()`, runs through the runtime, requires no services and has no typed errors |
| `a ->{Users; none} Unit` | A runtime call requires `Users`, has no typed errors and returns `()` |
| `a ->{none; FsError} b` | A runtime call without services, with the error `FsError` |
| `a ->{; FsError} b` | The same: an omitted side of the arrow is an empty row |
| `Eff none none a` | A stored runtime computation with two empty rows |

In an arrow, either side may be omitted: `->{; E}` equals `->{none; E}`, and `->{R;}` equals `->{R; none}`. The separator `;` is mandatory, so `->{E}` is a syntax error rather than a guess about which row `E` belongs to. `->{}` is shorthand for a runtime arrow with two empty rows; it does not reduce to a pure `->`. `Eff Unit Unit a` is rejected by kinds. In the internal JS/TS representation, `Unit` corresponds to `undefined`, and empty `E` and `R` correspond to `never`. The public TS API may use `void` for an ignored result; on entering the language, an adapter normalizes such a result to `()`.

In [TypeScript](https://www.typescriptlang.org/docs/handbook/2/functions.html), `void` and `undefined` are distinct. In [Effect](https://effect.website/docs/v4/getting-started/the-effect-type), the absence of expected errors and requirements is denoted by `never` in the corresponding parameters.

This is consistent with Effect: its requirements reflect contextual dependencies; some services are provided by default and do not appear in the user-visible `R`. [The Effect type](https://effect.website/docs/v4/getting-started/the-effect-type), [default services](https://effect.website/docs/v4/requirements-management/default-services).

## Example of concurrent execution

```flick
# type UserId = UserId String
# type alias User = { id : UserId; name : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
#
load_name_pair : UserId -> UserId ->{Users; NotFound} (String, String)
let load_name_pair first_id second_id =
  let first = task { Users.get first_id }
  let second = task { Users.get second_id }

  let (first_user, second_user) =
    parallel first second

  (first_user.name, second_user.name)
```

In the direct style, `parallel (Users.get first_id) (Users.get second_id)` would first evaluate both arguments. Deferred blocks let the `parallel` operation itself control when execution begins.

The contract: `task` is a description, `await` is execution in the current fiber, and the entry point is the external run boundary. Running a nested `Eff` requires the next explicit `await`; there is no automatic recursive flatten.
