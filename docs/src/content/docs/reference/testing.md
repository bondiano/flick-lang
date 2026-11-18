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
| `--reporter human\|json` | lines per case and a summary, or one JSON object |
| `--watch` | run again when a file of the project changes |

CI passes `--forbid-only`, so a `test.only` left in a commit fails the
build instead of skipping the rest:

```sh
flick test --forbid-only
```

Each result is `passed`, `failed` with what failed (`assertion`, `failure`,
`defect`, `cleanup` or `interrupted`), or `skipped`. The summary counts the
skipped cases, and the JSON report has a `skipped` count when there are any.
The exit code is `0` when every case that ran passed, `1` otherwise, and
`130` after SIGINT; skipped cases fail nothing.

## Diagnostics

| Problem | Diagnostic |
| - | - |
| a block without a name or braces, an unknown modifier, or a suite holding something other than `test` and `suite` blocks | `std/T001` |
| two cases of one suite, or two blocks of one module, with the same name | `std/T002` |
| a module with blocks that declares `tests` by hand | `FLICK-R008` |
| an error in a test's body | the usual one, at that code |

```flick fail std/T001
module App.BrokenTest

suite "users" {
  let ada = "Ada"
}
```
