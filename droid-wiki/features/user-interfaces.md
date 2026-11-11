# User interfaces

Active contributors: bondiano, Vassiliy Kuzenkov. Non-bot authors on `origin/main` for UI plugins, application APIs, and examples.

Flick builds interfaces as typed values, separates rendering from event execution, and gives component-local state a lexical lifetime. Rendering and hydration implementations belong to [runtimes](../packages/runtimes/index.md); this page explains the source-level capability.

## Values, readers, and writers

`Ui` describes elements and event handlers; constructing it does not run an event. `Atom` describes reactive state, while a supplied `Atom.Registry` holds its values. A view reads atoms through `Atom.Read` and rerenders when those dependencies change. Actions/tasks use `Atom.Write`; rendering is not allowed to mutate state or perform arbitrary I/O.

Inside core `ui { ... }` context, names from `Ui` can be used unqualified and foreign `Ui.Component props` values can be applied as components. This context has compiler-owned behavior in `flick-core/src/types/ui.rs`; it is not one of the five standard plugin heads.

The `component name (props : P) = ...` declaration **is** a plugin feature:

- An expression-only body expands to a `Ui.view`.
- Setup statements followed by a final render expression expand to `Ui.stateful`.
- Setup runs once per mounted instance; the final render runs again as its atom dependencies change.

`Ui.stateful` quantifies an instance region `s`. `Atom.local` and setup-created capabilities carry that region, so they cannot escape the component instance. `Ui.on_mount` and `Ui.watch` register lifecycle work that is canceled/joined when the instance unmounts.

## Event policies are explicit

`Ui.Handler snapshot` values are created through `Ui.Event` or corresponding `Action` policies. A handler receives inert event data, not an unrestricted live DOM event object.

| Policy | Behavior while earlier work is active |
| - | - |
| `exhaust` | Ignore new events |
| `latest` | Interrupt and join earlier work, then run the newest pending event |
| `queue capacity` | Run serially with bounded waiting capacity; report rejection when full |
| `concurrent limit` | Run bounded concurrent tasks; report excess events |

The task's public row permits asynchronous work, atom writes, and document operations with no recoverable errors. Additional services must be provided and typed failures handled inside it. Failures/rejected events go to the root supervisor according to the mounting API.

The UI plugin reports `FLICK-U001` for a known non-positive/non-integral policy bound. Computed bounds are checked when constructing a handler at runtime. `Action.with` supports passing data to reusable top-level actions rather than rebuilding closures on each render.

`Ui.Dom.mount` consumes a container, supervisor, and UI and runs until interrupted. Its documented shutdown stops event intake, unmounts React, interrupts/joins handler tasks, and only then returns from the interrupted run.

## Server rendering and hydration

`Ui.Server.render` produces server markup without running handlers. Concurrent requests need separate `Atom.layer` registries. `Ui.Server.state` serializes registered atoms for a root; mounting hydrates using those values before the first client render. `Ui.Server.stream` exposes chunked rendering and suspended/task state transfer.

Serializable atom keys come from an unconstrained top-level binding, `Module.name`, not a manually repeated string. The compiler supplies binding/application facts; `plugins/ui/src/calls.rs` owns `FLICK-U003` checks for missing top-level bindings, constrained/function-local creation, incomplete applications, and duplicate atoms sharing a binding.

Browser-only atoms require an explicit server/hydration policy through `Atom.on_server` or `Atom.client_only`. `Atom.route` connects a `Url` witness to navigation; `Atom.serving` supplies the request URL for server rendering.

`Ui.Server.to_html` is a separate non-hydrated HTML path, useful for email. It omits event handlers and refuses foreign components that require React. `Ui.text` keeps markup in the text channel; URL/raw-HTML facilities use `Ui.Safe` types and explicit trust boundaries rather than treating arbitrary strings as safe markup.

## Current diagnostic ownership

The working tree moves capability-specific verdicts out of the core while retaining their diagnostic identities:

| Check | Fact producer | Verdict owner |
| - | - | - |
| Policy bounds (`U001`) | Constant watched-call facts from the checker | `plugins/ui/src/calls.rs` |
| Serializable atom binding keys (`U003`) | `flick-core/src/types/ui.rs` | `plugins/ui/src/calls.rs` |
| Component length/nesting/complexity (`T036`/`T037`/`T038`) | Source-hole structure measurement in `flick-core/src/plugin/expansion/check.rs` | `plugins/ui/src/component.rs` through `check_structure` |
| Foreign component application and ordinary render typing | Normal type/effect checker | `flick-core/src/types/ui.rs` and related checker paths |

The structure facts describe original expression/declaration holes, excluding generated wrappers. These are uncommitted API changes in this snapshot; consult the [SDK/WIT boundary](../packages/plugins.md) before changing them.

## Modification entry points and checks

Start with `packages/std/src/Ui.fl`, `packages/std/src/Ui/Event.fl`, and `packages/std/src/Atom.fl` for API types; `plugins/ui/src/component.rs` for component sugar; `plugins/ui/src/calls.rs` for watched-call verdicts. Rendering/browser changes must also inspect the selected runtime and [browser tools](../systems/browser-tools.md).

Use `tests/check/fail/ui/` for render restrictions and `tests/run/ui/` for component, registry, event, server, hydration, and serialization behavior. Follow the runtime/browser validation rows and test both new states and cancellation/unmount behavior, not only static markup. This wiki generation did not run a browser.

## Key source files

| Path | Purpose |
| - | - |
| `packages/std/src/Ui.fl` | Elements, views, instance setup, lifecycle, document capabilities |
| `packages/std/src/Atom.fl` | Reactive state, registry, browser policies, serializable state |
| `packages/std/src/Action.fl` | Reusable typed event actions and policies |
| `packages/std/src/Ui/Dom.fl` | Root mounting and supervisor contract |
| `packages/std/src/Ui/Server.fl` | Render, stream, state transfer, and plain HTML |
| `packages/std/src/Ui/Safe.fl` | URL and trusted-HTML boundary |
| `plugins/ui/src/component.rs` | Component expansion, formatting, structural warnings |
| `examples/conduit/client/src/Conduit/Client/View.fl` | Application UI consuming shared contracts |

Return to [application features](application-features.md) for end-to-end examples.
