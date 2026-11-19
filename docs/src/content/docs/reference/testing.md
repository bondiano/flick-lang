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
time limit. A `before_all` is not part of any case's time.

```flick
module App.SlowTest

suite "network" timeout 5000 {
  test "answers" timeout 500 {}
  test "retries" {}
}
```

`Test.timeout` does the same for a test built by hand.

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
| a block without a name or braces, an unknown modifier, a `timeout` without its milliseconds, a hook with a name, or a suite holding something other than cases and hooks | `std/T001` |
| two cases of one suite, or two blocks of one module, with the same name | `std/T002` |
| a module with blocks that declares `tests` by hand | `FLICK-R008` |
| an error in a test's body | the usual one, at that code |

```flick fail std/T001
module App.BrokenTest

suite "users" {
  let ada = "Ada"
}
```
