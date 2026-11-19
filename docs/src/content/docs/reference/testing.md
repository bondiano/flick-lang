---
title: "Testing"
---

A module of the test root holds its tests as blocks: `suite "name" { … }`
groups `test "name" { … }` blocks and other suites, and a `test` block may
also stand alone. `flick test` finds the module, builds it and runs each
test in a scope of its own.

```flick
module App.UsersTest

suite "names" {
  test "trims the name" {
    let name = String.trim "  Ada  "
    Test.expect (Test.equal "Ada" name)
  }

  suite "empty" {
    test "stays empty" { Test.expect (Test.equal "" (String.trim "  ")) }
  }
}

test "counts" { Test.expect (Test.equal 3 (List.length [1, 2, 3])) }
```

The blocks are the plugin of the `std` package (see [plugins](../plugins/)),
which every module has without an import. They expand into calls of `Test`:
a suite is `Test.group`, a test `Test.effect_test name (task { body })`, as
one is written by hand, so `flick expand` shows the code each block stands
for.

## Blocks

A block starts a top-level declaration, as `suite` and `test` are heads only
there: elsewhere the words are names, and `Test.test "x" (fun () -> …)` is a
call. A top-level declaration may not start with a value named `suite` or
`test`, but a signature `test : T` still declares one.

| Block | What it holds |
| - | - |
| `suite "name" { … }` | `test` and `suite` blocks, a case a line, in the order they run |
| `test "name" { body }` | the test's body: lines, as after the `=` of a `let`, or one expression; `{}` for a test with nothing to check yet |

A test's body is an effectful test, as `Test.effect_test` runs one: it runs
in a scope of its own, which owns what the body acquires, with the live host
services (`Host.Live`), which outlive that scope. It passes when it ends and
its scope closes cleanly; `Test.expect` fails it on a failed assertion, and
a typed failure, a defect or a failed cleanup fails it too.

The cases of a module are its `tests : List Test.Test`, the case of each
top-level block in the order written. The module exports `tests` without
naming it in `exposing`, so a module with blocks needs no `exposing` at all.
A module that has blocks and declares `tests` by hand is `FLICK-R008`: move
the hand-written tests into a block, or drop the blocks.

```flick fail R008
module App.MixedTest

tests : List Test.Test
let tests = [Test.test "by hand" (fun () -> Test.Pass)]

test "a block" {}
```

A module without blocks builds `tests` by hand with `Test` as before (see
[the standard library](../stdlib/#tests)): both kinds of module run in the
same `flick test`.

## Test companions

A module of the test root sees only what the modules it imports export. To
test what a module keeps to itself, write its tests in a companion beside
it: `src/App/Users.test.fl` next to `src/App/Users.fl`. The companion
begins with the module's own header, without `exposing`, and sees every
top-level declaration of the module, private values, types and
constructors included, without an import:

```flick
module App.Users

# trim : String -> String
# let trim name = String.trim name
#
test "trims a name" { Test.expect (Test.equal "Ada" (trim "  Ada ")) }
```

The module never sees its companion, and no other module can import it.
Its cases are named after the module, `App.Users > trims a name`, and run in
the same `flick test` as those of the test root. Only `flick test` and
`flick fmt` read companions, and `flick check` one that a path names:
`flick build`, any other `flick check`, the published `.iface` and `.d.ts`,
the API docs and `flick pack` leave them out, so the output of a build is the
same with or without them. Under `flick test`, the
code of a module with a companion exports to it all it declares, and the
companion's view of the module is cached apart from the module's public
interface. A companion of a file that is not a module of the source root,
or with another module's header, is an error.

## Full names, skip and only

A case's full name is its module's name, its suites' names and its own:
`App.UsersTest > names > trims the name`. `--filter` keeps the cases whose
full name contains a text, and two cases of one suite, or two blocks of one
module, with the same name are `std/T002`.

A modifier after the word marks a case and every case inside it:

| Modifier | Meaning |
| - | - |
| `test.skip`, `suite.skip` | reported as skipped, never run |
| `test.only`, `suite.only` | while a case of the selected files is marked `only`, the run keeps only such cases and reports the others as skipped |

```flick
module App.WipTest

suite.skip "storage" {
  test "keeps a user" {}
}

test.only "being written" { Test.expect (Test.equal 2 (1 + 1)) }
```

A skipped case inside a suite marked `only` stays skipped. `Test.skip` and
`Test.only` mark a test built by hand the same way.

## Hooks

A suite's hooks run around the cases of the suite that the run selects, its
nested suites' included:

| Hook | Runs |
| - | - |
| `before_all { … }` | once, in the suite's scope, before its first selected case |
| `before_each { … }` | in each selected case's scope, before the body |
| `after_each { … }` | after each selected case, as a finalizer of the case's scope |
| `after_all { … }` | once, after the suite's last selected case, as a finalizer of the suite's scope |

```flick
module App.StoreTest

suite "store" {
  before_all { Platform.stdout_line "open the store" }
  before_each { Platform.stdout_line "begin" }
  after_each { Platform.stdout_line "roll back" }
  after_all { Platform.stdout_line "close the store" }

  test "keeps a user" {}

  suite "names" {
    before_each { Platform.stdout_line "load the names" }
    test "trims" {}
  }
}
```

Setup runs outer to inner and cleanup inner to outer: for `store > names >
trims`, `open the store`, then `begin`, `load the names`, the body and
`roll back`; `close the store` comes after the suite's last case. A suite
with no selected case, a skipped case and a filtered-out case run no hooks.
Hooks of one kind run in the order written, and their cleanup in reverse.

A failed `before_all` reports each selected case of its suite as a `setup`
failure and runs none of them; a failed `before_each` fails its case's
`setup` and skips the body. Cleanup still runs: the `after_*` hooks of the
suite and of those around it, as do the finalizers of the case's scope,
also after a timeout or an interruption. A failed `after_*` hook is the
`cleanup` failure of the case, next to its own. A hook's body is a task, as
a test's is, and `Test.before_all` and the others add a hook to a group
built by hand.

## Timeouts

`timeout <millis>` after a case's name gives it that many milliseconds for
its `before_each` hooks and its body, and after a suite's name gives them to
each case inside without its own. A case out of time is interrupted, its
scope closes, its `after_*` hooks run, and it is reported as `timeout`.
`--timeout` gives the cases without one theirs; by default a case has no
time limit.

A suite's hooks have its time too, its own `timeout` or the one around it,
the run's `--timeout` included: each `before_all`, `after_each` and
`after_all` hook, which is part of no case's time. A `before_all` out of
time reports each selected case of the suite as `setup` (`before_all timed
out after 50 ms`), and the suite's `after_*` hooks still run; an `after_*`
hook out of time is the `cleanup` failure of its case, or of the suite's
last case, and the run goes on. A hung hook therefore cannot hang a run
that has a timeout.

```flick
module App.SlowTest

suite "network" timeout 5000 {
  test "answers" timeout 500 {}
  test "retries" {}
}
```

`Test.timeout` does the same for a test built by hand.

## Fixtures

`fixture name = expression` in a suite is a value its cases and hooks read
with `await name`. Each case that uses it gets its own, made the first time
its code awaits it; `fixture shared name = expression` is made once for the
run of the suite, the first time one of its cases or hooks awaits it. A
fixture no code uses is never made, and one is made at most once in its
scope: it is a `Lazy` (see [effects](../effects/#lazy)).

```flick
module App.UsersTest

suite "users" {
  fixture shared names = SynchronizedRef.make ["Ada"]
  fixture count = List.length (SynchronizedRef.get (await names))

  before_all { SynchronizedRef.update (fun all -> all ++ ["Grace"]) (await names) }

  test "counts the names" { Test.expect (Test.equal 2 (await count)) }
  test "reads them" { Test.expect (Test.equal ["Ada", "Grace"] (SynchronizedRef.get (await names))) }
}
```

A fixture's expression is a task, as a test's body is, and may await the
fixtures declared before it, those of the suites around included; it cannot
use itself or a later one, so fixtures cannot depend on each other in a
cycle (`std/T003`). A shared fixture is made for no case in particular, so
it may use only shared fixtures, and so may the `before_all` and
`after_all` hooks.

The expression runs in a scope of its own, which owns what it acquires. A
case's fixtures are released when the case ends, after its `after_each`
hooks, which may still use them; shared ones after the suite's last
selected case and its `after_all` hooks. Each scope releases its fixtures
in the reverse order they were made in, also after a later one failed to be
made. `|> Fixture.cleanup release` releases a value then, and
`Resource.own` closes a value of `Close` then, seeing how the case ended:
whether its body passed, failed, ran out of time or never ran after a failed
setup. A transaction rolls back that way when its case fails:

```flick
module App.AccountsTest

import Resource exposing (Close(..), own)

say : String ->{} Unit
let say line = Host.raw_stdout_line line

/// A transaction: it commits when its case passes and rolls back otherwise.
type Tx = Tx String

instance Close Tx =
  let close _ (Tx name) = say ("roll back " ++ name)
  let close_exit _ exit (Tx name) =
    say ((if Exit.is_success exit then "commit " else "roll back ") ++ name)

suite "accounts" {
  fixture tx = own (Tx "accounts")

  test "opens one" { ignore (await tx) }
}
```

A fixture whose expression fails fails each case that awaits it, with its
error; a shared one is not made again for the next case.

A suite with a shared fixture runs its cases one at a time, also under
`--jobs`, while other suites run beside it. `suite parallel "name"` runs
its cases at once and shares only fixtures marked `fixture shared
concurrent`, whose author states that its cases may use them at once, as a
pool or read-only data: another shared fixture of a `parallel` suite is
`std/T004`. A `parallel` suite inside one that runs one case at a time still
runs its cases one at a time. A case's own fixtures are never shared, so a
per-case transaction over a shared database stays isolated.

A fixture's value holds no region: the types do not stop a case from
keeping it past its end, in a shared fixture or a fiber that outlives the
case, after which it may be released. That is the guarantee of
`Scope.add_finalizer`, not of `Resource.own`. Qualifying the value with the
region of the code that awaits it, as `own` qualifies its value, would turn
away most uses: a function that takes a value without a `^c` rejects one
that holds a region, so even `SynchronizedRef.get (await names)` would be
`FLICK-G001`.

`Test.fixture`, `Test.shared` and `Test.share` are what the blocks expand
into: `flick expand` shows them.

## A case for each row

`test.each rows "name" (pattern) { body }` is a case for each row of
`rows`, a list: the pattern takes the row apart for the body, and the case
is named `name [0]`, `name [1]` and so on, or, when the name has `{0}` to
`{n}`, with each replaced by that part of the row as `show` writes it, the
parts being those of the pattern's tuple. The cases stand where the
`test.each` does, beside the other cases of the suite, and each has its own
scope, fixtures and full name, which `--filter` and `--list` see.

```flick
module App.MathTest

rows : List (Int, Int)
let rows = [(1, 2), (2, 3)]

suite "math" {
  test.each [(1, 2), (2, 3)] "adds one to {0} to get {1}" (a, b) {
    Test.expect (Test.equal b (a + 1))
  }

  test.each rows "is ordered" ((a, b)) { Test.expect (Test.is_true "ordered" (a < b)) }
}
```

The rows are a list, a name or an expression in parentheses, built once,
when the module's tests are: a pure value, so the cases a run selects do not
depend on effects. Rows that need effects belong in a `before_all`, or in a
property test.

## Running

`flick test` builds the project with its test root into `.flick/test/` and
runs the selected cases, on Node, on Flick's own runtime (`--rt`) or as a Go
executable (`--emit go`), with the same names and reports on each.

| Option | Meaning |
| - | - |
| `--filter <text>` | only the cases whose full name contains `text` |
| `--list` | print the full name of each case the run would run, a line each, and run none |
| `--forbid-only` | fail with exit code `1`, before running any case, when a case of the run is marked `only` |
| `--jobs <n>` | at most `n` cases at a time, `1` by default |
| `--seed <n>` | the seed of the generators; a failed run prints the one it used |
| `--timeout <duration>` | the time of each case without a `timeout` of its own: `500ms`, `5s`, `2m` or milliseconds |
| `--reporter human\|json` | lines per case and a summary, or one JSON object |
| `--watch` | run again when a file of the project changes |

CI passes `--forbid-only`, so a `test.only` left in a commit fails the
build instead of skipping the rest:

```sh
flick test --forbid-only
```

Each result is `passed`, `failed` with what failed (`assertion`, `failure`,
`defect`, `setup`, `timeout`, `cleanup` or `interrupted`), or `skipped`. The summary counts the
skipped cases, and the JSON report has a `skipped` count when there are any.
The exit code is `0` when every case that ran passed, `1` otherwise, and
`130` after SIGINT; skipped cases fail nothing.

## Diagnostics

| Problem | Diagnostic |
| - | - |
| a block without a name or braces, an unknown modifier, a `timeout` without its milliseconds, a `test.each` without its rows or pattern, a hook with a name, a fixture without its name or expression, or a suite holding something other than cases, hooks and fixtures | `std/T001` |
| two cases of one suite, two blocks of one module, or two fixtures of one suite, with the same name | `std/T002` |
| a fixture that uses itself or one declared after it, a shared fixture or a `before_all`/`after_all` hook that uses one each case makes anew | `std/T003` |
| a shared fixture of a `parallel` suite that is not `concurrent` | `std/T004` |
| a module with blocks that declares `tests` by hand | `FLICK-R008` |
| an error in a test's body | the usual one, at that code |

```flick fail std/T001
module App.BrokenTest

suite "users" {
  let ada = "Ada"
}
```
