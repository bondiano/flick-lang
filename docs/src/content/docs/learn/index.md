---
title: Learn Flick
description: A guided path from TypeScript and Effect to Flick's types, effects, services and scopes.
---

Flick is an ML-family language that compiles to JavaScript ES modules and
runs on Effect through `@flick-lang/runtime`. It gives effects, errors and
services language syntax; it does not replace the JavaScript host or the
Effect runtime.

This path introduces the ideas in the order you need them. The
[reference](../reference/grammar/) contains the detailed rules; the
[guides](../guides/) help when you already have a task in mind.

## Choose your starting point

- **I write TypeScript, but I am new to Effect.** Start with the first three
  steps below. Pay particular attention to the difference between a value,
  a runtime call and a deferred computation.
- **I already use Effect.** Read [Flick and Effect](./flick-and-effect/),
  then focus on function stages, error rows and lexical scopes. The runtime
  foundation is familiar; the source language and some ownership contracts
  are different.
- **I know an ML-family language.** Skim the syntax and types, then start
  with effects and services. Do not assume an empty effect row means a
  function is pure.

## 1. Run and inspect a small program

Follow [Getting started](../start/getting-started/) to create an app,
install its pinned dependencies and run its `main`. The documented Node
setup uses Node.js 24.18 or newer and pnpm; the compiler and runtime expect
matching versions of the runtime package and Effect.

Then use the [CLI reference](../reference/cli/) to distinguish these jobs:

- `flick check` checks source and test modules without running the app.
- `flick build` writes ES modules, source maps and module interfaces.
- `flick run` builds and runs the entry point.
- `flick dev` rebuilds and replaces the running app after changes.

**Try it:** change the greeting, run the program again, then inspect its
JavaScript with `flick inspect App.Main --emit js`. Read
[Code generation](../reference/codegen/) alongside the output: runtime
stages become Effect computations, while pure stages stay plain functions.

## 2. Model data and write pure functions

Read [Kinds and types](../reference/types/), using the
[grammar](../reference/grammar/) when syntax is unfamiliar. Start with
functions, records, `List`, `Option`, `Result` and variant types; higher-rank
polymorphism and kinds can wait until a library API needs them.

There are a few important changes from TypeScript:

- Named functions, including local ones, have a full signature immediately
  above their `let`. Instance methods may inherit their class signature;
  non-function values can be inferred.
- Functions are curried. `a -> b -> c` has two application stages.
- Records are exact. Accepting additional fields requires an open record
  row, not an assumption of structural width subtyping.
- A `match` must be exhaustive. An expected absence is usually `Option`,
  not an unchecked access that may produce `undefined`.

**Try it:** define a variant with an empty case and a case carrying a
string, then write a pure function that renders both. Remove one match arm
and read the exhaustiveness diagnostic before restoring it.

Next read [Modules](../reference/modules/) to split that code into files
and choose what to expose. Built-in modules are available qualified, such
as `String.trim` and `List.map`; your own modules use explicit imports.

## 3. Learn where effects run

Read [Effects, errors and services](../reference/effects/), starting with
stages and `task` / `await`.

| Flick type | How to read it |
| - | - |
| `a -> b` | a pure function |
| `a ->{Users; NotFound} b` | a runtime call requiring `Users`, possibly failing with `NotFound` |
| `a ->{} b` | a runtime call with no service requirements or expected errors; **not** a pure function |
| `Eff r e a` | a deferred computation producing `a`, with service row `r` and error row `e` |

Within a runtime function, `let user = Users.get id` binds the resulting
user, not an `Eff`. Use `task { ... }` when an API needs a computation as a
value, and `await` to run that value in the current fiber. Constructing a
`task` does not execute its body. Module initialization is pure.

For Effect users, `Eff r e a` corresponds to `Effect.Effect<A, E, R>`:
**the parameter order is different**. Flick's `none` is an empty row, not
`Unit`, `Never` or the `None` constructor.

**Try it:** move a runtime call into a pure function and inspect the
reported boundary. Then defer it with `task` and use it from a runtime
function. The [comparison](./flick-and-effect/) shows complete examples of
the direct-call style.

## 4. Separate dependencies from expected failures

Continue with the error, service and layer sections of the
[effects reference](../reference/effects/).

Declare a domain failure with `type error`; the error row makes it visible
to callers. Handle it with `try ... with`, which removes a failure only
when the patterns handle every value of that error. Defects and
interruption are not ordinary domain errors.

Declare an external capability with `service`, build an implementation
with `Layer.succeed`, and make it available with `provide`. A service is a
runtime dependency, not a class: the distinction matters when you later
read [Classes and instances](../reference/classes/). Classes choose
canonical implementations for types, such as `Show` or `Eq`; they do not
look up an application service.

**Try it:** replace a service implementation in one `provide` block without
changing its caller. Then handle one declared error and check that an
unhandled error remains in the caller's signature.

## 5. Give concurrent work and resources an owner

Read [Regions and captures](../reference/regions/), then the concurrency
and cleanup contracts in [Runtime adapter](../reference/runtime/).

Use `task` values with `parallel`, bounded `parallel_all`, races and
timeouts. Use `scope` for acquired resources, and a named scope owner when
you need `Fiber.spawn`. The region checker also tracks resources hidden in
closures and tasks, so returning a callback does not bypass a resource's
lifetime.

**Try it:** acquire a scoped resource, use it inside the scope, and attempt
to return a task that captures it. Read the rejection, then keep the work
inside its owner's lifetime. This is a static lifetime check, not a claim
that foreign JavaScript automatically obeys every contract.

## 6. Connect to the JavaScript world

Read [Foreign functions](../reference/ffi/) before binding a JavaScript
API. The binding mode, error conversion, cancellation and callback
retention are part of its contract; a TypeScript declaration cannot always
supply all of that information.

Then read the TypeScript section of [Code generation](../reference/codegen/)
for `.ts` output, `.d.ts` declarations and Promise facades. Some Flick types,
including regions, higher-kinded variables and open records, cannot be
faithfully projected into a public TypeScript API. Design the boundary
rather than assuming every function can cross it unchanged.

Use [Editors](../reference/editor/) for the language server and cross-language
projection, and [Project and packages](../reference/project/) for module
roots, manifests and dependencies.

## Continue with a real task

Choose a recipe in [Guides](../guides/), and use the
[standard library overview](../reference/stdlib/) to find modules instead
of recreating existing functions. Learn the domain-specific parts only
when you need them:

- [Schemas and codecs](../reference/schema/) for validating external data.
- [SQL](../reference/sql/) for typed PostgreSQL queries.
- [HTTP APIs](../reference/http-api/) for endpoint descriptions.
- [Command-line applications](../reference/cli-dsl/) for CLI definitions.
- [User interfaces](../reference/ui/) for React-rendered views and owned
  event tasks.

If your next decision is whether to introduce a new language at all,
[Flick and Effect](./flick-and-effect/) compares the trade-offs without
assuming Flick is the right choice for every TypeScript application.
