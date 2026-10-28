---
title: Guides
description: Task-oriented routes through running programs, services, resources, browser UI and JavaScript interop.
---

Start with the task you need to finish. These recipes link to the existing
reference for complete syntax and contracts, rather than keeping a second
copy of the language manual. If the concepts are new, follow
[Learn Flick](../learn/) first.

## Run a program

**Goal:** create a Node application and get from source to a running entry
point.

With the Flick compiler, Node.js 24.18 or newer and pnpm available, follow
[Getting started](../start/getting-started/):

```sh
flick new my-app
cd my-app
flick install
flick check
flick run
```

`flick new` writes the project configuration, package manifest, entry module
and an example test. `flick install` installs the pinned dependencies;
`check`, `build` and `run` do not install packages for you.

1. Edit `src/App/Main.fl`. Keep the entry point's `main` signature and
   handle domain errors before returning from it.
2. Use `flick check` for compiler feedback without executing the program.
3. Use `flick run -- args` to pass arguments to the application, or
   `flick dev` to rebuild and restart after changes.
4. Use `flick build` when you need the emitted files instead of starting a
   process. The default app output includes `flick-main.js`, the entry runner.

**Read next:** [CLI](../reference/cli/),
[Project and packages](../reference/project/) and
[Code generation](../reference/codegen/). If the toolchain or a dependency
version does not match, start with `flick doctor` rather than changing the
runtime pin by hand.

## Inject a service and handle a domain error

**Goal:** keep application logic independent of a concrete implementation
and make expected failures visible to its callers.

1. Declare failures with `type error`, using data-only payloads. Give the
   calling function a runtime arrow whose error row lists those failures.
2. Declare the external operations in a `service`. Operations have runtime
   arrows; calling `Users.get`, for example, adds `Users` to the service row.
3. Implement the service as an `S.Impl` record and build a layer with
   `Layer.succeed S impl`. Use `provide layer { ... }` around the logic that
   needs it. A layer with dependencies passes its remaining requirements
   and construction errors to the surrounding code.
4. Use `try ... with` for the failures you can recover from. A partial or
   guarded handler does not remove an entire error from the row.
5. For a test, provide an in-memory implementation of the same service;
   the application function should not need a new signature.

The [effects reference](../reference/effects/) covers each step. The
[service and error examples](../learn/flick-and-effect/) show complete
modules beside their TypeScript/Effect equivalents. Use the
[standard library's testing API](../reference/stdlib/) and `flick test`
for the test harness.

**Watch for:** `try ... with` handles typed failures, not arbitrary defects
or interruption. A foreign throw needs an explicit conversion if it is a
domain error; see [Foreign functions](../reference/ffi/).

## Run concurrent work and release resources

**Goal:** finish a group of tasks without leaving background work or
resource users behind.

1. Build deferred work with `task { ... }`. A runtime call passed directly
   as an argument has already run before the receiving function starts.
2. Use `parallel` for a pair or `parallel_all limit` for a bounded list.
   Choose `parallel_results` when each branch's typed failure should be a
   result, rather than failing the group. Defects and interruption remain
   failures of the group.
3. Use `scope` around acquired resources. The scope provides its region's
   `Scope` service and owns the registered finalizers.
4. If you need an independently running child, name the scope and call
   `Fiber.spawn owner supervisor task`. Make the supervisor's behavior
   explicit; a child is not an unowned, fire-and-forget promise.
5. Keep the resource, and closures or tasks that capture it, inside the
   region's lifetime. The checker rejects escaping capabilities.

Read [Regions and captures](../reference/regions/) for the static rules and
[Runtime adapter](../reference/runtime/) for parallelism, races, timeouts
and cleanup order. The [standard library overview](../reference/stdlib/)
points to `File`, `Resource`, `Fiber` and the concurrency functions.

**Watch for:** closing interrupts and joins children before releasing
resources. A foreign promise that ignores cancellation is still awaited;
a timeout is not a promise of immediate shutdown. These rules matter when
binding a JavaScript resource or network client.

## Call another process with a typed contract

Start with the complete [RPC example](../reference/rpc/): describe a
procedure with request, success and error codecs, add it to a group, then
provide the exact handler record in a scope. Reuse that procedure value
from the client.

Choose the transport separately: HTTP for request/response calls,
WebSocket for a shared message connection, TCP for Node peers, or
`serve_stdio`/`spawn` for a child process, or `serve_worker`/`worker` for
a Web Worker. Start configuration from
`Rpc.defaults` and `Rpc.call_defaults` so serialization and transport
remain explicit. Test handler logic with `call_local`, but test codecs and
headers through a real connection.

## Find out where time and failures go

Use [observability](../reference/observability/) to choose the signal before
the destination: spans for a request's path, logs for events, metrics for
aggregates. Provide an OTLP layer around the application for collector
export, expose `Prometheus.route "/metrics"` for scraping, or connect a
DevTools layer to inspect spans.

**Watch for:** the combined OTLP layer takes a base URL; a single-signal
layer takes its full endpoint. Configuration export is opt-in per signal.
For CPU-bound recursion, read [code generation](../reference/codegen/):
stack-safe loops avoid stack growth, not synchronous execution time.

## Build a browser interface

**Goal:** mount a React-rendered Flick view and run event work under an
explicit policy.

1. Follow the web-page integration in
   [Getting started](../start/getting-started/) to import a `.fl` module
   through `@flick-lang/astro` in Astro or its Vite plugin in Vite. The
   integration invokes the local compiler; the host page starts the entry
   point through `runMain`.
2. Provide a DOM container and use `Ui.Dom.container` to find it. The UI
   adapter uses React 19 and React DOM, which are application-installed
   optional peer dependencies of the runtime.
3. Keep the state in atoms (`Atom.make`) and write views with `Ui.view`:
   a view's render reads atoms with `Atom.value` and returns a `Ui`.
   It runs no handler and performs no DOM access.
4. Choose an event policy deliberately: `Ui.Event.exhaust`, `latest`,
   bounded `queue`, or bounded `concurrent`. Write atoms inside the event
   task, not while rendering a view.
5. Inside `provide Atom.layer { … }`, start
   `Ui.Dom.mount container supervisor ui` in the intended lifetime.
   It runs until interrupted, and routes handler failures to its supervisor.
   A scope that owns the root waits for handler cleanup before releasing
   resources.

The [UI reference](../reference/ui/) describes props, safe URLs, foreign
React components, policies and the root's shutdown contract.

**Watch for:** `text` renders text, not HTML. An arbitrary string is neither
`SafeUrl` nor `SanitizedHtml`; use the documented constructors and treat
`Ui.trusted_html` as an explicit trust decision.

## Use JavaScript and TypeScript libraries

**Goal:** cross the language boundary with an accurate execution and
lifetime contract.

### Call into JavaScript

Use [Foreign functions](../reference/ffi/) to choose a binding mode:
`pure` for a function you trust to be pure, `sync` for synchronous runtime
work, `async` for a Promise factory, or `effect` for an Effect factory.
Declare argument arity, typed error conversion, optional values,
cancellation and callback retention where the API needs them.

For an installed package, `flick ffi import` can generate bindings from its
`.d.ts` files. Review its report and supply the policies the declarations
cannot express. A blocked export is not a usable fallback; unsupported
TypeScript forms and missing policies are reported explicitly. Use
`flick ffi check` when reviewing dependency changes and reviewed bindings.

### Export to TypeScript or a Promise caller

Use [Code generation](../reference/codegen/) to choose the public surface:

- `flick build --emit ts` emits TypeScript modules.
- `--emit-dts` writes declarations beside generated JavaScript.
- `--promise-facade` adds Promise entry points for eligible exported
  runtime functions that have no service requirements. Expected failures
  resolve as Effect `Result`; defects and interruption reject. This is not
  the usual convention of rejecting every error.

Do not expose a region, hidden scoped capture, higher-kinded variable or
open record as if TypeScript could preserve its full meaning. The
reference documents the projection limits and declaration diagnostics.
Use the [module export rules](../reference/modules/) to keep the boundary
small.

## Set up the editor and inspect a problem

**Goal:** use the compiler's actual analysis, not guess from generated
JavaScript.

- Follow [Editors](../reference/editor/) to run `flick lsp --stdio` or
  configure Zed. Hover, definition, completion and diagnostics use Flick's
  project analysis; built-in definitions can be browsed read-only.
- Use `flick explain FLICK-E001` to read a diagnostic's explanation.
- Use `flick inspect App.Main --emit js` or `--emit ts` to inspect lowering,
  and `--emit mapping` for the editor projection and source mappings.
- Use `flick fmt --check`, `flick check` and `flick test` for formatting,
  static feedback and the project's tests respectively. They answer
  different questions.

See [CLI](../reference/cli/) for options and exit codes. If you are deciding
between adding Flick and staying in TypeScript, read
[Flick and Effect](../learn/flick-and-effect/) before choosing an
integration boundary.
