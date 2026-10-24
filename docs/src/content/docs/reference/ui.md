---
title: "User interfaces"
---

`Ui r e` (see [Type-level programming and eDSLs](../../spec/10-type-programming-and-edsl/)) describes a user interface whose event handlers need
the services `r` and may fail with `e`. Building one is pure: it runs no
handler, reads no state and touches no DOM. `Ui.Dom.run` renders it with React
19 (`react-dom/client`) and runs each event as a task.

```flick
type alias SaveButtonProps c =
  {
    label : String;
    disabled : Bool;
    on_save : (Unit ->{Users; SaveError} Unit)^c;
  }

save_button : SaveButtonProps c -> (Ui {Users} {SaveError})^c
let save_button props =
  ui {
    button
      {
        disabled = props.disabled;
        on_click = Ui.Event.exhaust (fun _ ->
          task { props.on_save () }
        );
      }
      [text props.label]
  }
```

`c` is a capture row (see [regions](../regions/#captures)). Like any new type
variable of a signature it is quantified over the whole signature, so
`forall c.` can be left out ([types](../types/)).

The two `^c` are what let `save_button` accept any callback. A function type
without a qualifier holds no regions. So with `on_save : Unit ->{Users;
SaveError} Unit`, a callback that uses a `File s` from an enclosing `scope`
is rejected where it is passed (`FLICK-G001`), even when the `Ui` never
leaves the scope. With `^c` on the field the callback may hold anything, and
`^c` on the result keeps those captures in the `Ui`, so a `Ui` that holds a
region still cannot leave its scope. A qualifier on the result alone does not
help, since the field would still allow no captures. A top-level signature is
checked as written, so these qualifiers are never inferred. A component whose
callbacks never hold scoped resources can drop both of them.

`ui { … }` is a block in which the names `Ui` exports need no `Ui.`: a local
binding still wins, then a name of `Ui`, then the module's own names. It
adds nothing else, so `ui { button … [text …] }` and `Ui.button … [Ui.text
…]` compile to the same IR.

## Elements and props

Props are exact records: a missing field, an unknown one or a wrong type is
an error. A key is metadata, given with `Ui.keyed`, never a prop.

| Function | Element | Children |
| - | - | - |
| `text s` | text, through React's text channel: markup stays text | none |
| `fragment`, `optional`, `empty` | no element | `List`, `Option`, none |
| `div`, `span` `{ class_name }` | `div`, `span` | `List (Ui r e)` |
| `paragraph`, `heading`, `list`, `item` | `p`, `h1`, `ul`, `li` | `List (Ui r e)` |
| `button { disabled; on_click }` | `button type="button"` | `List (Ui r e)` |
| `input { value; placeholder; on_input }` | `input`; `value` is the initial text | none |
| `textarea { value; placeholder; on_input }` | `textarea`, as `input` over several lines | none |
| `pre { class_name }` | `pre`: spaces and line breaks kept | `List (Ui r e)` |
| `link { href : SafeUrl }` | `a` | `List (Ui r e)` |
| `html (s : SanitizedHtml)` | `div` with the HTML | none |
| `select (SelectProps a r e c)` | `ul role="listbox"`; choosing an item runs `on_change` with it | items through `render_item` |
| `panel { header; body; footer }` | `section` with `header`, `main`, `footer` | named places |
| `tabs { selected } (NonEmpty (Tab r e))` | a tab list and the selected panel | `Tab`s from `tab label ui` |
| `component c props` | a foreign React component | its props |

`text` is `forall r e. String -> Ui r e`, so it sits among the children of
any element. `SafeUrl` comes only from `Ui.safe_url`, which accepts relative
URLs and the schemes `http`, `https` and `mailto` and refuses control
characters; `SanitizedHtml` only from `Ui.trusted_html`, the program's own
word. A `String` is neither. A foreign component is a `Ui.Component p` bound
with `foreign pure`; React renders it, Flick never calls it.

## Memoized views

`Ui.lazy_view view model` delays a pure subview until React renders it and
uses the model's `Eq` instance to skip equal inputs. `Ui.lazy_view_with
equal view model` supplies a pure, curried comparator instead:

```flick
count_label : Number -> Ui none none
let count_label n = Ui.text ("count " ++ show n)

label : Number -> Ui none none
let label n = Ui.lazy_view count_label n
```

Pass a stable view function, usually a top-level definition. A new closure
is a new view and renders again even when the models compare equal; this
keeps handlers from retaining an old closure. A custom comparator must
account for everything the view and its handlers use, not just visible
text. Models are immutable snapshots; replace them rather than mutating
foreign objects in place. A record or tuple carries several inputs, so
separate two- and three-argument lazy helpers are unnecessary.

`Ui.keyed id (Ui.lazy_view view model)` gives a dynamic sibling a stable
identity. Keys are local to a sibling list and must be unique there.
Reordering preserves both the React subtree and the event policy state of
that key; it must not start another task for a busy `exhaust` handler.


## Handlers and policies

A handler field has the opaque type `Ui.Handler snapshot r e`, which only a
policy of `Ui.Event` makes, so no handler is fire-and-forget:

| Policy | An event that comes while earlier tasks run |
| - | - |
| `exhaust f` | is ignored |
| `latest f` | interrupts the running task, waits until it stopped, then runs; one event waits at most, the newest |
| `queue capacity f` | waits, `capacity` events at most; beyond that it is rejected |
| `concurrent limit f` | runs, `limit` tasks at most; beyond that it is rejected |

`capacity` and `limit` are positive integers: a constant that is not one is
`FLICK-U001`, a computed one is checked when the handler is made. A
rejected event is an infrastructure defect for the supervisor, never an
error of `e`. `f` gets an inert snapshot taken inside the DOM callback —
`ClickSnapshot` (`button`, `alt`, `ctrl`, `shift`, `meta`), `InputSnapshot`
(`value`), or the item of a `select`. `Ui.Event.prevent_default decide h`
calls the pure `decide` on the snapshot before the callback returns and
prevents the default action when it says so; otherwise the default action
happens.

Handlers keep their captures: a button whose handler holds a file of a
scope cannot leave that scope (`FLICK-G001`). Render is pure, so `Ref.get`
in a view is a runtime call where none is allowed (`FLICK-E001`).

An event prop keeps the same listener across committed renders, but uses
the current handler. Removing the prop or unmounting its element detaches
that listener and drops events still waiting in its policy queue. Tasks
already started remain root-owned: they may finish, and closing the root
still interrupts and joins them.

## Running a root

```flick
run :
  forall c r e m.
  Ui.Dom.Container -> (Failure e ->{r; none} Unit)^c -> Ref m -> (m -> (Ui r e)^c)^c ->{r; none} Never
```

`Ui.Dom.run container supervisor model view` renders `view` of the contents of
`model`; handlers replace the model with `Ref.set`. The initial DOM mount
is synchronous. Later task completions request a React render: React 19
can coalesce pending updates, and an unchanged model identity skips the
root view. Do not rely on a DOM commit after every task or on intermediate
models being painted. This is React scheduling, not a once-per-animation-frame
guarantee. Each task runs with the services of the `run` call. A failure of
a task, and each rejected event, goes to `supervisor`; an interruption by a
policy or by closing does not. A failure of the supervisor itself ends
`run` as a defect.

`run` lasts until the fiber running it is interrupted, usually by the end
of the scope that spawned it. Closing has two phases: React cleanup stops
taking events at once and unmounts the tree, removing its listeners; then
the handler tasks are interrupted and joined, supervisors included, and
only then `run` ends — so the scope releases its resources after the tasks
have stopped. `Ui.Dom.container id` finds the element to render into.

The adapter is `@flick-lang/runtime/ui`; `react` and `react-dom` 19 are
peer dependencies of the runtime, installed by the application.

DOM roots live in the separate `Ui.Dom` module. `Ui` itself imports only
the React-only adapter, so compiled `Ui.run_with` programs do not require
`react-dom`. DOM callers use `Ui.Dom.run` and `Ui.Dom.container`; there are
no aliases in `Ui`.

## Server-rendered initial views

Expose an initial-view function from Flick and evaluate its cold effect on
the server. Pass the resulting `Ui` to `uiRenderStatic` from
`@flick-lang/runtime/ui/react`, then serialize that React node with
`renderToString` from `react-dom/server`. Event bindings remain inert on
the server; the application does not need a second HTML implementation.

Mark its container with `data-flick-ssr`. `Ui.Dom.run` then uses
`hydrateRoot` instead of clearing the existing HTML. Server and client
must use the same initial model, keys and markup. Native textarea drafts,
focus and selection survive startup; adopt a pre-boot draft into the
Flick model before checking or submitting it. Mount the root before
starting background tasks that notify its event handlers.

The documentation header and Play/Transpile use this path. Without the
marker, ordinary client-only mounting is unchanged.

## Other React renderers

`@flick-lang/runtime/ui` is the React DOM adapter.
`@flick-lang/runtime/ui/react` contains the React-only node conversion,
memoization and task lifecycle; importing it does not load `react-dom` or
read `document` or `window`.

The latter exports `uiRunWith` and the renderer factory contract:

```typescript
type Renderer = () => {
  render(node: React.ReactNode): void;
  unmount(): void;
};
```

The factory is called when the root's Effect starts, not when its
description is built. Its `render` submits the supplied React node to the
chosen renderer; `unmount` synchronously detaches the root. The shared
runner stops taking events, unmounts, interrupts and joins task fibers
before returning. A renderer must unmount all its React components, not
just hide its surface.

Flick binds a factory as an opaque `Ui.Renderer` through `foreign pure`
and runs it with `Ui.run_with renderer supervisor model view`. This uses
the same services, errors, captures and shutdown contract as `Ui.Dom.run`.
It is the extension point for React Native, Ink and React Three Fiber,
not an implementation of those adapters. Existing `div`, `input` and
other HTML helpers remain DOM-specific; non-DOM surfaces need their own
typed primitives and event snapshots, or foreign React components through
`Ui.component`. Do not import the DOM entry point in a non-DOM adapter.

## Elm-like, not yet a full TEA program

The current API has a pure view, a `Ref` model and supervised Effect
handlers. It does not yet provide a serialized `Msg → update` loop,
commands returned by `init`/`update`, or model-driven subscription
start/stop. See the [prioritized gaps](#prioritized-gaps) below. Effects,
fibers, scopes and event concurrency policies already exist; a future MVU
API should reuse them rather than introduce another asynchronous runtime.

### Comparison with Elmish

The comparison is with [`elmish/react` v6.x](https://github.com/elmish/react/tree/v6.x),
not with a complete MVU framework. Its
[`common.fs`](https://github.com/elmish/react/blob/v6.x/react/common.fs)
provides `lazyViewWith` backed by `React.memo` and sibling keys;
[`react.fs`](https://github.com/elmish/react/blob/v6.x/react/react.fs)
adds root-model identity checks, `requestAnimationFrame` batching,
hydration and `valueOrDefault`. The dispatch loop and subscriptions belong
to the separate [`elmish/elmish`](https://github.com/elmish/elmish) core.
Flick's React 19 batching does not promise Elmish's animation-frame schedule.

### Completed optimizations and evidence

- Root views skip unchanged model identities. React 19 coalesces pending
  renders instead of forcing a synchronous commit after every task; only
  the initial DOM mount stays synchronous.
- `Ui.lazy_view` uses the existing `Eq` instance; `Ui.lazy_view_with` uses
  a pure comparator. Captures of the model, view and both curried comparator
  stages stay in the resulting `Ui`. Replacing a view closure invalidates
  the memoized view, even for equal models.
- Keys preserve subtree identity and busy event-policy state on reorder.
  Component-owned stable listeners receive the current handler after
  commit, without retaining an obsolete view.
- `Ui.Renderer`/`Ui.run_with` share the React lifecycle without importing
  `react-dom`. `Ui.Dom` owns DOM mounting; tasks belong to the root, not
  to the DOM surface.

The implementation is in the
[React-only runner](https://github.com/bondiano/flick-lang/blob/main/runtime/src/ui/react/index.ts),
[DOM adapter](https://github.com/bondiano/flick-lang/blob/main/runtime/src/ui/index.ts)
and built-in modules
[`Ui`](https://github.com/bondiano/flick-lang/blob/main/flick-core/src/types/builtins/Ui.fl)
and [`Ui.Dom`](https://github.com/bondiano/flick-lang/blob/main/flick-core/src/types/builtins/Ui/Dom.fl).
Regression coverage is kept with the behavior:

- [`runtime/test/ui.test.ts`](https://github.com/bondiano/flick-lang/blob/main/runtime/test/ui.test.ts):
  memoization, batching, keys, stable listeners, StrictMode, supervision,
  unmount/join and importing the React-only entry point without a DOM.
- Lazy-view check fixtures
  ([pass](https://github.com/bondiano/flick-lang/blob/main/tests/check/pass/ui/lazy.fl),
  [fail](https://github.com/bondiano/flick-lang/blob/main/tests/check/fail/ui/lazy.fl)):
  purity, `Eq`, captures, services and the supervisor contract.
- [`tests/run/ui/counter.fl`](https://github.com/bondiano/flick-lang/blob/main/tests/run/ui/counter.fl):
  a custom `Eq` instance and renderer.
- [`tests/run/ffi/foreign.fl`](https://github.com/bondiano/flick-lang/blob/main/tests/run/ffi/foreign.fl):
  unary callback identity across FFI. The
  [compiler's foreign-call lowering](https://github.com/bondiano/flick-lang/blob/main/flick-core/src/codegen/js/items.rs)
  passes unary callbacks unchanged, avoiding a wrapper allocation;
  callbacks with several arguments still adapt the curried ABI.
- `compiled_ui_core_imports_without_a_dom_renderer` in
  [`flick/tests/e2e.rs`](https://github.com/bondiano/flick-lang/blob/main/flick/tests/e2e.rs):
  imports a compiled Flick module while rejecting `react-dom` imports,
  rather than checking only the runtime's TypeScript entry point.

### Prioritized gaps

These capabilities are **not implemented**. The order accounts for future
React Native, Ink and React Three Fiber support: moving DOM tags alone
does not create another backend.

| Priority | Capability | Minimum contract and reason |
| - | - | - |
| P1 | Renderer-independent MVU program | `init`, pure `update`, stable `dispatch` and a serialized message queue, including reentrant dispatch. Views do not mutate `Ref` directly; DOM, Native, Ink and R3F share one loop. |
| P1 | Commands from `init`/`update` | Run existing Effect tasks and map results to messages, reusing supervision and interrupt/join. Do not build a second `Cmd` runtime over Effect. |
| P1 | Model-driven keyed subscriptions | Diff/start/stop subscriptions by stable identity, reject late events and use scope-owned cleanup. Timers, sockets, terminal input and animation sources must update the model without waiting for a UI handler to finish. |
| P1 | Typed host primitives and events | Separate HTML constructors from renderer-specific `View`/`Text`, Ink host nodes and R3F scene nodes. Styles, numeric props and event snapshots must not assume DOM `target.value` or `preventDefault`. |
| P1 | Real Native / Ink / R3F adapters | Build on `Renderer` and host primitives: registration/mounting, updates, synchronous detachment and resource shutdown. Define R3F's MVU/frame-loop/invalidation relationship explicitly; do not send 60 messages per second unnecessarily. |
| P1 | Controlled fields and value synchronization | `input`/`textarea` currently supply only an initial `defaultValue`. Reset/external updates, caret/IME preservation and asynchronous-handler behavior need defined contracts. Elmish's `valueOrDefault` is a reference, not an existing controlled-input model. |
| P2 | Typed React interop | Foreign-component children/slots, scoped refs, Context/providers and lifecycle with defined resource release. Currently only opaque `Ui.component` with props is available. |
| P2 | MVU diagnostics and development | Message/model tracing, replay of pure updates and state-preserving HMR. Build on the MVU contract, not on a global mutable singleton. |

Separate `lazyView2`/`lazyView3` helpers are unnecessary: records or tuples
already carry several inputs. Existing Effect, fibers/scopes, bounded event
policies, supervision, escaping checks and the safe text channel are not
missing capabilities and need no duplicate APIs.
