---
title: Flick and Effect
description: The shared runtime foundation, equivalent source examples and practical reasons to choose Flick or TypeScript with Effect.
---

Flick and Effect are not competing runtimes. **Effect is a TypeScript
library; Flick is an ML-family language that compiles to JavaScript and
uses Effect through a runtime adapter.** The choice is primarily about the
source language, compiler checks, tooling and integration boundary you
want—not about replacing Effect's execution model.

If your TypeScript application already uses Effect successfully, adopting
Flick is a separate language decision, not a prerequisite for obtaining
typed errors, dependency injection or structured concurrency.

## The shared foundation

Effect describes a computation with three parameters:
[`Effect.Effect<A, E, R>`](https://effect.website/docs/v4/getting-started/the-effect-type/)
for success, expected error and required services. A value describes work;
creating it does not by itself execute that work.

Flick's first-class computation type is `Eff r e a`, with **services first,
errors second and success last**. A runtime function such as
`String ->{Users; NotFound} User` compiles to a function returning an
Effect. A pure `String -> User` stays a plain JavaScript function.

The current [runtime adapter](../../reference/runtime/) pins Effect
**4.0.5**. Generated modules import `@flick-lang/runtime`, not a second
independent effect engine. The adapter also defines Flick-specific
contracts for lexical owners, supervision, foreign promises and shutdown;
sharing Effect does not make every low-level API interchangeable.
[Code generation](../../reference/codegen/) documents the actual lowering.

| Topic | TypeScript with Effect | Flick on Effect |
| - | - | - |
| Source language | TypeScript plus Effect APIs | A separate ML-family language and compiler |
| Computation value | `Effect.Effect<A, E, R>` | `Eff r e a`, built with `task` |
| Sequential runtime code | `Effect.gen` and `yield*`, or library combinators | Direct calls in runtime functions; `await` for an `Eff` value |
| Expected failures | Typed error channel; tagged data and `catchTag` | `type error`, error rows and `try ... with` |
| Services | `Context.Service`, implementations and `Layer` | `service`, `S.Impl`, `Layer` and `provide` |
| Pure code | Ordinary TypeScript; effect construction and execution are library boundaries | Distinct pure and runtime arrows checked by the compiler |
| Resource lifetime | Effect scopes and acquisition/finalization APIs | Runtime cleanup plus static regions and capture checks |
| JavaScript/TS boundary | Native TypeScript imports and types | Explicit foreign bindings and generated public projections |
| Effect API access | The installed library's TypeScript surface | The pinned, reviewed Flick surface and its recorded exclusions |

The examples below keep public types visible on both sides. TypeScript can
infer many return types; these are readable equivalents, not a
shortest-code or line-count contest. Each Flick block is a complete module
for the corresponding `src/Learn/Errors.fl` or `src/Learn/Services.fl` path.
They export reusable computations rather than an executable `main`.

## Example: a typed failure and a fallback

Both versions return `"Ada"` for identifier `1` and `"Guest"` when the name
is absent. The lookup is deliberately a small, in-memory rule, not a
pretend database integration.

### TypeScript with Effect

The declarations and recovery follow Effect's official
[expected errors guide](https://effect.website/docs/v4/error-management/expected-errors/).

```ts
import { Data, Effect } from "effect"

class NotFound extends Data.TaggedError("NotFound")<{
  readonly id: number
}> {}

const find = (id: number): Effect.Effect<string, NotFound> =>
  id === 1
    ? Effect.succeed("Ada")
    : Effect.fail(new NotFound({ id }))

export const lookup = (id: number): Effect.Effect<string> =>
  find(id).pipe(
    Effect.catchTag("NotFound", () => Effect.succeed("Guest")),
  )
```

### Flick

```flick
module Learn.Errors exposing (lookup)

type error NotFound = NotFound Float

find : Float ->{; NotFound} String
let find id =
  if id == 1 then "Ada"
  else from_result (Err (NotFound id))

lookup : Float ->{} String
let lookup id =
  try find id with
  | NotFound _ -> "Guest"
```

`find` may fail with `NotFound`. `lookup` handles every value of that error,
so its error row is empty. An empty runtime arrow, `->{}`, still does not
mean the function is pure. Calling it is a runtime operation.

The Flick body uses an ordinary branch, direct call and pattern handler.
The compiler supplies the Effect construction and sequencing. That is the
reduction in ceremony: these operations belong to the language instead of
being written as library composition at each call site. The full rules are
in [Effects, errors and services](../../reference/effects/).

Neither version turns every possible problem into `NotFound`. Effect
separates [expected failures from defects](https://effect.website/docs/v4/error-management/two-error-types/),
and Flick preserves that distinction. `try ... with` is not a blanket
JavaScript exception catcher.

## Example: declare, use and provide a service

Both versions describe a computation that obtains a `Greeter` service and
produces `"Hello, Ada"`. A layer supplies the implementation. Constructing
`program` does not run it, print anything or start a background process.

### TypeScript with Effect

This uses the patterns from
[Managing Services](https://effect.website/docs/v4/requirements-management/services/)
and [Managing Layers](https://effect.website/docs/v4/requirements-management/layers/).

```ts
import { Context, Effect, Layer } from "effect"

class Greeter extends Context.Service<
  Greeter,
  { readonly greet: (name: string) => Effect.Effect<string> }
>()("Learn.Services.Greeter") {}

const welcome = (name: string): Effect.Effect<string, never, Greeter> =>
  Effect.gen(function* () {
    const greeter = yield* Greeter
    return yield* greeter.greet(name)
  })

const greeterLayer = Layer.succeed(Greeter, {
  greet: (name: string) => Effect.succeed("Hello, " + name),
})

export const program: Effect.Effect<string> = Effect.provide(
  welcome("Ada"),
  greeterLayer,
)
```

### Flick

```flick
module Learn.Services exposing (program)

service Greeter =
  greet : String ->{} String

welcome : String ->{Greeter; none} String
let welcome name =
  Greeter.greet name

greeter_layer : Layer none none {Greeter}
let greeter_layer =
  Layer.succeed Greeter { greet = fun name -> "Hello, " ++ name }

program : Eff none none String
let program =
  task {
    provide greeter_layer {
      welcome "Ada"
    }
  }
```

`welcome` requires `Greeter`; `program` has no remaining service
requirement. In Flick, `service` creates the service key and operation
access, while the call in `welcome` is sequenced by the compiler. A pure
implementation callback is lifted to the declared runtime callback type.
The surrounding `task` makes the whole operation a value that can be
passed around without running it.

This is not a claim that Effect requires generators everywhere. Its
[creation guide](https://effect.website/docs/v4/getting-started/creating-effects/)
and service guide also show constructors and combinators. Flick chooses
one checked direct-call syntax for runtime bodies instead of making that
choice at each use.

## What Flick adds

Flick adds a language and static rules over this runtime foundation:

- **An enforced pure/runtime distinction.** Signatures distinguish
  `a -> b` from `a ->{r; e} b`; a runtime call is rejected in a pure body,
  even when both rows are empty.
  [Types](../../reference/types/) and [effects](../../reference/effects/)
  explain the rules. A `foreign pure` binding remains a trust contract,
  not a proof about arbitrary JavaScript.
- **ML-style data modeling.** Variant types, exhaustive `match`, exact
  records and open record rows are language features, not conventions
  layered on a TypeScript union. See [Types](../../reference/types/).
- **Classes, kinds and canonical instances.** Classes can abstract over
  type constructors; the checker resolves dictionaries and rejects
  overlapping instances. `deriving` supplies supported standard instances.
  These are not JavaScript classes or runtime services; see
  [Classes and instances](../../reference/classes/).
- **Static resource escape checks.** Regions and capture qualifiers track
  scoped values even inside returned closures or tasks. Owners and
  supervisors make the lifetime of spawned work explicit. See
  [Regions and captures](../../reference/regions/) for the guarantees and
  the role of trusted foreign retention contracts.
- **A compiler-controlled output boundary.** ES modules and source maps,
  TypeScript output, declaration generation and eligible Promise facades
  let Flick participate in a JavaScript codebase. This does not make every
  Flick type representable in TypeScript; see
  [Code generation](../../reference/codegen/).

Flick does **not** add resource finalization to a runtime that lacked it.
Effect already has
[resource-safe acquisition and finalization](https://effect.website/docs/v4/resource-management/introduction/).
The additional Flick feature is the source-language ownership and capture
checking, together with its adapter's specified shutdown contracts.

## What Effect does especially well

**Adoption inside existing TypeScript.** You can add Effect to a TypeScript
project and use it in ordinary `.ts` modules without introducing another
source language, compiler or foreign-binding layer. The official
[installation guide](https://effect.website/docs/v4/getting-started/installation/)
covers the TypeScript setup and host integrations.

**A directly accessible ecosystem.** Effect's
[overview](https://effect.website/docs/v4/onboarding/)
includes streaming, observability, schema validation, HTTP, SQL, CLI and
other packages on the same core. In TypeScript, you work with the installed
packages' own APIs and documentation. Flick's
[Effect coverage manifest](../../reference/effect-surface/) distinguishes
wrapped APIs, language constructs, exclusions and pending bindings. A
recorded decision is not the same as a callable Flick wrapper for every
Effect export.

**Native TypeScript integration.** Existing SDKs, framework types and
TypeScript-specific abstractions remain in their native language. Flick's
[FFI importer](../../reference/ffi/) has an explicit supported subset;
complex overloads, unions and callback policies may require a manually
reviewed boundary or be unsupported. Public Flick types also have
projection limits. If the job is mostly connecting TypeScript libraries,
remaining in TypeScript avoids that boundary work.

These are practical strengths, not evidence that Effect is less typed or
that its programs inherently need more source code. The languages offer
different tools for expressing the contracts.

## Which should you choose?

**Choose TypeScript with Effect when** preserving your existing TypeScript
workflow is the priority, most of the work lives in TypeScript SDKs or
frameworks, you need direct access to an Effect package's current API, or
your team does not want to maintain another language toolchain. You still
get typed expected errors, service requirements, structured concurrency
and resource management.

**Choose Flick when** you specifically want ML-style data and class
abstractions, checked pure/runtime boundaries, direct-call effect syntax
and static region/capture rules—and are willing to adopt its compiler,
syntax, pinned Effect release and explicit interop contracts. Start with
a contained module whose boundary you can describe accurately rather than
assuming the entire application must be rewritten.

**Combine them deliberately when** a small Flick component is useful inside
a JavaScript application. Review its generated declarations or Promise
facade, keep unprojectable types internal, and check error representations
and cancellation behavior. Flick errors have compiler-generated nominal
tags; do not assume they have the same shape as the `Data.TaggedError`
class above merely because the source names match. A shared Effect
foundation is not automatic source or representation compatibility.

Next, follow [Learn Flick](../) for the sequence, or choose a concrete
integration task in [Guides](../../guides/).
