# Prior art: statically typed components, hooks and reactive UI

Research for Flick's reactive UI layer: atoms/signals on Effect v4 `effect/reactivity`, rendered with React 19. Gathered 2026-10-24 from primary sources only; unchecked items are marked **(unverified)**.

Pinned commits (shallow clones, read only): react-basic-hooks `61344f4`, react-basic `b33e284`, halogen `1433238`, halogen-hooks `f22006b`, rescript-react `bb34644`, lustre `f5ac432`, leptos `5ca3a9b`, bonsai `f316614`, Feliz `7358b73`, elm/html `1affbf3`, urweb `55a881f`; effect@4.0.5 from npm (`dist/reactivity/*.d.ts`).

[rbh]: https://github.com/megamaddu/purescript-react-basic-hooks/blob/61344f44d55815a74c86f4662bba2193ab18ae19
[hh]: https://github.com/thomashoneyman/purescript-halogen-hooks/blob/f22006bb074ed006211361320454b43e00734bbf
[hal]: https://github.com/purescript-halogen/purescript-halogen/blob/143323876965b0e9f2d7c57c80e5830ccb16c1d8
[lep]: https://github.com/leptos-rs/leptos/blob/5ca3a9bf90cbcc0561dc4323f8018e18dce5906f
[bon]: https://github.com/janestreet/bonsai/blob/f31661450eb133fe89564219d97669c2735c6622
[lus]: https://github.com/lustre-labs/lustre/blob/f5ac432bb2d625951be4f8731fe446aa03320061
[fel]: https://github.com/fable-hub/Feliz/blob/7358b7379e91c04880213b9f0ecaeb2522a7a92b
[ur]: https://github.com/urweb/urweb/blob/55a881ff9b50d9e5c3b2fd564f5cd44a5cc5e6bc

---

## 1. PureScript

### 1.1 `react-basic-hooks`: an indexed `Render` monad

**Core types** ([`Hooks/Internal.purs` L39, L133, L141][rbh]):

```purescript
newtype Render :: Type -> Type -> Type -> Type
newtype Render x y a = Render (Effect a)

type Pure a = forall hooks. Render hooks hooks a
type Hook (newHook :: Type -> Type) a = forall hooks. Render hooks (newHook hooks) a
```

`x` is the hook stack before a computation and `y` the stack after it. Each primitive hook is an opaque type constructor that pushes one layer onto the stack (`foreign import data UseState :: Type -> Type -> Type`, `UseEffect`, `UseMemo`, ...; `Hooks.purs` L224, L265, L389). Sequencing goes through indexed bind, exported for qualified `React.do` ([`Internal.purs` L159][rbh]):

```purescript
bind :: forall a b x y z m. IxBind m => m x y a -> (a -> m y z b) -> m x z b
```

**How the rules of hooks are enforced.**

- **Order and count.** A component's whole hook sequence appears in its type: `Render Unit hooks JSX`, where `hooks` is something like `UseEffect deps (UseState Int Unit)`. The two branches of an `if` must end at the same `y`, so they must run identical hook sequences.
- **No hooks in loops or traversals.** The ordinary `Applicative`, `Bind` and `Monoid` instances exist only under `TypeEquals x y` (`Internal.purs` L172, L175). Generic `traverse` or `for` can therefore only run computations that add no hooks (`Pure`).
- **Phantom types must be complete.** The `unsafeHook` documentation (L84–L117) shows why every phantom has to carry its value type. If `UseState` did not record its state type, two branches could swap `useState 0` and `useState Nothing`, and React would return a value of the wrong type.
- **Rendering cannot perform effects.** `Render` has no `MonadEffect` instance; grep finds none in `src/`. Effects run during render only through `unsafeRenderEffect :: forall a. Effect a -> Pure a` (L128). The doc comment on `Render` says it "does not allow arbitrary side effects". A setter has type `(state -> state) -> Effect Unit` (`useState`, `Hooks.purs` L210), so calling it during render needs the unsafe escape hatch. It is still easy to call a setter inside `useEffect` or an event handler.
- **Not enforced:** hooks are not tied to components. `reactComponentFromHook` and custom hooks are ordinary `Render` values, and nothing stops a `Hook` from being run outside React (`unsafeDiscardRenderEffects`, `Hooks.purs` L167).

**Creating a component is an effect, and the component gets a name** ([`Hooks.purs` L87–L121][rbh]):

```purescript
type Component props = Effect (props -> JSX)
component :: forall hooks props. String -> (props -> Render Unit hooks JSX) -> Component props
reactComponent :: forall hooks props.
  Lacks "children" props => Lacks "key" props => Lacks "ref" props =>
  String -> ({ | props } -> Render Unit hooks JSX) -> Effect (ReactComponent { | props })
```

The doc comment says: "Creating components is effectful because React uses the function instance as the component's 'identity' or 'type'. Components should be created during a bootstrap phase and not within component lifecycles or render functions." The `String` becomes `displayName` (L165). Creating a component in render is made awkward, not impossible (`unsafeRenderEffect`). `Lacks "key"`/`"ref"` keep reserved props out of the row.

**Typed dependencies** (L245, L379):

```purescript
useEffect :: forall deps. Eq deps => deps -> Effect (Effect Unit) -> Hook (UseEffect deps) Unit
useMemo   :: forall deps a. Eq deps => deps -> (Unit -> a) -> Hook (UseMemo deps a) a
```

Dependencies are one value compared with the PureScript `Eq` instance, not an array compared with `Object.is`. A missing dependency is still possible, because nothing checks that the closure reads only from `deps`. There are escape hatches: `UnsafeReference a` has an `Eq` instance that compares references (L535–L540), and `useEffectOnce` and `useEffectAlways` exist.

**`memo`** (L185, L201): `memo :: Effect (ReactComponent props) -> Effect (ReactComponent props)` compares props by reference. `memo' :: (props -> props -> Boolean) -> Effect (ReactComponent props) -> Effect (ReactComponent props)` takes a comparison function, typically `eq`. Both stay in the creation-time `Effect`.

**Custom hooks behind a newtype.** A type alias would expose the hook's internal stack in error messages. `coerceHook` (L76) seals it instead:

```purescript
coerceHook :: forall hooks oldHook newHook a.
  Newtype newHook oldHook => Render hooks oldHook a -> Render hooks newHook a
```

`useAff` ([`Hooks/Aff.purs` L42, L88][rbh]) is the standard example:

```purescript
useAff :: forall deps a. Eq deps => deps -> Aff a -> Hook (UseAff deps a) (Maybe a)
newtype UseAff deps a hooks = UseAff (hooks & UseState (Maybe (Either Error a)) & UseEffect deps)
```

The fiber is killed when `deps` change or the component unmounts ("Stale request cancelled"). An error is rethrown into render with `unsafeRenderEffect` so that an error boundary catches it (`Aff.purs` ~L75–L86).

**Cost.** Hooks need qualified `React.do` syntax and an indexed monad. Every hook needs an opaque type constructor with complete phantoms. Hook stacks leak into error messages unless they are newtyped. PureScript has no row or effect inference for this; users write `Component Props` signatures.

### 1.2 `purescript-react-basic`

Now only shared core types (`JSX`, `ReactComponent`, `ReactContext`, `Ref`, `element`) ([`src/React/Basic.purs`](https://github.com/lumihq/purescript-react-basic/blob/b33e284d87d7820d612f509e725867ce83258cf4/src/React/Basic.purs) L18–L76); the class API moved to `react-basic-classic` (README).

### 1.3 Halogen and halogen-hooks

In **Halogen** the render function is pure and every effect goes through a separate eval monad ([`Component.purs` L79][hal]):

```purescript
type ComponentSpec state query action slots input output m =
  { initialState :: input -> state
  , render :: state -> HC.HTML (ComponentSlot slots m action) action
  , eval :: HalogenQ query action input ~> HalogenM state action slots output m }
newtype HalogenM state action slots output m a = HalogenM (Free (HalogenF state action slots output m) a)
```

(`Query/HalogenM.purs` L81.) `render` cannot change state because it is a plain function. `HalogenF` (L43) lists everything eval may do: `State`, `Subscribe`, `Lift (m a)`, `ChildQuery`, `Raise output`, `Fork`, `GetRef`. Child components are typed by a `slots :: Row Type` row. `unComponent` hides `state`/`action`/`slots` existentially, and "the compiler will complain about an escaped skolem" if they leak (`Component.purs` L92–L103).

**halogen-hooks** indexes `Hook` by a `HookType` kind that is a monoid of hook-type labels ([`Hook.purs` L27, L51–L61, L87][hh]):

```purescript
newtype Hook m h a = Hook (Free (UseHookF m) a)       -- type role nominal on h
bind :: forall h h' m a b. Hook m h a -> (a -> Hook m h' b) -> Hook m (h <> h') b
foreign import data UseState :: Type -> HookType
useState :: forall state m. state -> Hook m (UseState state) (state /\ StateId state)
```

The key split: **state is read in `Hook` and written only in `HookM`**:

```purescript
modify_ :: forall state m. StateId state -> (state -> state) -> HookM m Unit   -- HookM.purs L127
```

The hook body cannot write state, because no function lifts `HookM` into `Hook`. Writes happen in effects and handlers, which are `HookM` values. Effect dependencies are a `MemoValues` token built with `captures :: forall memos a. Eq (Record memos) => Record memos -> (MemoValues -> a) -> a` (`Hooks.purs` L202). Custom hooks are sealed with `class HookNewtype (a :: HookType) (b :: HookType) | a -> b` and `wrap` (`Hook.purs` L76, `Hooks.purs` L250). Components are built with `component :: (ComponentTokens q s o -> i -> Hook m hooks (H.ComponentHTML (HookM m Unit) s m)) -> H.Component q i o m` (`Component.purs` L61).

---

## 2. Contrast: other typed reactive UIs

**ReScript React.** Hooks are plain externals, for example `external useState: (unit => 'state) => ('state, ('state => 'state) => unit)` and `useEffect1: (unit => option<unit => unit>, array<'a>) => unit` ([`src/React.res` L135, L155](https://github.com/rescript-lang/rescript-react/blob/bb34644ca803e021e7218a8a63b572705df73701/src/React.res)). The docs say "ReScript doesn't enforce those rules within the compiler" and suggest running eslint-plugin-react-hooks on the compiled JS ([hooks overview](https://rescript-lang.org/docs/react/latest/hooks-overview)). Guarantees: typed props and fixed dependency arity (`useEffect1`/`useEffect2`...).

**Gleam Lustre.** Lustre is MVU. `application(init: fn(args) -> #(model, Effect(msg)), update: fn(model, msg) -> #(model, Effect(msg)), view: fn(model) -> Element(msg))` ([`src/lustre.gleam` L278][lus]). `component` adds attribute decoders and its own update loop (L304). The docs advise it only for complex widgets (L296–L302). `start_server_component` runs the same `App` headless on the server and sends DOM patches to connected clients (L389). SSR is `element.to_string`. On hydration the guide says: "Lustre doesn't have a built-in hydration mechanism". The user serializes the model into a `<script type="application/json">` tag and decodes it as the client's `flags` ([guide 05][lus], L97–L214 of `pages/guide/05-server-side-rendering.md`). The client/server split relies on Gleam compile targets, with three packages `client`/`server`/`shared` (`pages/guide/06-full-stack-applications.md` L18–L24). Guarantees: `view` is pure (Gleam), effects are values, and the `Msg` type is closed.

**Leptos (Rust).**

- *Signals.* `RwSignal` is "arena-allocated ... `Copy` and is disposed when its reactive `Owner` cleans up" ([`reactive_graph/src/signal/rw.rs` L24–L32][lep]). An `Owner` manages effect cancellation, context, cleanups and the arena (`owner.rs` L36–L49). Reading a disposed signal panics at runtime (`traits.rs` L66–L90). Use-after-scope is *not* a compile-time error.
- *Server functions.* `#[server]`: "its body will only run on the server ... If you call a server function from the client ... it will instead make a network request" ([`leptos_macro/src/lib.rs` L905][lep]). The macro emits the real body only under `cfg!(feature = "ssr")` (`server_fn_macro/src/lib.rs` L628, L846). The book says arguments and return values "both need to be serializable", and warns: "Server functions are not magic; they're syntax sugar for defining a public API" ([book: server functions](https://book.leptos.dev/server/25_server_functions.html)).
- *Isomorphic SSR.* The app is "compiled for two separate targets", `ssr` and `hydrate` (WASM). Hydration "walks over the existing HTML tree" ([book: life cycle](https://book.leptos.dev/ssr/22_life_cycle.html)). `Resource::new` requires `JsonSerdeCodec: Encoder<T> + Decoder<T>` and `T: Send + Sync`. On the server it runs the fetcher and serializes the result; "on the client, the initial value will be deserialized without re-running that async task" (`leptos_server/src/resource.rs` L1066–L1098).
- *Islands.* `#[island]`: "Only code included in an island itself is compiled to WASM". Props cross the boundary serialized; `children` are opaque server HTML (`leptos_macro/src/lib.rs` L620–L705).

**Elm.** `view : Model -> Html Msg` is pure by language design, and effects are `Cmd`/`Sub` values. The guide says: "Actively trying to make components is a recipe for disaster in Elm ... components = local state + methods ... = objects", and recommends helper `view` functions over one `Model` ([guide: structure](https://guide.elm-lang.org/webapps/structure.html)). Purity is what makes `Html.Lazy.lazy : (a -> Html msg) -> a -> Html msg` sound: "Since all Elm functions are pure we have a guarantee that the same input will always result in the same output" ([`src/Html/Lazy.elm` L5–L33](https://github.com/elm/html/blob/1affbf39efef4b4529110a567f706130c178a457/src/Html/Lazy.elm)). Official SSR/hydration: none found in elm/browser **(unverified, absence not proven)**.

**Jane Street Bonsai (OCaml).**

- *Construction is separate from runtime.* `Bonsai.graph` "is always `local_` because Bonsai applications have two phases: 1. The graph building phase ... 2. Runtime ... modifying the graph is no longer permitted" ([`src/cont.mli` L15–L21][bon]). Components take `(graph @ local)` (README). OxCaml's locality mode stops the graph handle from escaping into closures that run later. This is the closest analogue to Flick's capture qualifiers.
- *State and actions.* `state : 'model -> graph -> 'model t * ('model -> unit Effect.t) t` (L131). `state_machine : default_model:'model -> apply_action:(... -> 'model -> 'action -> 'model) -> graph -> 'model t * ('action -> unit Effect.t) t` (L215). The setter is a *value inside* `t` that returns a `unit Effect.t`, so a derivation can only *schedule* a write.
- *Static graph shape.* `'a t` is only `Applicative` (L26), not a monad. Dynamic structure goes through `match%sub`, `enum ~match_ ~with_:('k -> graph -> 'a t)` (L511) and `assoc` (L1018), which fix the branches when the graph is built. This is a structural rules-of-hooks: no conditional state creation at runtime.

**F# Feliz / Fable.**

- *2.x API.* Components were created with `React.functionComponent(name, render)`, which sets `displayName` (compiled 2.9.0 `Internal.fs.js` assigns `displayName`). The pitfalls page warns: "Components must be defined at the module level" because a component defined inside a function loses its state on re-evaluation. That rule was a convention, not a type error ([`docs/versioned_docs/version-2.9.0/api-docs/React/CommonPitfalls.md` L54][fel]).
- *Current API.* The current Fable 5 plugin is the `[<ReactComponent>]` attribute (`src/Feliz.CompilerPlugins/ReactComponent.fs` L349). At compile time it warns if the declaration is not a function, does not return `ReactElement`, or is camelCase, "to allow tools such as react-refresh to pick it up" (L448–L473). It checks no hook rules.

**Ur/Web (tierless; added because it maps directly onto Flick's goals).** Reading and writing are separate monads ([`lib/ur/basis.urs` L144–L155][ur]):

```
con transaction :: Type -> Type        val source : t ::: Type -> t -> transaction (source t)
val set : source t -> t -> transaction unit        con signal :: Type -> Type
val signal : source t -> signal t      val current : signal t -> transaction t
val dyn : ... -> tag [Signal = signal (xml ([Dyn] ++ ctx) use bind)] ...   (L838)
```

- *Read-only views.* A `<dyn>` view is a `signal`, and `signal` "represents ... some pure function over all sources"; using `signal` on a source "implicitly subscribe[s]" (`doc/manual.tex` L2222). No `set` exists in `signal`, so **the render-may-only-read rule is a type distinction**.
- *RPC.* `rpc : transaction t -> transaction t` (L240) marks a client-to-server call.
- *Tier placement is not typed.* The manual says: "The type system does not enforce such restrictions, but the compiler enforces them in the process of whole-program compilation" (L1470). The `Rpcify` pass classifies code by which library functions it needs (L2669). `serverOnly`/`clientOnly` declarations add FFI functions to that classification (L2487–L2496).
- *Serialization is explicit:* `serialize : t -> serialized t` (L282).

---

## 3. Effect systems that can type "render may only read"

- **Koka.**
  - *Heap effects.* Koka splits heap effects into `alloc<h>`, `read<h>` and `write<h>`; `st<h>` is their combination. `run : forall<e,a> (action : forall<h> () -> <alloc<h>,read<h>,write<h>|e> a) -> e a` discards `st<h>` "whenever the heap type h cannot be referenced outside this function" (runST-style rank-2) ([Koka book §3.2.5](https://koka-lang.github.io/koka/doc/book.html#sec-runst)).
  - *`var`.* Local `var`s use a `local<h>` effect and "cannot be passed to other functions" (same section, §3.2.4).
  - *Consequence.* A render typed `() -> <read<h>|e> jsx` cannot write. Polymorphism over `h` also prevents escape. Both are standard row-effect machinery.
- **Eff vs OCaml 5.** "Unlike languages such as Eff and Koka, effect handlers in OCaml do not provide effect safety" ([OCaml manual, "Unhandled effects"](https://ocaml.org/manual/5.3/effects.html)), so OCaml 5 alone gives no render-purity guarantee; Bonsai relies on API shape plus OxCaml modes (§2). Foundation: Plotkin & Pretnar, ESOP 2009 **(not re-read)**.
- **Unison abilities.** The type form is `I ->{A1,A2} O`. "A function that definitely requires no abilities has a type like `A ->{} B`", and `handle e with h` discharges abilities ([language reference: abilities](https://www.unison-lang.org/docs/language-reference/abilities-and-ability-handlers/)). A `Render` ability with only `read` operations would be checked like any other ability.
- **Effekt.** Capabilities are tracked as captures: `(() => Nothing / {}) at {exc}`, and `f: () => Unit at {}` demands a pure block. A boxed computation "can only be used where those capabilities remain in scope" ([Effekt tour: captures](https://effekt-lang.org/tour/captures)).
- **Scala 3 capture checking.** This is the same `^` notation Flick uses. `T^{c}` is a capturing type, `T^` means `T^{cap}`, and a capturing type that instantiates a type variable "is not allowed to carry the universal capability `cap`". The `usingLogFile { f => () => f.write(0) }` example is rejected because "a capability persists longer than its allowed lifetime" ([Scala 3 reference: capture checking](https://docs.scala-lang.org/scala3/reference/experimental/cc.html)).
- **React semantics.** React-tRace (Lee, Ahn, Yi, OOPSLA 2025, [doi:10.1145/3763067](https://doi.org/10.1145/3763067)) formalizes hooks, untyped **(abstract only)**; no paper typing hooks with algebraic effects was found.
- **The contract being typed.** React requires components to be idempotent and "Side effects should not run in render" ([rules: purity](https://react.dev/reference/rules/components-and-hooks-must-be-pure)). The only allowed render-time write is the rendering component's own guarded state. "Calling the `set` function of *another* component during rendering is an error" ([useState: storing information from previous renders](https://react.dev/reference/react/useState)).

**Effect v4 `effect/reactivity` (the Flick target).** `AtomContext` gives an atom's read function both `get` *and* `set`, `setSelf`, `refresh`, `mount` and `addFinalizer`. The separate `WriteContext` has `get`/`set`/`setSelf` (`effect@4.0.5 dist/reactivity/Atom.d.ts` ~L150–L200). The runtime therefore does **not** separate reads from writes; Flick must do that in its own types. `Atom.make` accepts `(get: AtomContext) => Effect<A, E, Scope | AtomRegistry>` (L253–L263). `Atom.family: <Arg, T extends object>(f: (arg: Arg) => T) => (arg: Arg) => T` (L692) gives atoms stable identity per argument. `Atom.serializable({ key: string, schema: S })` with `S extends Schema.ConstraintCodec<Type<R>, any>` returns `R & Serializable<S>` (L1596–L1612). `Hydration.dehydrate(registry, ...) : Array<DehydratedAtom>` and `hydrate(registry, dehydrated)` move that state across (`Hydration.d.ts` L47, L79). Atoms are disposed when not in use unless wrapped in `keepAlive` (L725).

---

## 4. Synthesis: static guarantees Flick could provide

The "Cost" column describes what users and the inference engine pay.

| # | Guarantee | Who does it, how | Cost |
|---|---|---|---|
| 1 | **Hook/setup order fixed.** | react-basic-hooks: indexed `Render x y`. halogen-hooks: `Hook m h` with `h <> h'`. Bonsai: applicative `t`, branches fixed when the graph is built. ReScript/Feliz: none. | Prior art: indexed monad, qualified do, hook stacks in errors, newtype sealing. **Flick:** setup-once factory components sidestep it: setup runs once in a `Setup` region and render never allocates, so only an effect-row split of `setup`/`render` is needed (#3). |
| 2 | **No state write during render.** | halogen-hooks: write only in `HookM`. Ur/Web: no `set` in `signal`. Koka: `read<h>` without `write<h>`. react-basic-hooks: setters are `Effect` and render has no `MonadEffect` (escape hatch exists). | Low, given effect rows. Give render an effect row with an `AtomRead` capability and no `AtomWrite`. Actions carry `AtomWrite`. Inference already handles rows. Cost: no React-style "set own state during render". Derived atoms replace it. |
| 3 | **Render purity** (no services `r`/errors `e` in render beyond reading). | Elm/Lustre/Halogen `render :: state -> HTML`: language-level or by signature. Koka `total`/`pure`. Unison `->{}`. | Low. Type render as `props -> <AtomRead> Jsx` with empty service and error rows. Hard parts: suspense (reading an unresolved `AsyncResult`) needs a distinguished effect, and the `useAff`-style "rethrow into error boundary" needs an explicit `e` path. |
| 4 | **Stable identity of components, atoms and actions.** | react-basic-hooks: component creation is an `Effect` plus a name string. Feliz: convention only (2.x pitfalls page; current `[<ReactComponent>]` docstring says "defined at the module level", plus a PascalCase lint). Leptos: arena handles owned by `Owner`. Effect: `Atom.family` memoizes per argument. | Medium. Flick can require component, atom and action *definitions* to live at module scope or in setup, and type them as values of a `Static`/top region, so creating them inside render is a region error. Naming comes free from the binding name (better than PureScript's string). |
| 5 | **No capture of scoped resources by long-lived atoms.** | Scala CC: `T^{c}` and no `cap` through type variables. Effekt: `at {caps}` and second-class blocks. Bonsai: `graph @ local`. Koka/runST: rank-2 `h`. Leptos: runtime panic only. | This is Flick's strongest differentiator, since it already has `^c`. Rule: a module-level or registry atom's read function must be pure in its captures (`^{}`). Atoms created in a component's setup region may capture `^setup` and are disposed with it. Cost: capture-set inference in closures (Scala CC is still marked experimental). |
| 6 | **Serializability for SSR hydration.** | Leptos `Resource`: `Encoder<T> + Decoder<T>` bounds. Effect `Atom.serializable({key, schema})`. Lustre: manual JSON. Ur/Web `serialized t`. | Low to medium. A `Serializable a` type class (derivable) plus a *stable key*. Flick can derive the key from the module path and binding name (Effect requires a hand-written string). Requiring it only for atoms read during SSR needs a "read on server" effect or whole-program analysis. |
| 7 | **Server-only code never reaches client bundles.** | Leptos: `#[server]` with `cfg(feature="ssr")` and two builds. Ur/Web: whole-program tier inference with `serverOnly`/`clientOnly`, explicitly *not* in the type system. Lustre: separate packages by target. | Medium. Model the tier as a service in the effect row (`Server` service available only in the server entry). A client-reachable function whose row contains `Server` is a type error unless it is wrapped by a `server fn` construct that requires `Serializable` arguments and results and compiles to RPC. The bundler then drops server-row modules. Treat each `server fn` as a public endpoint (Leptos warning). |
| 8 | **Typed effect dependencies.** | react-basic-hooks / halogen-hooks: `Eq deps` / `captures`. | With atom subscription, dependencies are the atoms actually read (runtime-tracked, Effect `get`); no list to get wrong. Keep `Eq` as derived-atom cutoff (Bonsai `cutoff ~equal`). |
| 9 | **Per-request registry isolation (SSR).** | Effect: atoms are values interpreted per `AtomRegistry`. Leptos: `Owner` per request **(per-request owner not verified in source)**. | Make `AtomRegistry` a required service in the effect row of server render. A module-level atom *definition* is shared, while its *state* is per registry. Static rule: no module-level mutable cell outside atoms (Flick regions already police this). |

**Recommended shape for Flick.**

- *Setup once, render many.* `component Name(props) { setup: <Setup, AtomWrite?>; render: <AtomRead> Jsx }`. This keeps the guarantees of #1–#3 without PureScript-style indexed hook stacks.
- *Stable identity.* The binding name is the component, so #4 needs no extra syntax.
- *Captures and tiers.* Capture qualifiers give #5, and the effect-row tier gives #7.
- *Deliberately not adopted.* React's in-render self-`setState` pattern (#2) is left out because derived atoms subsume it.

---

## What was not verified

- Elm's lack of official SSR (absence not proven).
- Plotkin & Pretnar's paper body and the full React-tRace paper (abstract only).
- Whether Leptos uses one `Owner` per request in its server integrations.
- Scala capture-checking usability cost.
- Feliz `React.functionComponent` exact F# signature (only the compiled JS of 2.9.0 and the docs were read; the F# source is not in the current main branch).
- Line numbers marked `~` are approximate within a few lines.
