# Reatom v1001: research notes for Flick's reactive UI layer

Sources, primary only, with nothing executed:

- Branch `v1001` of `reatom/reatom` at commit `aee4f572c251fab2e707ba04090d7a04b3acaac3`.
- https://v1001.reatom.dev, whose pages are built from `docs/src/content/docs/**` in the same repo.

Citation keys:

- `core:X` means `packages/core/src/X` and `react:X` means `packages/react/src/X`. Any other path
  is relative to the repo root.
- Permalink: `https://github.com/reatom/reatom/blob/aee4f572c251fab2e707ba04090d7a04b3acaac3/<path>`.
- `d:/X` means `https://v1001.reatom.dev/X`.
- **UNVERIFIED** marks an inference that no source confirms.

## 1. Core model

### 1.1 One primitive: the atom

- Everything is an atom: a function plus `__reatom` metadata (`reactive`, `middlewares`,
  `pipeline`, `onConnect`) `core:core/atom.ts#L32-L87`.
- An **action** is an atom with `reactive: false`. Its state is an array of calls
  `{params, payload}` that is cleared in the `cleanup` queue `core:core/atom.ts#L986-L994`,
  `core:core/action.ts`.
- `computed(fn)` is an atom that cannot be written: `set = undefined` `core:core/atom.ts#L1349-L1361`.
- `effect(cb)` is a `computed` extended with `withAbort(), withDynamicSubscription()`. It
  **subscribes to itself when created** and exposes `unsubscribe` `core:methods/effect.ts#L56-L97`.
- Reads are calls (`a()`). Writes are `a.set(v | fn)`, which stashes the params in a global slot
  and then calls the atom `core:core/atom.ts#L1113-L1143`. Calling an atom with arguments throws
  ("use .set instead") `core:core/atom.ts#L1167-L1295`.
- Middleware chain: `[userFn, computedMiddleware | actionMiddleware, cacheMiddleware, ...custom]`.
  - `_recompile` precompiles the chain after each `extend`.
  - An untouched atom keeps `pipeline === null` and takes a monomorphic fast path
    `core:core/atom.ts#L997-L1004`.

### 1.2 Frames, context, isolation

- **Frame.** State lives in a Frame `{state, error, pubs, subs, root, run, 'var#…'}`, not on the
  atom. A `RootState` holds `store: WeakMap<Atom, Frame>` and four queues
  `core:core/atom.ts#L203-L344`.
  - `context.start()` creates a new root: a new store and new queues.
  - `context.reset()` swaps the default root `core:core/atom.ts#L353-L446`.
- **Single-context fast path.** While only one context exists, frames are cached on `meta._frame`
  and the WeakMap is skipped. After a second `context.start`, the store becomes the source of truth
  `core:core/atom.ts#L462-L494`. Isolation costs nothing until you use it.
- **The stack.** The current frame lives on a global `STACK`, managed push/run/pop
  `core:core/atom.ts#L436-L450`.
  - `top()` throws `missing async stack` when the stack is empty.
  - `clearStack()` empties the stack on purpose, so any call made outside an explicit context
    fails `core:core/atom.ts#L1476-L1493`.
  - The React README recommends `clearStack()`, then `context.start()`, then a Provider
    `packages/react/README.md#setup-context`.
- **Frames are copied on change.** `_copy` clones the frame and sets `pubs[0] = null` (dirty)
  `core:core/atom.ts#L497-L516`.
  - `pubs[0]` is both the actualization flag and the **imperative write cause**: the frame that
    called `.set`.
  - `pubs[1..]` are the dependency frames `core:core/atom.ts#L219-L224`, `#L923-L980`.
  - The previous frame of each atom is kept in `root.frames` (`getPrevState`)
    `core:methods/context.ts`.

### 1.3 `wrap`: keeping the context across async code

- `wrap(fn)` returns a function that runs in the captured frame. It first checks that the root was
  not reset and that no abort is pending `core:methods/wrap.ts#L47-L75`.
- `wrap(promise)` "seals" settlement, so code after `await wrap(x)` runs with the frame restored
  `core:methods/wrap.ts#L76-L115`:

  ```ts
  queueMicrotask(() => void STACK.push(frame))
  cb()
  queueMicrotask(() => void STACK.pop())
  ```

  It also subscribes to `abortVar`, so the promise rejects when an ancestor aborts.
- Documented limit: the seal relies on native `async/await` microtask timing. A toolchain that
  targets below ES2017 breaks it ("missing async stack") `d:/handbook/history/` ("Build Target
  Compatibility").
- The authors' rules `summary.md` ("wrap rules"):
  - Wrap every async boundary.
  - Don't chain after `wrap`: `await wrap(fetch(url)).then(...)` is wrong.
  - Don't wrap the callbacks you pass to Reatom hooks.
- `bind(fn, frame)` is a lighter `wrap` that ignores abort `core:core/atom.ts#L1510-L1514`.

### 1.4 Variables, abort propagation, `spawn`

- **`variable<T>()`** emulates TC39 `AsyncContext.Variable`.
  - `run(value, fn)` is an *action* that writes `frame['var#name']`.
  - `find` **walks `frame.pubs` recursively**, so lookup follows the cause and dependency graph,
    not lexical scope `core:methods/variable.ts#L211-L230`.
  - The JSDoc pitches it as a DI container: token = variable, provider = `.run`,
    inject = `.require()` `core:methods/variable.ts#L250-L345`.
- **`abortVar`** holds a `ReatomAbortController` `core:methods/abortVar.ts#L54-L120`.
  - For reactive frames the search starts at `pubs[1]` (`_findReactiveStartIndex = 1`). An abort
    therefore reaches a computed through its **dependencies**, not through its writer.
  - `abortVar.subscribe()` chains the current controller to every ancestor controller
    `core:methods/abortVar.ts#L122-L210`.
- **`spawn`** exists **only as `variable.spawn`** (e.g. `abortVar.spawn(cb)`).
  - It is an action at which `find` stops (`frame.atom === this.spawn`), so the work survives its
    parent's abort `core:methods/variable.ts#L168`, `#L217`.
  - There is no top-level `spawn` export `core:methods/index.ts`, despite the React README's "use
    `spawn` from the core package" `packages/react/README.md#unmount-behavior`.
- **`withAbort(strategy)`** `core:extensions/withAbort.ts#L67-L259`:
  - `'last-in-win'` (default): a new call aborts the previous ones.
  - `'first-in-win'`: a call made while another is still running throws an abort.
  - `'finally'`: children are aborted when the call settles.
  - `'manual'`.
- `race(...)` aborts the losers with the reason `'race'` `core:methods/abortVar.ts#L267-L294`.
- Debounce is just `await wrap(sleep(ms))` inside a `withAbort()` action `summary.md` ("withAbort
  strategies").

### 1.5 Dependency tracking, laziness, glitch-freedom

- **Tracking.** While a computed runs, `meta.linking = true`, and each atom it reads pushes its
  frame into the parent's `pubs` `core:core/atom.ts#L1059-L1061`, `#L1097-L1099`. Dependencies are dynamic: they
  are rebuilt on every run, and `relink` reconnects them only if the list changed
  `core:core/atom.ts#L646-L655`.
- **Pull validation.** An unsubscribed computed revalidates on every read. It compares each stored
  dependency frame's `state`/`error` with the fresh one using `Object.is`, and recomputes only if
  something differs `core:core/atom.ts#L828-L920`.
- **Push caching.** A subscribed computed is push-cached: `_mark` copies the dirty subscriber
  frames down the graph `core:core/atom.ts#L526-L553`. The review skill says the same:
  "Unsubscribed computeds revalidate on every read; subscribed ones are push-cached"
  `skills/reatom-review/SKILL.md`.
- **Glitch-freedom.** A write only marks frames dirty. Values are recomputed lazily on read, and
  subscribers run in the `effect` queue after every compute.
  - The diamond test runs each of its 5 computeds once per change and notifies once
    `core:core/diamonds.test.ts`.
  - **UNVERIFIED** as a formal guarantee: the docs don't state it; I inferred it from the code and
    that test.
- **Cycles.** A `processing` counter, deferred recomputation, and up to 10 retries before
  `Stuck in recursion` `core:core/atom.ts#L1012-L1106`.

### 1.6 Scheduling, batching

- **Queues.** Four queues drain in priority order: `hook → compute → cleanup → effect`. Priority is
  rechecked after every task, and each root schedules one microtask `core:core/queues.ts`.
- **Docs view.** The docs describe nested loops `d:/handbook/lifecycle/`: updates, then hooks
  (`withChangeHook`/`withCallHook`), then computations, then cleanups, then effects (`subscribe`,
  `schedule`, `withConnectHook`).
- **Consequences:**
  - `subscribe` callbacks run asynchronously (in a microtask) unless you call `notify()`
    `skills/reatom-review/SKILL.md`.
  - `batch(cb, shouldNotify)` nests and flushes synchronously at depth 0 `core:core/queues.ts#L55`.
  - React handlers call `notify()` after each call `react:reatomComponent.ts#L51-L85`.
- **Errors.** An error thrown inside a queue task is swallowed into
  `console.error('Unhandled error in Reatom queue!')` `core:core/queues.ts#L100-L104`.

### 1.7 Lifecycle (v1001's onConnect/onDisconnect)

- **`meta.onConnect`** is an action extended with `withAbort` `core:core/atom.ts#L596-L644`.
  - It is queued (in the `effect` queue) when the subscriber count goes from 0 to 1.
  - `onConnect.abort` is queued when the count returns to 0.
  - Both propagate transitively through `link`/`unlink`.
- **`withConnectHook(cb)`** registers on it `core:extensions/withConnectHook.ts`.
  - Anything wrapped and any effect started inside it is aborted on disconnect.
  - A returned function becomes the cleanup, run via `abortVar.spawn`.
  - `withDisconnectHook(cb)` is `withConnectHook(() => () => cb())`.
- Connections are one-directional: a connect hook on a derived atom does not fire when only its
  source is subscribed `d:/handbook/lifecycle/`.
- Pitfall: if a connect-hook effect reads the hook's own target, it can loop forever between
  connect and subscribe `skills/reatom-review/SKILL.md`.
- v3 renames: `onConnect`→`withConnectHook`, `reaction`→`effect`,
  `onCtxAbort`→`abortVar.subscribe`, `ctx.spy`→`a()`, `ctx.get`→`peek` `d:/handbook/history/`.

## 2. Extensions

### 2.1 Mechanism and typing

- **`extend(...exts)`** calls `ext(target)` for each extension `core:core/extend.ts#L119-L142`.
  - If the extension returns `target`, it was a middleware extension.
  - If it returns an object, the object's keys are merged into the same function object.
  - Returning a different atom throws, and so does overriding an existing key:

  ```ts
  if (key in this && (this as AtomLike & Rec)[key] !== result[key])
    throw new ReatomError(`extension can not override existing methods: ${key}`)
  ```

- **Typing** `core:core/extend.ts#L24-L98`:
  - The extension types are `Ext<Target, Extension>`, `GenericExt`
    (`<T extends Target>(t: T) => T`, which preserves generics) and `AssignerExt<Methods>`.
  - `Extend<This>` is **10 hand-written overloads**. Extension *n* is typed against
    `Merge<This, [T1..Tn-1]>`, so later extensions see what earlier ones added.
  - Past 10 extensions, or on a mismatch, the result is a `{ extend_ERROR: '…' }` sentinel type.
- **Authoring rules** `d:/handbook/extensions/`: name the interface `NameExt`, use `Ext` for a
  specific target and `GenericExt` for any atom, and derive child names from `target.name`.
- **Middleware:**
  - `withMiddleware(cb, place)`, with `place` one of `'invalidation' | 'read' | 'computed'`
    `core:core/extend.ts#L200-L240`.
  - The last extension listed is the outermost wrapper `d:/handbook/extensions/` ("Middleware
    Order").
  - `withActionMiddleware` wraps only the user call `core:core/action.ts`.
  - `withParams` reshapes the call signature `core:core/extend.ts`.
  - `addGlobalExtension` applies to atoms created later and to registered global atoms
    `core:core/atom.ts#L822-L825`.

### 2.2 Async

- **`action(fn).extend(withAsync(opts))`** `core:async/withAsync.ts#L42-L180`:
  - Adds `ready`, `pending`, `error`, the `onFulfill`/`onReject`/`onSettle` actions, `params` and
    `retry`.
  - `status: true` and `cacheParams: true` are opt-in; both default to off for performance and
    memory reasons.
  - Without `cacheParams`, calling `retry` on an action throws at call time.
  - `withAsync` adds **no abort** by itself `summary.md` ("withAsync").
- **`computed(async …).extend(withAsyncData({initState, mapPayload}))`** is a superset that
  includes `withAbort` `core:async/withAsyncData.ts`. It is "the main recommended way to fetch
  data", and fetching manually in an `effect` is an antipattern `summary.md`.
- **Retry:** `retryComputed(target)` resets the dependencies and re-reads `core:methods/retry.ts`.
  `withSuspenseRetry` re-runs actions that touched a suspended atom `d:/handbook/suspense/`. I
  found no backoff retry extension in core.
- **Cache/SWR:** `withCache` takes `length = 5`, `staleTime = 5 min`,
  `swr: bool | {shouldFulfill, shouldReject}`, `paramsToKey | isEqual`, `ignoreAbort` and
  `withPersist`. It is a `reatomMap` of records with `invalidate`, `setWithParams` and
  `deleteWithParams` `core:extensions/withCache.ts#L60-L130`.
- **Transactions:** `withRollback()` on atoms plus `withTransaction()` on actions roll back on
  error, **not on abort** `summary.md` ("Transactions notes"), `core:methods/transaction.ts`.

### 2.3 Persist, changes, history

- **`reatomPersist(storage)`** produces a `withX(key | options)` extension
  `core:persist/index.ts#L94-L140`. Options: `key`, `toSnapshot`/`fromSnapshot`, `schema`
  (Standard Schema), `version` + `migration`, `time` (TTL) and `subscribe`.
- **Adapters:** local and session storage, IndexedDB, BroadcastChannel, cookie and cookieStore
  `core:persist/web-storage/index.ts`. They **fall back to memory** when browser APIs are missing
  (SSR, Node), when storage is disabled, or when the quota is exceeded `d:/handbook/persist/`.
- **Change hooks:** `withChangeHook` and `withCallHook` (hook queue), `withErrorHook`, `withTap`
  `core:extensions/withChangeHook.ts`. The docs say not to sync atoms with `withChangeHook`; use
  `computed`/`withComputed` `summary.md`.
- **Undo/history:** there is **no built-in history extension** in v1001. `withHistory` appears
  only as an authoring example `d:/handbook/extensions/`. The only built-in revert is
  `withRollback`.

## 3. React binding (`@reatom/react`)

- **Rendering model.** `reatomComponent(Component, name | {name, deps, abortOnUnmount})` sits on
  the framework-agnostic `reatomAbstractRender` `core:reatomAbstractRender.ts#L71-L174`.
  - The component renders inside a `computed` named `_${name}`, with props in a `_props` atom.
  - Every atom read during render becomes a dependency.
  - Outside of render, the computed only re-reads its old dependencies to keep them alive, then
    calls `useState` to trigger a rerender.
- **Subscription timing.** It subscribes in **`useLayoutEffect`**, not `useEffect`, because a
  dependency that resolves purely through microtasks could otherwise settle before the
  subscription and leave the component stale forever `react:reatomComponent.ts#L139-L150`. This
  was fixed in 1001.0.1 `packages/react/CHANGELOG.md`.
- **1001 breaking change.** `abortOnUnmount` now defaults to `false` `packages/react/CHANGELOG.md`,
  `react:reatomComponent.ts#L103`.
  - The README's "Unmount Behavior" section still says unmount aborts the context
    `packages/react/README.md#unmount-behavior`, so it is stale.
  - `reatomFactoryComponent` still aborts its init action on unmount
    `react:reatomComponent.ts#L166-L219`.
- **StrictMode/Activity.**
  - `recheckAbort` replaces an aborted controller on remount ("Related to react remounts of
    `StrictMode` and `Activity`") `core:reatomAbstractRender.ts#L94-L107`.
  - `reatomFactoryComponent` **re-runs the factory** via `recreate` when its instance was aborted,
    so in dev the factory can run twice `react:reatomComponent.ts#L190-L211`.
- **Suspense.**
  - A thrown Promise (or a "Suspense Exception") is caught and re-thrown after `React.use(promise)`
    when `use` exists `react:reatomComponent.ts#L87-L157`.
  - Core's `withSuspense()` adds `.suspended()`, and `preserve: true` keeps the old value during a
    refresh.
  - The docs recommend Suspense **only for global init state**: auth, settings, flags, i18n
    `d:/handbook/suspense/`.
- **Hooks.** `useAtom(atom | fn | init, deps, opts)` is built on `useSyncExternalStore`.
  `useAction` and `useWrap` return a stable identity that points at the latest closure, updated in
  `useInsertionEffect` `react:hooks.ts`, `react:reatomComponent.ts#L51-L85`.
- **Devtools names** `react:reatomComponent.ts#L21-L35`, `#L111`, `#L161`.
  - The name comes from the 2nd argument, falling back to `named('Component', Component.name)`,
    and is set as `render.name` via `defineProperty`.
  - Unnamed hooks read the owner component from React internals
    (`__CLIENT_INTERNALS…A.getOwner()` / `ReactCurrentOwner`).
- **SSR/hydration.** The whole SSR guide reads: "Reatom has all features to give simple and
  powerful SSR experience with isomorphic code. **The docs under development**." `d:/guides/ssr/`
  - Available pieces:
    - per-request `context.start()` `d:/handbook/history/` ("Migration from v3")
    - `reatomContext.Provider` `packages/react/README.md#setup-context`
    - per-atom `toJSON` / `withToJson` / `withFromJson` `core:extensions/withToJson.ts`
    - "SSR-compatible" cookie persistence `d:/handbook/persist/`
  - On the server, `urlAtom.init` throws "window is undefined, you should setup urlAtom manually."
    `core:web/url.ts#L128-L134`.
  - There is **no documented dehydrate/hydrate API**. Whether one exists outside core and react is
    **UNVERIFIED**.

## 4. Forms, routing, fullstack

- **Forms.** `reatomField(init, opts)` `core:form/withBaseField.ts#L43-L340`.
  - Options: `validate`, `validateOnChange`, `validateOnBlur`, `validateOnConnect`,
    `keepErrorOnChange`, `keepErrorDuringValidating`, `disabled`, `getValue`, `isDirty`.
  - `focus` is `{active, dirty, touched}`.
  - `validation` is `{error, triggered, validating}`, with an abortable `trigger` and `errors` that
    each carry a `source`.
  - A field also has `reset`, `disabled` and `elementRef`.
  - `reatomFieldSet`, `reatomFieldArray` and `reatomForm(fields, {schema, onSubmit, validateOnBlur,
    name})` build on fields `core:form/`.
  - Validators track the atoms they read after the first trigger, and async validators run
    abortably `summary.md` ("Forms").
  - The docs recommend creating forms inside computed factories or route loaders, not as module
    singletons `d:/handbook/computed-factory/`, `skills/reatom-review/SKILL.md`.
- **Routing.** `reatomRoute(path | {path, search, params, loader, render, layout})`.
  - A route is an atom of its params or `null`, with `.go`, `.path`, `.exact`, `.match` and nested
    `.reatomRoute(child)`.
  - Params and search are validated and transformed with Standard Schema. A route whose validation
    fails is `null` `summary.md` ("Routing").
  - `loader` is `computed(async…)` + `withAsyncData({status: true})`. It awaits the parent loader
    and aborts when you navigate away `core:routing/route.ts#L398-L420`.
  - Guards return `null` from `params()`. Routes with no path ("modal gates") keep their state in
    memory `summary.md`.
  - `urlAtom` intercepts link clicks. Its `sync` can be swapped for `noop` `core:web/url.ts#L49-L72`.
- **Computed factory.** A computed that *returns atoms* gives "globally reachable state whose
  lifetime is tied to a meaningful unit of work" `d:/handbook/computed-factory/`.
  - Pitfall: any read inside the factory can rebuild the inner model.
  - Fixes: split the inputs, use `peek`, or use `memo`.
- **Fullstack.** v1001 has no server/RPC layer. The server-side pieces are per-request
  `context.start`, cookies, and OTel `traceparent` propagation in `@reatom/opentelemetry`
  `docs/src/content/docs/guides/opentelemetry.md`. That page returns 404 on the v1001 site.

## 5. Debugging and observability

- **`connectLogger({match})`** `core:methods/connectLogger.ts#L172-L379`.
  - It pushes a logging middleware into `EXTENSIONS`, so it covers **only atoms created after the
    call**. That is why the setup module must be imported first `d:/start/tooling/`.
  - Logs are enqueued in the `hook` queue *before* `next` runs, so they come out in call order.
  - Each entry shows new and previous state, `connected`, params and payload, and a cause tree.
- **"Why did this change."**
  - `getStackTrace(frame)` walks `pubs` (the cause plus the dependencies) into a tree with
    `[#serial]` frame ids `core:methods/getStackTrace.ts`.
  - The test expects `'─log\n └─ offset\n    └─ page'` `core:core/action.test.ts#L13-L35`.
  - `isCausedBy(atom)` checks causality at runtime, e.g. to skip self-triggered updates
    `core:methods/isCausedBy.ts`.
- **Naming.**
  - The name is the 2nd argument everywhere. Unnamed atoms become `atom#N` / `action#N`
    `core:core/atom.ts#L792-L797`.
  - Names should be dotted and structural, e.g. `users#${id}.name`.
  - A segment starting with `_` hides the atom from logs and traces (`isSkip`)
    `core:methods/getStackTrace.ts#L4`, `skills/reatom-review/SKILL.md`.
  - `anonymizeNames()` strips names `core:core/atom.ts#L1149`.
- **`log`, `log.label`, `log.state`** are dev-only logs that carry the trace. They do nothing until
  `connectLogger` runs `d:/start/tooling/`.
- **Devtools.** The tooling page says "We will publish our devtools soon" `d:/start/tooling/`, but
  the repo already ships `@reatom/admin` 1000.15.1 `packages/admin/README.md`.
  - `createAdminDevtools` gives activity, timeline, a **causal graph**, a state explorer and replay
    export.
  - I read only its README, not its code.
- **ESLint.** The docs suggest a `no-restricted-syntax` rule that requires `wrap` on every `await`
  inside `action`/`computed`/`effect` `d:/start/tooling/`.

## 6. Testing

- v1001 has no `@reatom/testing` package; there is no such directory under `packages/`. The test
  tools live in core:
  - `context.start()` / `context.reset()` for isolation `core:core/atom.ts#L353-L406`.
  - `mock(target, cb)` inserts a middleware next to the kernel, so extensions still process the
    mocked payload, and returns an `unmock` `core:core/atom.ts#L1536-L1564`.
  - `createMemStorage` for persist `core:persist/`.
  - Variables for DI mocks `core:methods/variable.ts#L250-L345`.
- Pitfalls:
  - Subscribers fire in a microtask, so a test must `notify()` or `await`. The repo's own tests
    call `notify()` `core:core/diamonds.test.ts`.
  - Tests that share the default context need `context.reset()` in `beforeEach`
    `skills/reatom-review/SKILL.md`.

## 7. Documented pitfalls and tradeoffs

- **Losing the context.** It happens after an unwrapped `await`, in `.then` chains, DOM refs and
  observers, and in code transpiled below ES2017 `summary.md`, `d:/handbook/history/`,
  `skills/reatom-review/SKILL.md`.
- **Module-level `effect`.** It connects eagerly at import and never disconnects. A component that
  re-subscribes to it leaks `skills/reatom-review/SKILL.md`.
- **Memory.**
  - `cacheParams` and `status` are opt-in to avoid leaks `core:async/withAsync.ts#L168-L178`.
  - `abortVar.subscribe` needs `unsubscribe` or `using`, "otherwise a memory leak will occur"
    `core:methods/abortVar.ts#L117-L150`.
  - Factories invalidate but don't free on disconnect; use `withDisconnectHook` for that
    `d:/handbook/computed-factory/`.
- **Global atoms vs frames.** An atom's identity is global, while its state is per context. The
  core is a **singleton** (a global `STACK`), so loading two versions throws "can't use different
  versions" `core:core/atom.ts#L411-L417`, `d:/handbook/history/` ("Package Updates").
- **Closures.** `memo` keeps only its first callback ("use stable closures") `summary.md`. The React
  hooks avoid stale closures by mutating a ref `react:hooks.ts`.
- **Abort noise.** Aborted loaders and fetches surface as unhandled rejections, which the docs call
  "not a bug" `summary.md`.
- **Suspense.** Use it for global init only. For fine-grained loading, error and cancellation
  handling it "becomes much more complex" `d:/handbook/suspense/`.

## 8. Ideas for Flick

Each item maps a Reatom mechanism that runs at runtime to something Flick could guarantee statically.

1. **Context → a `Reactive r` effect plus one region per root.** Reading or writing an atom requires `Reactive r` in the effect row. The root runner is `runRoot : (∀r. Eff {Reactive r | e} a) -> Eff e a`. An `Atom r a` cannot escape its root, so per-request SSR isolation holds by construction. This replaces the global `STACK`, `clearStack` and the runtime "missing async stack" error.
2. **`wrap` → nothing.** Fibers carry their context (FiberRefs). That removes the microtask seal, the ES2017 constraint and the wrap lint. Raw JS callbacks enter only through an FFI boundary that captures the context.
3. **`abortVar`/`withAbort` → structured concurrency.** Fiber interruption replaces abort controllers. The `withAbort` strategies become typed combinators over a fiber slot. `spawn` becomes a daemon fork that needs an explicit `Detach` effect.
4. **Abort through dependencies vs through causes → two kinds.** A `Derived` value is pure plus `Reactive` and is interrupted when its inputs' scope ends. A `Command` is `Reactive` plus effects and is interrupted when its caller's scope ends.
5. **computed/action split → row purity.** Reatom only recommends this split; Flick can enforce it. A computed cannot `Write` or do IO, except a `Fetch` marked idempotent. `.set` is allowed only in a `Command`.
6. **`withConnectHook` → a `Scope` tied to the subscriber count.** Finalizers come from that scope. The "effect reads its own connect target" loop becomes a static error if the hook's read set excludes its target.
7. **Module-level `effect` leak → effects are values.** An effect must be run in a scope (route, component or init). A side effect at import time cannot be expressed.
8. **`extend` → row-typed records.** This replaces Reatom's 10 overloads, its `extend_ERROR` type and the runtime check for key collisions. An extension is typed `Atom s a -> {| ext | ρ}` with a lacks-constraint, so collisions become type errors and there is no limit on the number of extensions. Middleware order matters, so middleware extensions form an ordered, typed list.
9. **Opt-in members → the type says which members exist.** `retry` exists iff `CacheParams` is in the record; Reatom instead throws at call time. Errors are typed through the effect's error channel.
10. **Keep the glitch-free hybrid.** Pull revalidation by dependency equality, push marking for subscribed atoms, and effects after computes. Effect's `Atom` works similarly, and purity makes "recompute = re-read" sound.
11. **Causes → provenance by construction.** Reatom walks `pubs[0]` for logs, `isCausedBy` and variable lookup. Flick knows every `.set` site, so each write can carry a source span plus the fiber or span id. That gives cause trees with source locations, and OTel through Effect tracing.
12. **Names → generated by the compiler.** Derive `module.binding` from the AST, plus `#key` for each factory instance. The `_` hot-path prefix becomes an attribute, and `anonymizeNames` becomes a build flag.
13. **Computed factory → a region per scope key.** Inner atoms live in a sub-region that is destroyed when the key changes. With `Factory k (∀r. Model r)`, a stale inner handle cannot be expressed.
14. **Subscribe-on-read components → inferred read sets.** Compute read sets statically where possible, with dynamic tracking as the fallback. Event handlers require a `Command` context, which replaces "wrap your JSX handlers". Keep subscribing in `useLayoutEffect`, and keep factory re-runs StrictMode-safe by running setup in a `Scope`.
15. **Forms → `Form a` derived by a type class from a record type.** Validators are `Derived` effects, interruptible when async. Schema errors map onto record paths.
16. **Testing → handlers instead of `mock` + `context.reset()`.** Use layers per test root and one region per test. A test scheduler drains the queues deterministically.
17. **SSR (a gap in Reatom).** A `Serializable` constraint on atoms in a client-shared region would generate dehydrate and hydrate. Effects that are server-only (cookies, `urlAtom` set from the request) are kept out of client bundles by row checks.

## Unverified / not checked

- Glitch-freedom as a guarantee. I inferred it from the code and `diamonds.test.ts`; it is not
  documented.
- Whether any SSR dehydrate/hydrate API exists. I checked only core, react and the docs, and the
  SSR guide is a stub.
- `@reatom/admin` features. I read the README only.
- Full parity between the published site and `docs/src/content`. I spot-checked it:
  - the key URLs return 200
  - the SSR text matches
  - `guides/opentelemetry` returns 404
- `withCache` SWR runtime semantics, and `withAsyncData` internals beyond its option types and
  JSDoc.
- React 19 `use()` and `<Activity>` paths. I read them in code comments only. No code was run.
