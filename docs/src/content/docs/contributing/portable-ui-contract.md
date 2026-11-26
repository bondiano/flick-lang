---
title: "Portable UI and rendering contract"
description: "Design for per-component MVU, stores as services, compiled templates, the host interface, and native metadata."
---

**Proposed architecture.** The [UI guide](../../reference/ui/) describes the shipped API. This design replaces the decisions recorded for [#473](https://github.com/bondiano/flick-lang/issues/473): atoms as the state model, a renderer that receives whole trees, and a React DOM adapter. It specifies the interfaces and behavior for the work tracked in [epic #385](https://github.com/bondiano/flick-lang/issues/385) and the native metadata for [#496](https://github.com/bondiano/flick-lang/issues/496).

## Problem

A Flick UI today is a typed layer over React 19 that runs only on the JS target. State lives in atoms. A view becomes a memoized React component that subscribes to the atoms its render read, and a handler is a supervised task that writes atoms under an event policy. The node tree carries DOM tags and React props, `Ui.mount_with` takes a React renderer factory, and the runtime exists twice: once on the plain runtime and once on the Effect adapter. Go and Lua refuse UI. [JS tree](https://github.com/bondiano/flick-lang/blob/main/runtime/src/ui/react/nodes.ts) · [Views on rt](https://github.com/bondiano/flick-lang/blob/main/runtime/src/rt/ui/views.ts) · [Adapter copy](https://github.com/bondiano/flick-lang/tree/main/packages/effect-ts/src/ui) · [UI package](https://github.com/bondiano/flick-lang/blob/main/packages/ui/flick.toml)

Three goals need a different core:

- A typed message log: replay, time travel, tests without a browser, and devtools that agents can query.
- Native SwiftUI and Compose applications, whose bridge should carry data, not live subscriptions.
- One small core in Flick that runs on JS, Go, Lua and the VM. Only host operations stay platform code.

## Components and messages

Each component instance runs its own loop. It has a Model, a message type, a pure `update`, a `view` and its `subscriptions`. A message reaches only the `update` of the instance it was sent to. A parent never stores a child's Model and never sends a child a message: it passes props down and receives the child's emitted messages through a mapping it supplies. Each instance gets its own child scope. Its commands, subscriptions and provided services run in that scope, and closing the scope ends them. Every child is a failure boundary: a failed child shows its fallback until its key changes.

The sketch below is plugin syntax, so the [core syntax freeze](https://github.com/bondiano/flick-lang/issues/944) does not cover it. Model and Msg are ordinary types.

```text
type Model = { count : Int, pending : Bool }
type Msg = Increment | ResetLater | ResetDone

command reset_after (delay : Duration) : {Clock} Unit   // policy: exhaust

component counter (start : Int) =
  init = { count = start, pending = false }
  update model msg = match msg
    | Increment  -> { model with count = model.count + 1 }
    | ResetLater -> ({ model with pending = true }, [reset_after 1s |> done ResetDone])
    | ResetDone  -> { count = 0, pending = false }
  view model = ui {
    stack { gap = Space.s }
      text (show model.count)
      button { on_click = Increment } (text "+1")
      button { on_click = ResetLater; disabled = model.pending } (text "Reset")
  }
```

- **update.** Its type is `Model -> Msg -> Next Model Msg r`, where `Next` is either a Model alone or a Model with commands. It runs no effect. The plugin checks that the `match` is exhaustive.
- **Handlers.** A handler in a view is pure: a message (`on_click = Increment`) or a function from the event's snapshot to a message (`on_input = fun e -> Typed e.value`). It starts no task, so the view's row stays free of `Async`.
- **Commands.** A command is a named top-level declaration with an argument list, a result type, an effect row and a policy: `exhaust`, `latest`, `queue n` or `concurrent n`. Its name comes from the binding, and its result is mapped to a message. Policy state belongs to the instance and the command. `exhaust` ignores a new start while a run is busy. `latest` interrupts and joins the previous run, then starts the newest. `queue` and `concurrent` keep their bounds, and overflow is reported to the instance's supervisor. Removing the instance interrupts and joins its runs.
- **Subscriptions.** `subscriptions model` returns a keyed list of sources such as timers, sockets, the URL or storage. The runtime compares the keys after each update. It starts new keys, stops removed keys, and keeps a source whose key stays. Each item a source yields is a message, so the log records it.

## Stores and services

Flick has one state mechanism: a Model changed by messages. Shared state is a **store**: a service with a Model, messages and an `update`, but no view. A component reads a store in its view through the store's read effect, and it sends a store messages from commands. A provider component is a component that provides a store to its children.

```text
store Cart =
  init = { items = [] }
  update model msg = match msg
    | Added item -> { model with items = item :: model.items }

Cart.count : () ->{Cart.Read} Int      // read in a view; holes track it
Cart.send  : Cart.Msg ->{Cart.Write} Unit   // used inside commands

component shop () =
  view _ = ui {
    provide Cart.layer
      header ()      // reads Cart.count
      catalog ()     // a command sends Cart.Added
  }
```

- **Typed injection.** A component's type carries the effect row that its view reads, commands and subscriptions need. A parent satisfies part of that row with `provide` on its subtree, or the requirement passes into the parent's own row. `mount` and the server renderer receive what remains, so a missing service is a type error at the mount, not a failure at run time.
- **Lifetime.** A store lives as long as the scope that provides it: an instance, a route, a root or a server request. It closes when that subtree unmounts. The nearest provider wins. Code reaches a store only through its effect, so no handle can outlive the scope.
- **External sources.** The route, storage, search parameters and server streams arrive as subscriptions. The query cache is a store service. It keeps one entry per query and argument key, removes an entry an idle time after its last reader leaves, and accepts invalidation from commands. A failed fetch is a value; the cache never retries on its own.
- **Routing and forms.** A typed router is a store fed by a URL subscription, with a link element. `deriving Form` generates the Model, messages and `update` of a form's fields and their validation. Both belong to the UI package, so a client application needs no server framework.

## Views

A view returns a tree of portable elements. The core vocabulary is what every host supports: `text`, `button`, `input`, `stack`, `row`, `list`, `image`, `link`, `scroll`, and `keyed`. HTML tags, raw attributes and explicitly trusted HTML belong to a `Web` capability module. A native profile or the terminal host reports a reachable `Web` construct at its use site. The tree still distinguishes escaped text, safe URLs and trusted HTML.

Layout and style are typed props: direction, gap, padding, alignment, and color and size tokens. Each host maps them: the DOM to CSS variables and classes, the terminal to box properties, SwiftUI to modifiers, and Compose to `Modifier`. A `class` string or a stylesheet is a `Web` feature. A constraint solver can later supply layout through the same props. [Constraint systems #386](https://github.com/bondiano/flick-lang/issues/386)

**Templates with holes.** The UI plugin compiles each `ui {}` block into a static template and a list of holes. A hole is a text, an attribute or prop, an event binding, a child or a keyed list. Each hole records the paths it reads in the Model, the props and the stores. A field access such as `model.count` gives a static path. Any other expression is traced when the hole is evaluated. After an update, the runtime compares the old and new values along the recorded paths, which costs a reference check per field because models are immutable. It then evaluates only the holes whose paths changed. A keyed list compares its rows by key and updates only the changed rows. Static structure is never compared.

**Consistency rule.** For every Model, store and props value, applying the changed holes to a mounted template gives the same tree as rendering the view again from scratch. Path tracking is an optimization that must never change what the user sees.

## Host interface

A host implements node operations. The runtime drives them from templates and holes.

```text
Host<Node>
  createElement(ElementKind) -> Node
  createText(String) -> Node
  setText(Node, String)
  setAttr(Node, Name, Option<String>)
  setProp(Node, Name, Value)
  insertBefore(parent: Node, node: Node, ref: Option<Node>)
  remove(Node)
  setEvents(Node, List<RenderBinding>, EventSink)
  commit()                                   // once per flush and after mount
  supports: ElementKinds, Events, Capabilities
  // optional
  clone(Template) -> Node                    // e.g. a cloned <template> on the DOM
  adopt(Node, Template) -> Adoption          // hydration: reuse existing nodes

RenderBinding = { id: BindingId, event: EventKind<S> }
EventSink.dispatch(BindingId, S) -> AllowDefault | PreventDefault
```

The host owns its handles and listeners. A binding stores its handler as data. The host calls `dispatch` for the innermost element with a handler for the event. The host copies the typed snapshot while its callback is valid. Dispatch evaluates a pure `prevent_default` predicate synchronously and returns its decision before the callback returns. A revoked sink or an inactive binding sends no message and returns `AllowDefault`. A host releases what it acquired on unmount and rolls back a failed attach, including on error paths.

The imperative hosts are the DOM, the server HTML writer, the in-memory test host, the terminal and World of Warcraft frames. SwiftUI and Compose do not implement this interface. They consume the checked facts below and generate their own views.

## Identity, ownership, and close

- **Identity.** Siblings match within the same parent and slot. A keyed node matches by explicit key and element kind or component declaration. An unkeyed node matches by position and kind. An instance's path is the chain of component names and keys from the root, `Name[key]`. A second unkeyed child of the same component at one position is a render error for that update. Changing props or reordering keyed siblings keeps their instances and Models. Changing the kind, key or parent, or removing the node, ends the instance, and reinsertion starts a new one with `init`.
- **Delivery.** Messages to one instance are applied in the order they were sent, one `update` at a time. After a batch of updates, the runtime applies the changed holes in one flush, parents before children, and then calls `commit`. A child whose props are unchanged keeps its holes.
- **Ownership.** Each instance owns its Model, its subscriptions, its command runs and the stores it provides. Commands that started remain with their instance. Closing the instance interrupts and joins them before its scope releases resources.
- **Close.** Closing a root first stops dispatch and rejects further flushes. It detaches stream observers and asks the host to unmount. Then it closes the instances from the leaves up: subscriptions, queued messages, command runs, then provided stores. It waits for tasks and supervisors before the enclosing scope releases resources. Close is idempotent, and a cleanup failure does not skip later releases. A partial mount follows the same order for the resources that exist.

## Server rendering and hydration

The server renders templates directly to markup. Static parts are serialized ahead of time, and holes are written as they are evaluated. A server render evaluates `init`, the updates triggered by server-side loads, and views. Client handlers, subscriptions and mount commands start only on the client. The request owns its store scope and any stream work.

The **snapshot** for an identified root contains the Model of every server-rendered instance by path, the Models of the stores provided in that root, and the query cache entries that the render read. The codecs come from the types, so the author writes none. A Model or store field that cannot be serialized, such as a function or a handle, is a compile error at the component's use under a server render. A component marked `client_only` renders a placeholder on the server and starts with `init` on the client. A streamed result delivers state before the markup that uses it. Ending or cancelling the stream releases its subscriptions.

The client validates the root's snapshot before applying any entry. It applies the snapshot once, before the first render, and it uses the same props and keys as the server. Hydration walks the existing nodes with `adopt` and checks their structure against the templates.

| Result | Root behavior |
| - | - |
| The host adopts the server nodes | Commit one live attachment. Keep supported pre-boot input, focus and selection, and let the DOM host move that input into the Model through messages. |
| The structure mismatches and the snapshot is valid | Report the root and reason. The host releases what the failed attach acquired. The root makes one fresh mount in the same container with the already applied Models. Browser state that lived only in the replaced nodes is reset. |
| The snapshot is invalid or belongs to another root | A snapshot targeted at this root with an incompatible entry is reported, and none of its entries is applied. The root mounts fresh with `init`. A snapshot addressed to another root stays with its owner. |
| The host fails, or the one fresh mount fails | Report the failure through the root's error channel, release the acquired resources, and end the root. |

A remount ends the original stream delivery, including when the first attachment never became live. Later chunks of the abandoned stream are ignored. The root validates each live state chunk as a unit. An incompatible chunk is reported, applies no entries, and ends that stream. The tree keeps its last committed state, and unresolved loads restart from the client. Recovery never applies a snapshot twice and never dispatches an event from an abandoned attachment.

## Message log and tooling

The runtime writes one log per root. An entry records the instance path, the message and its source (a handler, a command result, a subscription or a store), the commands it started, and when each run ended and how. Models are not copied into entries. A Model at an entry is recomputed by folding `update` over the log from the last snapshot.

- **Story tests** run against the in-memory test host. A test gives a Model, sends a message, expects the commands that `update` returned, and resolves a command with the message its result would produce. No effect runs. Scene tests add the view: they find elements by role or label, click or type, and check text. Play tests run real commands on a test clock.
- **Replay.** A recorded session is a test fixture. Replaying it on the test host checks that the same messages give the same Models and commands.
- **Devtools.** `flick dev` shows the log, the causal chain of a message, each instance's Model, props and store reads at any entry, and time travel that paints the page at an entry and returns to the live state.
- **MCP.** The dev server exposes the log to agents on loopback only: list sessions, instances and entries, get an entry or its chain, get or diff the state at an entry, show an entry, resume, get the JSON Schema of an instance's messages, wait for command runs, and dispatch messages. The message schema is derived from the type. Dispatch is off unless enabled.

## Checked facts for native targets

Collect these facts after expansion and type checking, while marks, types and source ranges are available. A native consumer sees them before UI calls become generic IR. A target profile decides support from them and reports an unsupported construct at its use site. [Checked call facts](https://github.com/bondiano/flick-lang/blob/main/flick-plugin/src/facts.rs) · [Backend projection contract](https://github.com/bondiano/flick-lang/blob/main/docs/backends/contract.md)

| Checked fact | SwiftUI consumer | Compose consumer |
| - | - | - |
| Component declaration ID, props and slot types, element kinds, keys | A `View` per component, `ForEach` with stable IDs | A `@Composable` per component, `key` in lazy lists |
| Model and Msg types, the `update` function | An `@Observable` model owned by the native core, messages as Swift enums | A state holder owned by the native core, messages as sealed classes |
| Template holes and their read paths | Property reads that SwiftUI observation tracks | State reads that recomposition tracks |
| Commands with their rows and policies, subscriptions | Run in the Flick core; the bridge never runs them | Run in the Flick core; the bridge never runs them |
| Store declarations, provide points, read effects | Environment values scoped to the subtree | `CompositionLocal` scoped to the subtree |
| Style props and tokens, capability tags, model version, source ranges | Modifiers; profile check | `Modifier`; profile check |

The bridge carries **messages in and snapshots out**. The native view sends a message value across the ABI. The Flick core applies `update`, runs commands, and publishes a versioned snapshot of the changed paths. SwiftUI calls the core through a Swift ABI on the main actor. Compose calls it through JNI. Neither side keeps an independently mutable copy of the Model. A web capability or an HTML tag is rejected by these profiles at its source location. The versioned checker projection in `BuildRequest` is a candidate transport. [Native metadata #496](https://github.com/bondiano/flick-lang/issues/496) · [Apple bridge #498](https://github.com/bondiano/flick-lang/issues/498)

## Hosts and backends

The UI core is written in Flick over the kernel, and only host operations are foreign code. The same loop, commands, stores and templates therefore run on each backend.

| Backend | Hosts in the first milestone |
| - | - |
| JS | DOM, server HTML with streaming and hydration, terminal |
| VM | In-memory test host for Story, Scene and replay; the REPL |
| Go | Server HTML |
| Lua | World of Warcraft frames, built on the event, timer and pool helpers of [#842](https://github.com/bondiano/flick-lang/issues/842) |
| Swift, Kotlin | Generated SwiftUI and Compose views over the native core, from the checked facts |

A headless kit in the UI package provides keyboard, focus and ARIA behavior for select and listbox, tabs, field and form, dialog, menu, popover, combobox, toast, tooltip, checkbox, radio and switch. The author supplies the markup and the style.

## Acceptance examples

| Example | Expected observations |
| - | - |
| **Counter.** Mount a label and a button whose click sends `Increment`. Click once. | The label goes from `0` to `1`. Building the tree sends no message. Mount registers one binding, the flush touches one text hole, and close leaves zero live bindings. The log holds one entry. [Counter fixture](https://github.com/bondiano/flick-lang/blob/main/tests/run/ui/counter.fl) |
| **Keyed rows.** Reorder two keyed rows, change one row's props, then remove that row and insert it again under a new key. | Reorder and props keep both instances and their Models. Only the changed row's holes update. The removed row's subscriptions and command runs end, and the new key starts with `init`. [Keyed example](https://github.com/bondiano/flick-lang/blob/main/docs/tour/app/app-ui.fl) |
| **Shared store.** Provide a cart store over two subtrees. A command in one subtree sends `Added`. | Only the holes that read the cart count update, in both subtrees. A component outside the provider that reads the cart fails to type check. Unmounting the provider closes the store once. |
| **SSR and hydration.** Render root A with `count = 5` and root B with `count = 9`, then change only A's server markup. | B adopts its nodes and starts at `9`. A reports the mismatch and ends with one live attachment showing `5`. Its first click shows `6`, and the click is applied once. A malformed snapshot for A gives a fresh mount with `init`. [SSR fixture](https://github.com/bondiano/flick-lang/blob/main/tests/run/ui/server.fl) |
| **Replay.** Record a session of three clicks and one resolved command, then replay it on the test host. | The replay produces the same Models and the same command starts in the same order, and it runs no effect. |

Four lifecycle transitions are also part of acceptance:

- Re-render a button while its `exhaust` command runs, then click again. The click's message is applied, and the command does not start a second run.
- Fail an initial mount after the host acquired a node and delivered a premature event. The event sends no message. The host and the instance release their resources once, and the failure is reported.
- Replace a streamed root that has a pending load. The client restarts the load once, and a late server chunk leaves the new tree unchanged. A live chunk with one invalid entry applies none of its entries.
- Close a root with a running command and a queued message. The command finishes its cleanup before the enclosing scope releases resources. The queued message is never applied, and a second close releases nothing.

## Formal obligations

The reference model executes the UI runtime: per-instance message delivery, command policies, subscription keys, store and scope lifetimes, close order and the hydration decision. Lean states and proves four properties over the model:

- Folding `update` over a recorded log gives the recorded Models and command starts.
- Every acquired resource is released exactly once on close, mismatch recovery or failed mount.
- The policy invariants hold: an exhausted command has at most one run, `latest` has at most one run, and queues keep their bounds.
- The consistency rule holds: hole updates give the same tree as a full view render.

A model proof is not a proof of the compiler or the runtimes. Differential tests compare each runtime with the model.

## Migration

The new core, hosts and plugin are built beside the shipped API under internal names. One switch change then migrates every consumer: the site widgets, playground, landing, tour, course, the Conduit client, the fixtures and the SSR load test. The same change deletes atoms, React and the old plugin rules. There is no compatibility layer.

| Shipped API | Replacement |
| - | - |
| `Atom.make`, `Atom.local`, setup in `Ui.stateful` | A Model field and `init` |
| `Atom.derived` | An expression in the view, tracked by hole paths |
| `Action.*`, `Ui.Event.*` | A command policy and a pure handler that returns a message |
| `Atom.query`, `loader`, `mutation`, `query_stream` | The query store, commands and subscriptions |
| `Atom.route`, `Ui.route_link` | The router store and its link |
| `Atom.stored`, `search_param`, `on_server`, `client_only` | Host subscriptions and commands; `client_only` on a component |
| `Atom.serializable*` | Codecs derived for Models and stores |
| `Form` | `deriving Form` that generates Model, Msg and `update` |
| `Ui.Document` (focus, scroll, measure) | Host commands |
| `Ui.Dom.mount`, `Ui.Server.*` | Mount on a host and the server renderer |
| `Ui.component`, `Ui.Renderer`, `Ui.mount_with`, `[target.js] ui` | Removed. React interop is not planned |

The UI language items leave the compiler. A view's read effect and the setup region become ordinary services and effects. The plugin moves out of the standard plugin into the UI package. Diagnostics U001 to U007 are retired or replaced explicitly, and their codes are not reused for a different meaning. [Package extraction #474](https://github.com/bondiano/flick-lang/issues/474)

## Alternatives considered

**Atoms with read sets (the previous design).** Fine-grained and already shipped. Writes happen inside effectful tasks, so there is no complete message log, no replay without running effects, and a native bridge would have to carry dynamic subscriptions.

**Depend on an existing TypeScript MVU library.** Effect UI implements per-component MVU, a public host interface, story tests and devtools with an MCP server, and it is the closest prior art for this design. As a dependency it would tie every UI program to one JS runtime, and it provides no checked facts for native, Go or Lua targets.

**A virtual DOM per component.** Simple and proven: the view runs again and the result is compared, stopping at children with unchanged props. Templates with holes keep the same component boundary and skip both the rerun of static structure and the comparison.

**Runtime signals derived from the Model.** Every field becomes a cell, with no compiler work. Tracking would be dynamic only, and the runtime heavier on every backend.

**A single root Model.** The classic Elm layout is the easiest to formalize. Every message runs the view from the root, parents forward their children's messages, and parts of an application cannot load separately.

**Keep a React adapter.** Foreign React components would keep working. React would remain in the core's bundles, and hydration and events would have two semantics.

Prior art: Elm and its guide's advice against stateful components, Swift's Composable Architecture, Android's unidirectional data flow with Compose, and renderers that compile static templates with dynamic holes.
