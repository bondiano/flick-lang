# Effect v4 reactivity, fullstack, observability and CLI: primary-source notes

Researched 2026-10-24 for Flick's reactive UI layer. Pinned target: `effect@4.0.5` + `@effect/atom-react@4.0.5`. npm `latest` for both is now `4.0.6`.

**Citation keys**
- `E:` = `/home/dev/worktrees/vkusori/vkusori-herdr/node_modules/.pnpm/effect@4.0.5/node_modules/effect/` (read only)
- `AR:` = `.../node_modules/.pnpm/@effect+atom-react@4.0.5_effect@4.0.5_react@19.3.0_scheduler@0.28.0/node_modules/@effect/atom-react/` (read only)
- `gh:` = github.com/Effect-TS/effect at tag `effect@4.0.6`
- `web:` = effect.website/docs/v4/api/...

## 0. Where v4 lives, and stability

- **Repository.** v4 source is now in **github.com/Effect-TS/effect** (`packages/effect`, `packages/atom/react`, ...). The effect-smol README says "Effect V4 Has Moved ... This repository is archived and read-only", and the full history was merged into `Effect-TS/effect` (`gh api repos/Effect-TS/effect-smol/readme`). (`gh repo view` still says `isArchived: false`). `E:package.json` `repository` = `Effect-TS/effect`, `packages/effect`.
- **No `unstable/` path in 4.0.5.** The subpath exports are `effect/reactivity`, `effect/cli`, `effect/devtools`, `effect/observability`, `effect/rpc`, `effect/http`, `effect/http-api` and others (`E:package.json:32-62`). Pre-release `effect/unstable/*` import paths are not in the 4.0.5 export map. Use `effect/reactivity`, not `effect/unstable/reactivity`.
- **Stability is a JSDoc tag.** It is written as `@stability unstable|stable`. All barrel entries of `reactivity`, `cli`, `devtools`, `observability`, `rpc`, `http`, `http-api` `index.ts` are `@stability unstable`. Core `Tracer.ts`, `Metric.ts` and `Logger.ts` are mostly `stable`: Logger has 29 stable and 3 unstable entries, Effect.ts 525 stable and 4 unstable (counted with grep in `E:src/`). The web reference marks the Atom module and all 78 of its exports "unstable" (`web:effect/reactivity/Atom`), and every hook "unstable" (`web:atom-react/Hooks`). All of `AR:src/*.ts` is `@stability unstable`.
- **Sibling packages** (from `npm view`, all `latest` 4.0.6 with peer `effect ^4.0.6`): `@effect/atom-react` (peers `react >=19 <20`, `scheduler >=0.25 <0.29`, repo dir `packages/atom/react`), `@effect/atom-solid`, `@effect/atom-vue`, `@effect/opentelemetry`, `@effect/platform-node`, `@effect/platform-browser` and `@effect/vitest`. The **v3-only** packages are `@effect/cli@0.77.2` (peer effect ^3.22.2), `@effect/experimental@0.61.1` (^3.22.1) and `@effect-atom/atom-react@0.7.0` (^3.22.1). Do not use these with v4.
- **4.0.6 delta.** It adds a `skipInitialRun` option to `Atom.serializable` and `Atom.initialValue`: a seeded effect or stream atom then keeps its hydrated value instead of running on mount (`gh:packages/atom/react/CHANGELOG.md` 4.0.6, PR #9522; `web:effect/reactivity/Atom`). This option is **absent in 4.0.5**.

## 1. Atom (`effect/reactivity` → `Atom`, `E:src/reactivity/Atom.ts`)

### Model
```ts
interface Atom<A> { keepAlive: boolean; lazy: boolean; read: (get: AtomContext) => A
  equals(a: A, b: A): boolean; refresh?; label?; idleTTL?: number; initialValueTarget?: Atom<A> }   // :71-81
interface Writable<R, W = R> extends Atom<R> { write: (ctx: WriteContext<R>, value: W) => void } // :163-166
```
- An atom is a plain, immutable **description**. State lives in a registry `Node` keyed by atom identity. For serializable atoms the key is the `key` string instead (`AtomRegistry.ts:483-484`).
- **Every combinator returns a new object**: `keepAlive`, `autoDispose`, `setIdleTTL`, `serializable` and `withServerValue` all copy with `Object.assign(Object.create(proto), {...self, ...})` (`Atom.ts:263-271, 1742-1763, 3296-3305, 3343-3347`). So a non-serializable atom created during render is a different node on every render.
- `equals` defaults to `Object.is` (`Atom.ts:278`). Override it with `withEquality` (`:1838`). Equal values skip notification (`AtomRegistry.ts:962`).

### AtomContext (the reader passed to `read`) (`Atom.ts:175-207`)
Tracked `get(atom)` / `get.get(atom)`; untracked `once`; `result/resultOnce` (AsyncResult atom as Effect, `suspendOnWaiting`); `some/someOnce` (Option); `addFinalizer`; `mount`; `refresh/refreshSelf`; `self()` (previous value, Option); `setSelf` (async push); `set`; `setResult` (set + await); `stream/streamResult`; `subscribe(atom, f, {immediate})`; `registry`.

`WriteContext` offers only `get / refreshSelf / setSelf / set` (`:216-221`).

The implementation is a per-build **Lifetime** (`AtomRegistry.ts:1105-1288`). After the lifetime is disposed, `get` falls back to an untracked `registry.get`, and `set/setSelf/refresh/mount/subscribe` become no-ops (`:1120-1245`). Writes from a stale fiber are dropped.

### Constructors
- `readable(read, refresh?)` and `writable(read, write, refresh?)` produce lazy atoms that are not kept alive (`Atom.ts:416-449`).
- `make` is overloaded (`:464-597`):
  - `make(value)` → `Writable<A>` state
  - `make((get) => A)` → `Atom<A>`
  - `make(Effect<A,E, Scope | AtomRegistry>)` or `make(get => Effect)` → `Atom<AsyncResult<A,E>>`, with options `initialValue` and `uninterruptible`
  - `make(Stream<A,E, AtomRegistry>)` → `Atom<AsyncResult<A, E | NoSuchElementError>>`

  Without a runtime, an effect atom may require **only** `Scope | AtomRegistry`.
- `fn` (`:1348-1437`): `fn<Arg>()(f: (arg, get: FnContext) => Effect<A,E,Scope|AtomRegistry> | Stream, {initialValue?, concurrent?}): AtomResultFn<Arg,A,E>`. This is `Writable<AsyncResult<A,E>, Arg | Reset | Interrupt>` (`:1291`). Writing an Arg bumps a counter and re-runs (`:1460-1472`). `Reset` returns it to the initial state. `Interrupt` gives `Failure(Cause.interrupt())` (`:1410-1412`). Without `concurrent`, a new write disposes the previous lifetime and so interrupts the previous fiber. With `concurrent`, fibers are forked detached, collected and joined (`:1414-1425`). Inside `fn`, `get` is **untracked** (`isFn`, `AtomRegistry.ts:1279`).
- `fnSync<Arg>()(f, {initialValue?})` gives `Writable<Option<A>, Arg>` (`Atom.ts:1215-1279`).
- `pull(stream, {disableAccumulation?})` gives `Writable<PullResult<A,E>, void>`. Each write pulls the next chunk, which suits infinite lists (`:1491-1597`).
- `subscriptionRef(ref | effect)` (`:1068`).
- `kvs({runtime: AtomRuntime<KeyValueStore>, key, schema, defaultValue, mode?})` is a schema-coded KeyValueStore entry: sync gives the value, async gives an AsyncResult (`:2859-2918`).
- `searchParam(name, {schema?})` gives `Writable<string | Option<T>>` bound to the URL. On the server (`window` undefined) it returns `""` or `none`. The schema "has to be synchronous and have no context" (`:2925-2960`).
- `windowFocusSignal` and `refreshOnWindowFocus` (`:2788-2840`).

### family (`Atom.ts:1603-1640`)
`family<Arg, T extends object>(f: (arg) => T): (arg) => T` memoises per arg in a `MutableHashMap`, holding `WeakRef`s cleaned by a `FinalizationRegistry`. It falls back to strong refs when those are unavailable. Object keys are looked up by `Hash.hash` + `Equal` (`MutableHashMap.ts:405-417`), and `Equal` compares plain objects, arrays, maps and so on **structurally** (`Equal.ts:1-8, 128-134`). The family is global and module-level, shared by every registry.

### Lifetime, laziness and TTL combinators
- `keepAlive` / `autoDispose` flip `keepAlive` (`:1742-1763`).
- `setIdleTTL(d)`: a finite `d` gives `idleTTL = ms`; an infinite `d` gives `keepAlive = true` (`:230-271`).
- `setLazy(bool)`: "Lazy atoms defer recomputation while they have no active listeners or active non-lazy dependents" (`:1777-1790`).
- `withLabel(name)` is diagnostic only (`:1918-1925`).
- `initialValue(atom, a)` → tuple for `AtomRegistry.make({initialValues})` (`:1994`).
- `transform`, `map` and `mapResult` keep the write type of a writable source (`:2080-2230`).
- `withFallback` (`:1650`).
- `debounce(d)` (`:2275`), `withRefresh(d)` (`:2340`).
- `swr({staleTime, revalidateOnMount?, revalidateOnFocus?: boolean|"always", focusSignal?})` (`:2394`, `dist/reactivity/Atom.d.ts:1186-1199`).
- `optimistic(self): Writable<A, Atom<AsyncResult<A, unknown>>>`. Writes are transition atoms. On success the source refreshes; on failure the value rolls back (`:2528-2536`). `optimisticFn({reducer, fn})` gives `AtomResultFn` (`:2632`, `.d.ts:1259-1268`).
- `batch(f)` = `AtomRegistry.batch` (`:2770`). It collects stale nodes and notifications, rebuilds, then notifies once (`AtomRegistry.ts:1356-1394`). **`batchState` is module-global** (`:1346`), not per registry.

### Runtimes: binding the service row (`Atom.ts:743-956`)
- `Atom.runtime: RegistryRuntimeFactory = context()` (`:956`). `Atom.runtime(layer | get => layer)` gives `AtomRuntime<R, ER>`, an `Atom<AsyncResult<Context<R>, ER>>` with methods `.atom`, `.fn`, `.pull` and `.subscriptionRef`. Through those methods the effects may require `R | Scope | AtomRegistry | Reactivity` (`:743-822, 831-846`).
- The layer is built `uninterruptible` with `Layer.buildWithMemoMap`. By default the memo map is **an atom, so it is scoped per registry** (`:890-913`). The services therefore live once per registry, which suits per-request SSR. `context({memoMap: Layer.MemoMap})` instead shares one memo map across registries (`:866-886`).
- `factory.addGlobalLayer(layer)` mutates a factory-wide global layer (`:916-918`).
- `withReactivity(keys)` refreshes the atom when `Reactivity` keys are invalidated (`:931-940, 971`). `runtime.fn(f, {reactivityKeys})` wraps the effect in `Reactivity.mutation`, so a successful run invalidates those keys (`:355-381`). `Reactivity` itself is "Process-local invalidation" and does not cache (`Reactivity.ts:1-11`).

### How effect atoms run (`Atom.ts:663-737`)
1. Each build makes an unsafe `Scope` and registers a finalizer that closes it (`:671-674`).
2. It runs `runForkWith(services + Scope + AtomRegistry + Scheduler)`, flushes the dispatcher, and polls (`:676-680, 714-717`).
3. **A synchronous Exit returns the value directly with no Waiting flash.** Otherwise the build returns `waiting(previous | initial)`, and the fiber's observer `setSelf`s the final result (`:683-701`).
4. Cancelling removes the observer and, **unless `uninterruptible`, interrupts the fiber**. It is registered as a lifetime finalizer (`:693-695, 722-728`).

The lifetime is disposed on every **invalidation** (a dependency change or refresh) and on **node removal** (unmount / idle TTL) (`AtomRegistry.ts:995-1001, 1068-1072`). Re-running a stale effect atom therefore interrupts the in-flight fiber. Streams work the same way: they pull in a loop, `setSelf(success(last, {waiting: true}))`, and end as `Success` or `Failure(NoSuchElementError)` if the stream was empty (`Atom.ts:993-1056`).

### AsyncResult (`E:src/reactivity/AsyncResult.ts`)
```ts
type AsyncResult<A, E = never> = Initial<A,E> | Success<A,E> | Failure<A,E>   // :58
Initial { _tag: "Initial"; waiting }                                            // :168
Success { _tag: "Success"; value: A; timestamp: number; waiting }               // :249
Failure { _tag: "Failure"; cause: Cause<E>; previousSuccess: Option<Success<A,E>>; waiting } // :290
```
- `waiting` means a refresh is in flight while the previous value is kept (`:395-433`).
- Failures carry the last success (`failureWithPrevious`, `:344`).
- It implements `Equal` and `Hash` (`:120-145`), but atom equality is still `Object.is`.
- Helpers: `match`, `matchWithError`, `matchWithWaiting`, `builder`, `all`, `toExit`, `getOrThrow`, `isInterrupted`, and `AsyncResult.Schema({success, error})` for serialization (`:733-1218`).

## 2. AtomRegistry (`E:src/reactivity/AtomRegistry.ts`)

- **API:** `get`, `set`, `modify`, `update`, `refresh`, `mount(atom): () => void`, `subscribe(atom, f, {immediate}): () => void`, `reset`, `dispose`, `getNodes`, `setSerializable` and `preloadedSerializable` (`:69-88`). `make({initialValues?, scheduleTask?, timeoutResolution?, defaultIdleTTL?})` (`:131-142`).
- **Effect integration:** `AtomRegistry` is a `Context.Service` (`:158`). `layerOptions(...)` and `layer` build a fresh registry that is **disposed when the layer scope closes** (`:172-198`). `Atom.get/set/update/modify/getResult/refresh/mount/toStream` are Effects requiring `AtomRegistry` (`Atom.ts:3019-3191`).
- **Dependency graph.**
  - Reads during `build` record parents (`readTracked`, `:891-918`). A rebuild drops parents it did not read (`dropUnread`, `:919-933`), so tracking is dynamic and per run.
  - Invalidation marks nodes stale. **Observed** nodes, meaning those with listeners, non-lazy nodes, or nodes with observed children, rebuild eagerly. Unobserved nodes just propagate stale (`:813-820, 995-1022`).
  - `value()` pulls lazily (`:837-844`).
  - A throwing build marks children stale and rethrows (`:881-889`).
- **Notification scheduling.** Listeners run **synchronously** in `setValue → announce → notify`. Inside `batch` they are deferred to the commit phase (`:956-993, 1053-1058`).
- **Disposal.** A node `canBeRemoved` when it is not keepAlive and has no listeners and no children (`:806-808`). Removal is scheduled on the registry's async dispatcher, `scheduleTask(…, 0)` (`:635-650`).
  - With an `idleTTL` or `defaultIdleTTL` the node goes into a time bucket of resolution `timeoutResolution` (default 1000 ms, or `defaultIdleTTL/2`) and is swept later (`:501-518, 589-591, 662-725`).
  - `removeTtl = setIdleTTL(0)` opts an atom out of the default TTL (`Atom.ts:273`, `AtomRegistry.ts:589-591`).
  - On removal the lifetime finalizers run in reverse order (`:1272-1281`).
- **`dispose()`** sets `disposed`. Any later `ensureNode` on a new atom then throws `Cannot access Atom …: registry is disposed` (`:746-749, 618-621`).
- **Per-request SSR.** Use one registry per request: `AtomRegistry.make()` or `layer` inside a request scope. A default `Atom.runtime`'s layers are then memoised per registry (§1). Caveats: `family` and the `batchState` module are process-global (`Atom.ts:1603`, `AtomRegistry.ts:1346`). Running effect-atom fibers stop only when the registry is disposed or the node is removed.

## 3. Hydration (`E:src/reactivity/Hydration.ts`)

- `Atom.serializable({key, schema})`: `S extends Schema.ConstraintCodec<Type<R>, any>` and returns `R & Serializable<S>`. It builds `Schema.toCodecJson(schema)` and uses **synchronous** `encodeSync/decodeSync` (`Atom.ts:3254-3305`). It sets the label to the key. The key replaces atom identity as the registry key (`AtomRegistry.ts:483-484`), so two atoms with the same key share one node.
- `dehydrate(registry, {encodeInitialAs?: "ignore"|"promise"|"value-only"})` returns `DehydratedAtom[]` with fields `{key, value, dehydratedAt, resultPromise?}` (`Hydration.ts:37-42, 72-118`).
  - It walks **only nodes currently in the registry**, and reads each one, which forces a build.
  - Values whose schema encoding fails are **silently skipped**: a `SchemaError` gives `Skipped` (`:46-57`).
  - `"promise"` attaches a `Promise` that resolves when the atom leaves `Initial`. That value is not JSON, so the caller must stream it.
- `hydrate(registry, state)` calls `setSerializable(key, encoded)`, which is consumed on the next `ensureNode`. That call decodes the value and `setValue`s it, or seeds `initialValueTarget` (`AtomRegistry.ts:603-616`). A resolved `resultPromise` later updates the live node (`Hydration.ts:136-165`).
- `toValues` is a cast to `DehydratedAtomValue[]` (`:124`).
- **Server values.** `withServerValue(atom, get => value)` is read by `getServerValue(atom, registry)`, which React's `getServerSnapshot` uses (`Atom.ts:3324-3406`, `AR:src/Hooks.ts:56-60`). `withServerValueInitial` makes the server render `AsyncResult.initial(true)` (`Atom.ts:3358`).
- **AtomRpc and AtomHttpApi** make queries serializable only if you pass `serializationKey`. The key is then `AtomRpc:${tag}:${serializationKey}` or `AtomHttpApi:${group}:${endpoint}:${serializationKey}`, and the value is coded with `AsyncResult.Schema` (`AtomRpc.ts:244-251`, `AtomHttpApi.ts:278-286`). RPC mutations are always serializable under `AtomRpc:mutation:${tag}` (`AtomRpc.ts:213-219`). Stream queries are never serializable (`:244`).

## 4. `@effect/atom-react` 4.0.5 (`AR:src/`)

Exports: `Hooks`, `RegistryContext`, `ReactHydration` and `ScopedAtom`, all re-exported at the root (`index.ts:9-24`). The package is marked `"use client"`.

- **`RegistryContext`** defaults to a **module-global registry** `AtomRegistry.make({scheduleTask, defaultIdleTTL: 400})` (`RegistryContext.ts:47-50`). `scheduleTask` uses `scheduler.unstable_scheduleCallback(LowPriority)` (`:25-28`). Without a provider, every SSR request shares the same registry.
- **`RegistryProvider({initialValues?, scheduleTask?, timeoutResolution?, defaultIdleTTL?})`** creates its registry once in a `useRef`. When the provider unmounts it **disposes the registry after a 500 ms timeout**, and a remount cancels that disposal (`:79-111`). This tolerates StrictMode's mount → unmount → mount. Changing props after mount has no effect.
- **The core of every reading hook** is `useSyncExternalStore(store.subscribe, () => registry.get(atom), getServerSnapshot)`. Each store is cached per `(registry, atom)` in nested WeakMaps (`Hooks.ts:28-71`). `getServerSnapshot` returns `Atom.getServerValue`. While any reader is subscribed it is frozen at the first snapshot, so late Suspense boundaries hydrate consistently (`:40-61`). Documented gotcha: mismatches can still occur "before the first reader hydrates or after the last unsubscribes … as can a derived atom first read inside a late boundary" (`:141-147`).
- **Reading in render calls `registry.get`.** For an effect atom that **forks its fiber during render**, including on the server and in discarded concurrent renders. A node that is never subscribed is reclaimed by the scheduled removal or TTL (`AtomRegistry.ts:618-650`).

| Hook | Semantics (source) |
|---|---|
| `useAtomValue(atom)` / `useAtomValue(atom, f)` | Subscribes and returns the value. The selector is memoised on `[f]` and caches the last source/result (`Hooks.ts:156-223, 73-91`). Pass a stable `f`, because a new `f` each render resets the cache. |
| `useAtom(writable, {mode?})` | `[value, set]`. `set` accepts `W | (R => W)` (`:377-397, 254-256`). |
| `useAtomSet(writable, {mode?: "value"|"promise"|"promiseExit"})` | Mounts in `useEffect` and returns a setter without subscribing. The promise modes are typed only for AsyncResult atoms. They `set`, then await `getResult(..., {suspendOnWaiting: true})`. `"promise"` rejects with `Cause.squash` (`:311-331, 229-262`). |
| `useAtomMount(atom)` | `useEffect(() => registry.mount(atom), [atom, registry])` (`:225-289`). |
| `useAtomRefresh(atom)` | Mounts and returns `() => registry.refresh(atom)` (`:353-359`). |
| `useAtomSuspense(atom, {suspendOnWaiting?, includeFailure?})` | Throws a cached promise, per registry and atom, while the result is `Initial` (or `waiting`). Failures are **thrown** with `Cause.squash` unless `includeFailure`, so an error boundary is needed (`:476-489, 399-449`). After it resolves, the helper subscription is held for 1 s (`:430`). |
| `useAtomSubscribe(atom, f, {immediate?})` | A subscription in `useEffect` with deps `[registry, atom, f, immediate]`, so an unstable `f` resubscribes on every render (`:512-522`). |
| `useAtomInitialValues(pairs)` | `setValue` during render, at most once per atom per registry (`:112-125`). |
| `useAtomRef`, `useAtomRefProp`, `useAtomRefPropValue` | For `AtomRef` (`effect/reactivity/AtomRef`). Subscribed via `useReducer` in an effect (`:544-596`). |

- **`HydrationBoundary({state})`** hydrates **new** keys during render inside `useMemo`, and defers keys that **already exist** to `useEffect`, so an aborted transition does not update the visible UI (`ReactHydration.ts:51-112`).
- **`ScopedAtom.make(f)`** returns `{Provider, use, Context}`. Each Provider creates its atom once. `use` throws outside the Provider, and a changed `value` prop does not recreate the atom (`ScopedAtom.ts:73-162`).
- **4.0.6** changed `Hooks.ts` and `RegistryContext.ts` (`gh compare effect@4.0.5...effect@4.0.6`). Those diffs were not read (**unverified**).

## 5. Fullstack: Rpc, HttpApi, AtomRpc, AtomHttpApi

- **Rpc** (`effect/rpc`, unstable):
  - `Rpc.make(tag, {payload?, success?, error?, defect?, stream?: boolean, primaryKey?})`. With `stream: true`, success becomes `RpcSchema.Stream<Success, Error>` (`E:src/rpc/Rpc.ts:941-960`).
  - `RpcGroup.make(...rpcs)` (`RpcGroup.ts:411`).
  - Server: `RpcServer.make/layer/layerHttp/toHttpEffect/toHttpEffectWebsocket`, plus the protocols `layerProtocolHttp|Websocket|SocketServer|Stdio|WorkerRunner` (`RpcServer.ts:602-1592`).
  - Client: `RpcClient.make(group, {flatten?})` and `layerProtocolHttp|Socket|Worker` (`RpcClient.ts:643-1502`).
  - Serialization: `RpcSerialization.layerJson|Ndjson|JsonRpc|NdJsonRpc|SchemaBinary` (`RpcSerialization.ts:595-656`).
- **AtomRpc.Service<Self>()(id, {group, protocol: Layer | get => Layer, spanPrefix?, spanAttributes?, generateRequestId?, disableTracing?, makeEffect?, runtime?})** returns a `Context.Service` class with `.runtime`, `.mutation(tag)` and `.query(tag, payload, {headers?, reactivityKeys?, timeToLive?, serializationKey?})` (`AtomRpc.ts:117-288`). The protocol layer is `Layer.orDie`d (`:154-158`).
  - `mutation(tag)` is `family`-cached and gives `AtomResultFn<{payload, reactivityKeys?, headers?}, Success, Error | RpcClientError | middleware errors>`. Stream RPCs give `never` here (`:30-56, 195-221`).
  - `query` returns `Atom<AsyncResult<…>>`, or a **`Writable<PullResult<…>, void>` for stream RPCs**. It is `family`-cached on a structural `QueryKey` (`:58-89, 223-288`).
  - A finite `timeToLive` gives `setIdleTTL`; an infinite one gives `keepAlive` (`:253-257`).
- **HttpApi** (`effect/http-api`, unstable): `HttpApi.make(id)` (`HttpApi.ts:235`), `HttpApiGroup.make` (`HttpApiGroup.ts:415`), `HttpApiEndpoint`, `HttpApiBuilder.layer/group` (`HttpApiBuilder.ts:66,130`), `HttpApiClient.make(api, …)` (`HttpApiClient.ts:525`), plus OpenApi, Swagger and Scalar.
- **AtomHttpApi.Service<Self>()(id, {api, httpClient: Layer | get => Layer, transformClient?, transformResponse?, baseUrl?, runtime?})** has `.mutation(group, endpoint, {responseMode?})` → `AtomResultFn`, and `.query(group, endpoint, request & {reactivityKeys?, timeToLive?, serializationKey?})` (`AtomHttpApi.ts:60-140, 172-330`).
- **HttpServer / HttpRouter** (`effect/http`): `HttpServer.serve` (`HttpServer.ts:93`), `HttpRouter.toWebHandler` (`HttpRouter.ts:1499`), `HttpEffect.toWebHandler*` (`HttpEffect.ts:336-482`). The drivers are in `@effect/platform-*`.
- **"Server functions"**: there is no `serverFn` primitive. The closest is an `Rpc` with a typed `AtomRpc` client, where `reactivityKeys` link mutations to queries through `Reactivity` (`AtomRpc.ts:204-210`). This conclusion comes from a grep for such a primitive in `E:src/reactivity`, `rpc` and `http` (**inference**).

## 6. Observability

- **Core and stable:** `Tracer.ts` (`Tracer` Reference `:653`, `make` `:470`, `externalSpan` `:512`), `Effect.withSpan / withSpanScoped / useSpan / makeSpan / annotateCurrentSpan / currentSpan / withTracer` (`Effect.ts:16681-17604`), and `Effect.fn(name)` traced vs. `fnUntraced` (`:23109, 22984`). `Metric.counter/gauge/frequency/histogram/summary/timer/update/snapshot` (`Metric.ts:2102-4102`). `Logger.ts`.
- **`effect/observability`** (unstable) contains OTLP exporters that need no OpenTelemetry SDK: `Otlp.layer({baseUrl, resource?, headers?, compression?, maxBatchSize?…})`, `layerJson`, `layerProtobuf`, `OtlpTracer`, `OtlpLogger` and `OtlpMetrics` (`.layer` and `.layerFromConfig`), `OtlpSerialization.layerJson|layerProtobuf`, and `PrometheusMetrics.layerHttp` (`observability/*.ts`, `Otlp.ts:37-173`). ai-docs: "use the lightweight Otlp modules from `effect/observability` in new projects, or use `@effect/opentelemetry` NodeSdk when integrating with an existing OpenTelemetry setup" (`E:ai-docs/src/08_observability/index.md`). The example is in `20_otlp-tracing.ts`.
- **`@effect/opentelemetry@4.0.6`** provides `NodeSdk`, `WebSdk`, `OtelTracer`, `OtelLogger`, `OtelMetrics` and `Resource`, all unstable. Its peers `@opentelemetry/*` v2 / api-logs 0.2xx are optional (`gh:packages/opentelemetry/src/index.ts`, `package.json`).
- **DevTools** is part of core in v4: `effect/devtools` → `DevTools.layer(url = "ws://localhost:34437"): Layer<never>`, `layerWebSocket`, `layerSocket`, `DevToolsClient`, `DevToolsServer` and `DevToolsSchema`. It "streams spans, span events, span completions, and metric snapshots" and does not start a server (`devtools/DevTools.ts:1-73`). `@effect/experimental` is v3-only.
- **Atom tracing hooks:** `AtomRpc` has `spanPrefix`, `spanAttributes` and `disableTracing` (`AtomRpc.ts:120-123`). Fibers started by atoms inherit whatever Tracer the runtime layer provides. **Unverified**: whether atom builds create their own spans. None was seen in `makeEffect`.

## 7. CLI (`effect/cli`, unstable; replaces v3 `@effect/cli`)

Modules: `Argument`, `CliConfig`, `CliError`, `CliOutput`, `Command`, `Completions`, `Flag`, `GlobalFlag`, `HelpDoc`, `Param`, `Primitive` and `Prompt` (`E:src/cli/`).

- **Command:** `make`, `withHandler`, `withSubcommands`, `withSharedFlags`, `withGlobalFlags`, `withDescription`, `withAlias`, `withExamples`, `provide*`, `wizard`, `run`, `runWith` (`Command.ts:640-3052`). A `Command<Name, Input, ContextInput, E, R>` is itself an `Effect<ContextInput, never, CommandContext<Name>>` (`:122-129`). `run({version, renderErrors?})(cmd): Effect<void, E | CliError, R | Environment>`, where `Environment = FileSystem | Path | Terminal | ChildProcessSpawner | Stdio` (`dist/cli/Command.d.ts:2239-2243`, `Command.ts:405`).
- **Flag** (`String/Boolean/Int/Choice/Path/File/Redacted/...` + `withAlias/withDefault/optional/withFallbackConfig/withFallbackPrompt/withSchema`) and **Prompt** (`Confirm/Select/MultiSelect/AutoComplete/Password/...`) (`Flag.ts`, `Prompt.ts`).
- **Wizard:** `Command.wizard(cmd, {prefix?}): Effect<string[], CliError | QuitError, Environment>` (`Command.ts:2704-2710`), plus the built-in `GlobalFlag.Wizard`.
- **Completions:** `Completions.generate(exe, shell, descriptor)` for `"bash" | "zsh" | "fish" | "powershell"` (`Completions.ts:22, 124`), and `GlobalFlag.Completions`.
- **Other built-in global flags:** `Help`, `Version`, `LogLevel` (`GlobalFlag.ts:165-324`).
- Example: `E:ai-docs/src/70_cli/10_basics.ts`. 4.0.6 touched `Command`, `Completions`, `Flag` and `GlobalFlag`, and added `TerminalCapabilities.ts` (gh compare). That diff was not read.

## 8. Stability summary

| Area | Module path | Marker |
|---|---|---|
| Atom, AtomRegistry, Hydration, AsyncResult, AtomRpc, AtomHttpApi, AtomRef, Reactivity | `effect/reactivity` | unstable (`reactivity/index.ts`) |
| React bindings | `@effect/atom-react` | unstable (all `AR:src`) |
| Rpc / Http / HttpApi | `effect/rpc`, `effect/http`, `effect/http-api` | unstable |
| CLI | `effect/cli` | unstable |
| OTLP / Prometheus | `effect/observability` | unstable |
| OpenTelemetry SDK bridge | `@effect/opentelemetry` | unstable |
| DevTools | `effect/devtools` | unstable |
| Tracer, Metric, most of Logger/Effect | `effect` | stable |

## 9. Invariants a compiler could enforce

Each item names the runtime fact, then the static check.

1. **Atom identity is stable.** Nodes are keyed by object identity and every combinator copies (§1). Atom definitions should be top-level or `family`/ScopedAtom values only. Reject `Atom.make`/combinator calls in a component body or render path, unless they are memoised by a compiler-known key.
2. **Families get keys with structural equality.** `family` keys are compared with `Equal`/`Hash` (§1). Restrict family argument types to an `Eq + Hash` class with value semantics: no functions, no mutable objects. Mutating a key after use would break lookup (**inference**).
3. **Effect atoms need a closed service row.** `make(effect)` permits only `Scope | AtomRegistry` (`Atom.ts:477, 495`). Anything else must go through `runtime(layer).atom`. Type-check effect atom rows: `r ⊆ {Scope, AtomRegistry}`, or `r ⊆ R_layer ∪ {Scope, AtomRegistry, Reactivity}` for runtime atoms. Lift layer errors `ER` into the atom's error row `E | ER` (`Atom.ts:743-760`).
4. **Track only during the synchronous build.** `get` inside `fn` and after the lifetime is disposed is untracked, and writes after disposal are dropped (`AtomRegistry.ts:1120-1245, 1279`). Give the read context a region and capture discipline: the `get` capability must not escape into async continuations or closures. Distinguish tracked `get` from `once` in the types.
5. **Fibers are interruptible.** Invalidation and unmount interrupt the fiber unless the atom is `uninterruptible` (§1). Effects with irreversible side effects, such as mutations, should live in `fn` or `uninterruptible` atoms, so mark them in the effect row (for example a `Mutation` label). A compiler could reject such effects in derived read atoms.
6. **Reads are pure aside from declared finalizers.** A `read` may run many times, in discarded concurrent renders, and on the server. Require reads to be pure except through `addFinalizer`, `setSelf` and the effect itself.
7. **Serializable keys are unique and the schema is sync and context-free.** Keys replace identity (`AtomRegistry.ts:483`). Encoding is `encodeSync` and failures are silently skipped (`Hydration.ts:46-57`). `searchParam` schemas must be synchronous with no context (`Atom.ts:2925`). Check globally that keys are unique (or derived from the family key), and that a codec exists for `Type<atom>` with an empty service row and no async.
8. **Server/client parity.** Hydration mismatches occur if the server value differs from the client's first snapshot (`Hooks.ts:141-147`). `withServerValue` must have the atom's exact type. Atoms that touch `window` (`searchParam`, `windowFocusSignal`) need a server value or a client-only annotation.
9. **Hooks follow React's rules.** They call `useContext`/`useSyncExternalStore`/`useEffect` (`Hooks.ts`). Enforce static hook order: no hooks under conditionals or loops, with components as a distinct effect.
10. **Callbacks are stable.** `useAtomValue`'s selector and `useAtomSubscribe`'s `f` are dependencies (`Hooks.ts:84, 520`). Hoist lambdas or require them to be closed and pure.
11. **Suspense and error boundaries are required.** `useAtomSuspense` throws a promise, and throws on failure without `includeFailure` (`Hooks.ts:476-489`). Encode "may suspend" and "may throw E" as component effects, and require an enclosing Suspense or ErrorBoundary handler. Alternatively force `includeFailure` and make the handling of `E` exhaustive.
12. **Registry scope on the server.** The default `RegistryContext` is process-global (`RegistryContext.ts:47`). For SSR entrypoints, require a `RegistryProvider` or a per-request registry in scope (a region per request). Prevent the registry from escaping after `dispose()`, which throws on any new node access (`AtomRegistry.ts:618-621`).
13. **Writable vs. readable.** `useAtom` and `useAtomSet` need `Writable<R, W>`, and the write type differs from the read type (for example `AtomResultFn` accepts `Arg | Reset | Interrupt`). This is already typed in TS. Keep separate read and write type parameters in Flick.
14. **Promise-mode setters only on AsyncResult atoms.** The mode is typed conditionally (`Hooks.ts:318`). A typeclass constraint `AsyncResultLike R` expresses this.
15. **Rpc stream vs. unary.** `query` returns a `PullResult` writable for streams, and `mutation` is `never` for streams (`AtomRpc.ts:30-89`). Map these to distinct Flick types, and forbid `serializationKey` on stream queries (it is ignored, `:244`).
16. **TTL and keepAlive leaks.** `keepAlive` atoms and runtimes are never collected. `family` holds weak references only. Lint `keepAlive` inside families or per-request code.

## Not verified

- Diffs in `@effect/atom-react` 4.0.6 (`Hooks.ts`, `RegistryContext.ts`) and in `effect` 4.0.6 reactivity/CLI.
- The exact behaviour of StrictMode double-render on effect atoms. The 500 ms dispose delay and scheduled node removal were read, but not run.
- Whether atom builds emit spans.
- Website pages for modules other than `atom-react`, `atom-react/Hooks` and `effect/reactivity/Atom`.
- Behaviour of the `Hash` cache for mutated family keys.
