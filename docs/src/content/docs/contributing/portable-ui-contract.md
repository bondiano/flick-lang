---
title: "Portable UI and rendering contract"
description: "Design for typed UI nodes, renderer operations, hydration, and native metadata."
---

**Proposed architecture.** The [UI guide](../../reference/ui/) describes the shipped API. This design specifies the interfaces and behavior for package extraction [#474](https://github.com/bondiano/flick-lang/issues/474), the neutral renderer [#475](https://github.com/bondiano/flick-lang/issues/475), and native metadata [#496](https://github.com/bondiano/flick-lang/issues/496). [Design issue #473](https://github.com/bondiano/flick-lang/issues/473)

## Problem

`ui` and `component` already produce Flick `Ui` values. The shared JS tree contains DOM tags, browser event bindings, and React components. `Ui.mount_with` accepts a factory whose `render` method takes a `ReactNode`. The `ui` sources have their own JS-gated package, while the compiler dispatches the corresponding plugin hooks through `Std`. To extract the package and support another renderer, the library needs to own the node and lifecycle rules. Adapters need a common input they can translate to their host. [JS tree](https://github.com/bondiano/flick-lang/blob/main/runtime/src/ui/react/nodes.ts) · [Renderer factory](https://github.com/bondiano/flick-lang/blob/main/runtime/src/ui/react/tree.ts) · [UI package](https://github.com/bondiano/flick-lang/blob/main/packages/ui/flick.toml) · [Plugin dispatch](https://github.com/bondiano/flick-lang/blob/main/flick-core/src/plugin/first_party.rs)

## Nodes and renderers

The UI library owns `Node`, component declarations, the types of props, slots and events, and the rules for component state. A node describes a tree. Building it performs no mount, subscription, or event task. A component render can read atoms through `Atom.Read`. State allocation belongs to the instance's setup, and writes and effects belong to owned tasks. The compiler keeps its generic block, type, effect, capture, and source-range machinery. The UI plugin owns the meanings of its marks and its rules for component expansion and checked calls. [Expansion of `ui`](https://github.com/bondiano/flick-lang/blob/main/plugins/ui/src/block.rs) · [Expansion of `component`](https://github.com/bondiano/flick-lang/blob/main/plugins/ui/src/component.rs) · [UI call checks](https://github.com/bondiano/flick-lang/blob/main/plugins/ui/src/calls.rs)

The types below sketch the boundary. `P` and `S` stand for a node's prop type and an event's snapshot type. An element or component declaration gives each slot its name and expected child shape. `RenderTree` is the evaluated tree of elements and text, plus its keys and any extensions supported by the target. The UI runtime resolves Flick components and owns their instances. The adapter receives event kinds and IDs. The root retains the handlers.

```text
Node = Text(String)
     | Fragment(List<Node>)
     | Element(ElementKind<P>, P, Slots<Node>, List<Binding<S>>)
     | Component(ComponentDef<P>, P, Slots<Node>)
     | Keyed(Key, Node)
     | Extension(Capability<P>, P, Slots<Node>)

Binding<S> = { event: EventKind<S>, handler: Handler<S> }
RenderBinding<S> = { id: BindingId<S>, event: EventKind<S> }
EventSink
  dispatch(BindingId<S>, S) -> AllowDefault | PreventDefault

Renderer<Target, Handle>
  mount(Target, RenderTree, EventSink) -> Result<Handle, RenderFailure>
  update(Handle, RenderTree) -> Result<Unit, RenderFailure>
  unmount(Handle) -> Result<Unit, RenderFailure>

ServerRenderer<Artifact>
  render(RenderTree, StateSnapshot) -> Result<Artifact, RenderFailure>

Hydrator<Target, Handle>
  hydrate(Target, RenderTree, EventSink) -> Hydration<Handle>

Hydration<Handle> = Attached(Handle, Reused | Rebuilt(Reason))
                  | Mismatch(Reason)
                  | Failed(RenderFailure)
```

`Target` and `Handle` are adapter-owned types. The root prepares new instances, read sets and bindings for a render. The adapter owns host handles and listeners and rolls back resources from an unsuccessful attach. After a successful mount, update, or hydrate, the root publishes the prepared changes and starts the new instances' mount tasks. Until then, dispatch uses only the last committed bindings. Each attachment receives its own `EventSink`. A revoked sink or inactive binding starts no task and returns `AllowDefault`. The root releases abandoned setup state and read subscriptions once. It can reuse the prepared tree across hydration recovery when the initial state still matches. If an update fails after touching the host, the root closes the affected handle and reports the failure to its owner.

Each target declares its supported elements, events, and extensions. The DOM adapter converts the tree to React and owns `ReactNode`, browser handles, CSS classes, navigation and default-prevention hooks. The tree distinguishes escaped text, safe URLs, and explicitly trusted HTML. Raw HTML, arbitrary DOM attributes and external React components enter through web capabilities. A native consumer gets a source diagnostic for a reachable unsupported construct. It can support a typed element with its own mapping. [Current UI constructors](https://github.com/bondiano/flick-lang/blob/main/packages/ui/src/Ui.fl)

## Identity, state, and event ownership

- **Identity.** Match siblings within the same parent and slot. A keyed node matches by explicit key and element kind or component declaration. An unkeyed node matches by sibling position and kind or declaration. A fragment groups children for matching without requiring a host element. Keys must be unique among keyed siblings in a slot. A duplicate produces a render error for that update. Changing props or reordering keyed siblings preserves their instances. Changing the kind, key or parent, or removing the node, ends the instance. Reinsertion starts a new one. A runtime key, a compiler declaration ID, and an atom's serialization key name different things.
- **State.** The caller provides a registry for a root or request. An instance owns its setup state and its subscriptions. Rendering records the atoms it reads, including dependencies of derived atoms. A commit replaces that view's read set and checks for a write between reading and subscribing. A changed value schedules another pass. Removing the instance releases its subscriptions and local state. An atom provided by an enclosing scope keeps the lifetime of that scope.
- **Events.** The adapter copies the typed snapshot while the host callback is valid. Dispatch evaluates a pure web `prevent_default` predicate synchronously and returns the decision to the adapter before the host callback returns. A binding ID follows its node instance and event slot across commits. The latest committed handler receives new events. Rebuilding the handler closure keeps the binding's running and waiting work. An ordinary handler owns policy state per binding. Uses of the same `Action` share policy state within a root, and `Action.with` gives each binding its own policy state. `exhaust` ignores events while busy. `latest` cancels and joins the previous run before starting the newest event. `queue` and `concurrent` enforce their bounds and report overflow to the root supervisor. Removing a binding stops its dispatch and drops its private queue. A shared action's queue belongs to the action owner. Started tasks remain with their root or instance owner. Closing that owner interrupts and joins them. [Shipped event rules](../../reference/ui/#handlers-and-policies) · [Atom subscription tests](https://github.com/bondiano/flick-lang/blob/main/packages/effect-ts/test/ui-atoms.test.ts) · [Action tests](https://github.com/bondiano/flick-lang/blob/main/packages/effect-ts/test/ui-actions.test.ts)

A solver can publish a coherent snapshot through the same read and write boundary. Its readers and subscriptions follow instance ownership. Constraint identity and solving belong to [#478](https://github.com/bondiano/flick-lang/issues/478).

## Server rendering, hydration, and close

The request owns its registry and any stream work. The server renders with that registry and emits a snapshot for an identified root. Each atom entry uses its serialization key and codec. Server rendering can read atoms and evaluate views. Client event handlers and instance mount tasks start only on the client. A streamed result delivers state before the markup that uses it. Ending or cancelling the stream releases its subscriptions. A plain HTML renderer can supply an artifact with no client attachment. [Server API](https://github.com/bondiano/flick-lang/blob/main/packages/ui/src/Ui/Server.fl) · [RT stream tests](https://github.com/bondiano/flick-lang/blob/main/runtime/test/rt-ui-stream.test.ts)

The client validates the initial snapshot for its root before applying any entry. It consumes the valid snapshot once, applies it before the first render, and uses the same initial inputs and keys as the server. The UI root controls when an attempt may dispatch events or start mount tasks.

| Result | Root behavior |
| - | - |
| The adapter reuses the server tree | Commit one live attachment and keep supported pre-boot input, focus, and selection. The browser adapter may adopt that input into Flick state. |
| The tree mismatches and the snapshot is valid | Report the root and reason. The adapter releases resources acquired by the failed attach and returns `Mismatch`. The root makes one fresh mount in the same container with its already initialized registry. Reset browser state that lived only in the replaced DOM. An adapter that guarantees the same outcome may return `Rebuilt` with the diagnostic reason. The root uses that attachment directly. |
| The snapshot is invalid or belongs to another root | A snapshot targeted at this root with incompatible entries causes a diagnostic and a fresh mount using client initialization. None of those entries is applied. A snapshot addressed to another root stays with its owner. The client hydrates without a seed only where the profile supports reconstructing the same start state. |
| The adapter fails, or the one fresh mount fails | Report the failure through the root's error channel, release acquired resources, and end the root. |

A remount or `Rebuilt` result ends the original stream delivery, including when the first attachment never became live. The root retains validated state already applied and restarts unresolved loads from their client initializers after commit. Later chunks of the abandoned stream are ignored. The root validates each live state chunk as a unit before applying it. An incompatible chunk is reported, applies no entries, and ends that stream. The live tree keeps its last committed state and restarts unresolved loads. Recovery never applies a snapshot again or dispatches an event from the abandoned attachment. Matching hydration preserves pre-boot drafts in the browser today. The DOM adapter delegates mismatches to React. [DOM adapter](https://github.com/bondiano/flick-lang/blob/main/runtime/src/ui/dom.ts) · [Hydration tests](https://github.com/bondiano/flick-lang/blob/main/packages/effect-ts/test/ui-hydration.test.ts) · [React hydration reference](https://react.dev/reference/react-dom/client/hydrateRoot)

Closing a root first stops dispatch and rejects further commits. The root detaches its stream observers and asks the adapter to unmount. The adapter detaches its listeners and releases its host resources on unmount or rollback, including error paths. The root releases instance subscriptions and pending events, interrupts the owned tasks, and waits for tasks and supervisors before its enclosing scope releases resources. The caller controls disposal of the supplied registry. Partial mount follows the same ownership order for resources that exist. Close is idempotent, and a cleanup failure does not skip later releases. [Root teardown](https://github.com/bondiano/flick-lang/blob/main/runtime/src/rt/ui/react.ts#L176-L183) · [Root lifecycle tests](https://github.com/bondiano/flick-lang/blob/main/packages/effect-ts/test/ui-roots.test.ts)

## Checked facts for native targets

Collect the facts below after expansion and type checking, while marks, types and source ranges are still available. The native consumer sees them before UI-specific calls become generic IR or foreign bindings. The UI plugin identifies UI constructs using checked marks, and the checker supplies types, effects and source ranges. A target profile decides support from these facts. Identity, lifetime and dependencies need separate representations in the target mapping. [Checked call facts](https://github.com/bondiano/flick-lang/blob/main/flick-plugin/src/facts.rs) · [Backend projection contract](https://github.com/bondiano/flick-lang/blob/main/docs/backends/contract.md) · [SwiftUI identity and lifetime](https://developer.apple.com/videos/play/wwdc2021/10022/)

| Checked fact | What the consumer uses |
| - | - |
| Component declaration ID, prop and slot types, element and child operations, key expressions | Select a view or widget, type its inputs, and preserve the identity rules across updates. |
| State declared in setup, reads in render, read/write bindings, watch and mount declarations | Assign storage and subscription lifetimes. Dynamic subscriptions still come from the render's actual reads. |
| Event type, snapshot, action signature and required effects, policy and owning instance or root | Bind an event and run its action under the same ownership and concurrency rules. |
| Model version, capability tags, and source ranges for declarations and uses | Check profile support and report an unsupported construct at its use site. |

A counter with text, a button, a typed action and an atom has enough facts for a profile that supplies those primitives. A keyed `for` over runtime values preserves a typed child computation and key expression. The profile must support that operation to render the list. An opaque React component requires the web interop capability. A native profile without it reports the marked call at its source location. The versioned checker projection in `BuildRequest` is a candidate transport. [Native metadata #496](https://github.com/bondiano/flick-lang/issues/496)

## Acceptance examples

| Example | Expected observations |
| - | - |
| **Counter.** Mount a text label and a button with a typed `exhaust` action. Dispatch one click and update the atom. | The label goes from `0` to `1`. Construction triggers zero actions, mount registers one binding, and disposal leaves zero live bindings. The neutral inputs are the node, the snapshot and state operations. [Counter fixture](https://github.com/bondiano/flick-lang/blob/main/tests/run/ui/counter.fl) |
| **Atoms and keys.** Update an atom read directly by one view and through a derived atom by another. Reorder two keyed rows and update one row's props. Remove that row and insert it under a new key. | Both dependent views observe the write, and a view that read neither atom has no reason to update. Props and reorder preserve local state for the same key. Removal releases its subscriptions, and a new key gets fresh instance state. [Atoms fixture](https://github.com/bondiano/flick-lang/blob/main/tests/run/ui/atoms.fl) · [Keyed example](https://github.com/bondiano/flick-lang/blob/main/docs/tour/app/app-ui.fl) |
| **SSR and hydration.** Render root A with `count = 5` and root B with `count = 9`. Transfer each snapshot to its matching root. Change only A's server markup to force a mismatch. | B still starts at `9` and reuses its markup. A reports the mismatch and ends with one live attachment displaying `5`. Its first click displays `6` and runs once. A malformed snapshot for A instead triggers client initialization. A closed root has no live callback or observer. [SSR fixture](https://github.com/bondiano/flick-lang/blob/main/tests/run/ui/server.fl) · [Successful hydration fixture](https://github.com/bondiano/flick-lang/blob/main/tests/run/ui/hydrate.fl) |

Exercise four more transitions in renderer acceptance.

- Rerender a button while its `exhaust` handler runs, then dispatch another click. The retained binding ignores that click while busy.
- Fail an initial mount after acquiring a host resource and delivering a premature event. The event starts no task. Release the acquired host and setup resources once and report the failure.
- Replace a streamed root with a pending load. The client restarts the load once. A late server chunk leaves the new tree untouched. Feed a live stream a chunk with one invalid entry and verify that none of its entries take effect.
- Close with a running handler and a queued event. The handler finishes cleanup before the enclosing scope releases resources. The queued event never starts. A second close releases nothing again. [Root lifecycle tests](https://github.com/bondiano/flick-lang/blob/main/packages/effect-ts/test/ui-roots.test.ts)

## Migration boundaries and open choices

| Work | Boundary and acceptance |
| - | - |
| Package extraction #474 | Give the UI package its node declarations, marks and plugin rules. Keep generic block expansion, checked types, effects, captures and spans in the compiler. Preserve the names and behavior seen by `ui` and `component`, UI diagnostics and source ranges, and the `Ui.Dom` and `Ui.Server` entry points. Verify external package resolution in locked and offline builds. |
| Neutral renderer #475 | Implement the lifecycle against the neutral inputs and run the acceptance examples. Map `Ui.mount_with` and the `[target.js].ui` override through the JS adapter during migration. Preserve the HTML expectations and behavior of `Ui.Dom`, `Ui.Server`, and the docs site's SSR entry points. |
| Native metadata #496 | Choose the transport and versioning for the checked facts. Show that an optimized projection retains the fields and spans used by a native profile. Exercise a supported example and a rejected web extension. |

Adapter choices include batching and reconciliation algorithms, the markup and stream protocol, how a host detects a mismatch, and which typed elements it supports. Every choice observes the root, identity, event and recovery rules above. Native element mappings, the Apple state bridge, and a browser renderer driven by atoms have their own tasks. [Browser renderer #476](https://github.com/bondiano/flick-lang/issues/476) · [Apple bridge #498](https://github.com/bondiano/flick-lang/issues/498)

## Alternatives considered

**Keep the React renderer factory.** This preserves the existing extension point with little migration. Every consumer then needs React input and loses the checked facts a native target requires at that boundary.

**Use HTML or a DOM tree as the interchange format.** That gives SSR a common artifact. Component identity, typed actions, and native slots would require a second model or reconstruction from browser details.

**Use library-owned typed nodes and target capabilities.** This provides a common model for the counter, atoms and SSR. It costs a mapping for each host and an explicit declaration for platform extensions. Each adapter can keep its own rendering algorithm.
