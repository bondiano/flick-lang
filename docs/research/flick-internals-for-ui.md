# Flick internals relevant to a UI-layer redesign

Scope: what the compiler and runtime already do that a move to Reatom-like atoms on `effect/reactivity` (with `{Read}` render, factory components, auto-naming, SSR/hydration, fullstack, CLI, observability) can reuse, and what it would have to change. Paths are relative to the repo root. Generated-JS samples come from `/var/cache/cargo-target/debug/flick build` on a scratch `Demo.fl`.

## 1. How effects compile

**Rule.** The IR carries one bit per stage, not the effect rows. `ir::Param { pat, runtime: bool }` (`flick-core/src/ir/mod.rs:357-361`) and `ExprKind::App { runtime: bool, .. }` (`ir/mod.rs:143-149`). Lowering copies the bit from the checker's side table, `Typing.runtime: HashSet<Range>` and `Typing.stages` (`types/typing.rs:32-37`), which `types/calls.rs:147,293` fill in. Read at `ir/lower/expr.rs:250,287`.

- The last stage is runtime, so the body is `Effect.gen(function* …)` (`codegen/js/function.rs:53-54,109-114`). Pure stages are plain arrows. A runtime stage that is not the last gets wrapped in `Effect.succeed(...)` (`function.rs:131-136`).
- A runtime call becomes `(yield* f(a))` (`codegen/js/expr.rs:191-194`, `run` at `expr.rs:240-245`). The codegen asserts that it sits inside a generator: `assert!(self.generator, "a runtime call runs inside a runtime stage")`.
- `task { }`, handlers and `provide`/`scope` bodies go through `gen_block` and become `Effect.gen` (`function.rs:188-199`). `provide` becomes `$rt.provide(layer, gen)` (`expr.rs:247-253`) and `scope` becomes `$rt.scope(owner => …)` (`expr.rs:255-264`).
- A **lift** (a pure fn where a runtime one is expected, spec 03:76) is `$f => $x => Effect.sync(() => $f($x))` or `Effect.map` (`expr.rs:414-435`).
- Spec: `Pure` vs `Runtime(r, e)` are separate markers, and "Empty `r` and `e` do not make a runtime computation pure" (`spec/04-computations.md:50-54`, `spec/03-direct-calls.md:18,76`). The module docs of the backend say the same: "Pure code is plain JS; a runtime stage is an `Effect.gen` whose runtime calls are `yield*`" (`codegen/mod.rs:1-4`).

**Foreign modes** (`ir/mod.rs:92-102`; spec `08-typescript-and-ffi.md:224-275`). Generated from the scratch example:

```js
// foreign pure hypot from global "Math.hypot"  arity 2
export const hypot = ($a0) => ($a1) => globalThis.Math.hypot($a0, $a1);
// foreign sync now_millis from global "Date.now"  arity 0     (Unit ->{} Number)
export const now_millis = ($a0) => Effect.sync(() => globalThis.Date.now());
// foreign effect fetch_user from "./users.js" "fetchUser"     (String ->{; AppError} String)
export const fetch_user = ($a0) => Effect.suspend(() => $f_fetch_user($a0));
```

`foreign async` uses `Effect.tryPromise` (spec 08:250-262).

**Services and a `->{}` "read" today:**

```js
export const $s_Log = $rt.service("Demo.Log");                // service Log = info : String ->{} Unit
export const label = (n) => ("count: " + $m_Instances.$i_Show_Number_fbf2eb85.show(n));   // pure
export const greet = (name) => Effect.gen(function* () {      // String ->{Log; none} String
  (yield* $rt.operation($s_Log, "info", 1)(name));
  return ("hi " + name);
});
export const render_count = (cell) => Effect.gen(function* () {  // Ref Number ->{} String
  return label((yield* $m_Ref.get(cell)));
});
export const view = (n) => { return Effect.gen(function* () { return label(n); }); };  // Number -> Eff none none String
```

**Could a `Read`-only render compile to plain sync calls?** Not today. Any non-pure arrow is `runtime=true` and becomes `Effect.gen`. The IR does not know which rows a stage has: `Expr` is just `{kind, span}` (`ir/mod.rs:105-108`). There are three ways to get there:

1. **No compiler change (cheapest).** Declare `service Read` (or a builtin `Ui.Read`) whose ops are `foreign sync`. The view `Props ->{Read} Ui` is then an `Effect.gen`. The React adapter runs it with `Effect.runSync` under a context providing `Read`, which records dependencies on the registry. This is correct, and every op is sync, so `runSync` never suspends. The cost is generator and fiber overhead on each render. A defect or async op inside render surfaces as an `AsyncFiberException`.
2. **Typed fast path (the real fix).**
   - Add a third stage kind. `Param.runtime: bool` becomes `enum Stage { Pure, Sync, Runtime }`, or a `sync` side table next to `Typing.runtime`.
   - The checker marks a stage as `Sync` when, at the end of the declaration, its solved services row is a subset of the builtin `{Read}` (plus maybe `Scope s`), its errors row is empty, and every call in it is to a `Sync` stage or a `foreign sync` op.
   - `perform` (`types/effects.rs:102-121`) already counts the calls of each stage, and rows are solved per declaration (`effects.rs:1-6`, `effects/solve.rs`).
   - Codegen then emits a plain arrow that takes a hidden reader (`($read) => …`, or a module-level ambient tracking slot) instead of `Effect.gen`. Calls become `f(a)($read)`.
   - A `Sync` function passed where a general runtime fn is expected needs a lift, like the existing `Lift` (`expr.rs:414-435`).
   - Files touched: `types/typing.rs`, `types/calls.rs`, `types/effects.rs`, `ir/mod.rs`, `ir/lower/expr.rs`, `codegen/js/function.rs`, `codegen/js/expr.rs`.
   - Watch out: `expr.rs` is already 17654 B, so put the new logic in a new module.
3. **Make render pure and read through an ambient JS tracker.** This contradicts spec 04:50-54 and the E001 test fixtures (§9). Reject it.

## 2. Effect rows, services, E001, new capabilities

**Declaring a service** (`examples/first-slice/src/App/Users.fl:32-36`):

```flick
service Users =
  get : UserId ->{; NotFound} User
```

- The checker generates the operations and `Users.Impl` (`types/services.rs:1-6`).
- Codegen emits `export const $s_Users = $rt.service("App.Users.Users")` (`codegen/js/items.rs:76-82`). That is a `Context.Service(key)` with a qualified key (`runtime/src/effects.ts:8`).
- An op call is `$rt.operation(tag, name, arity)`, which does `Effect.service(tag)` and then calls the impl (`effects.ts:27-33`).

**Providing it.** A layer is built with `Layer.succeed Users { get = fun id -> … }` (`examples/first-slice/src/App/Live.fl:14-21`; `Layer.fl:15`). It is supplied with `provide layer { … }`, which becomes `Layer.buildWithScope` + `Effect.provideContext` inside an owner (`runtime/src/layer.ts:9-15`). Other constructors: `Layer.effect` (`Layer.fl:53`), `Layer.merge`, and `from_build : (forall s. MemoMap ->{Scope s | ri; e} Context ro) -> Layer …` (`Layer.fl:188`). Database is `service Database` plus `Database.pg : PgConfig -> Layer none none {Database}` (`builtins/Database.fl:6-26`).

**E001.** The code is `diagnostics/codes.rs:147`. The decision is made in `Checker::perform` (`types/effects.rs:102-112`):

```rust
if let Some(reason) = &self.stage.pure {
    let message = format!("this is a runtime call, but {reason}");
    ... self.error(codes::E001, span, message);
```

`Stage.pure: Option<String>` is set when the function's last arrow is `->` (`types/infer.rs:272-278`). It is also set by default for module initialization (`effects.rs:64-67`: "module initialization is pure: runtime calls start at the entry point"). The checker output for a pure view that reads a Ref:

```
error[FLICK-E001]: this is a runtime call, but `view` is pure: its signature has `->`, not `->{…}`
```

**Adding a built-in effect or capability.**

- A builtin module is a `.fl` file registered in `types/builtins.rs` (`("Scope", include_str!("builtins/Scope.fl"))` at `:179`). Most capabilities are ordinary `service` declarations with `foreign` ops.
- `Scope s` is the exception. It is a primitive row entry `Prelude.Scope` (`types/regions/mod.rs:27`) that the innermost `scope` supplies (`regions/scopes.rs:135-157`).
- A `Read` capability for option 1 is just `service Read` plus foreign ops. For option 2 it also needs a known key, the way `SCOPE` is a known key, so the checker can recognise a "sync-only" row.
- `effect/reactivity` is currently excluded in the Effect coverage manifest: `"effect/reactivity" = { excluded = "reactive keys for UI caches are outside 0.1.0" }` (`stdlib/Effect/surface/entrypoints.toml:18`). Wrapping it means moving it to `modules`, classifying every export, and passing the `cargo test` linter (`reference/effect-surface.md`; implementation-decisions.md:16).
- Effect 4.0.5 ships `effect/reactivity/{Atom,AtomRegistry,Hydration,AtomRef,AtomHttpApi,AtomRpc,Reactivity}`. Relevant exports: `Atom.make`, `family`, `keepAlive`, `withLabel`, `serializable({key, schema})`, `get`/`set` (which need `AtomRegistry`), and `Hydration.dehydrate`/`hydrate`/`toValues`.

## 3. Regions and captures

- `scope s { … }` makes a fresh rigid region one level deeper. Operations that acquire ask for the service `Scope s`. Hidden captures of closures and tasks go into the qualifier `T^{s}` and are solved with the rows (`types/regions/mod.rs:1-12`). Spec: `spec/07-errors-and-concurrency.md:246-317`.
- **G001** "value holds more than its capture qualifier allows" is raised at `regions/solve.rs:186-199`, with the fix "add the region to the qualifier". G002 (escape) is at `regions/mod.rs:410` and `solve.rs:173`. G003 is at `solve.rs:264`.
- Runtime: `$rt.scope = withOwner` and `$rt.within = inside` (`runtime/src/owner.ts:120,126`). The close order is fixed by spec: "forbid new work → cancel children → wait → finalizers LIFO → Exit" (spec 07:334).
- The current UI already relies on this. Every `Ui` constructor is `foreign pure … retention call` with `(Ui r e)^c` (`builtins/Ui.fl:125-127`). Tests pin "handler escaping a scope: G001" (§9).

**Can a component-instance scope reuse this?** Yes. There is a higher-rank precedent: `Layer.from_build : (forall s. MemoMap ->{Scope s | ri; e} …)` (`Layer.fl:188`) and `Test.effect_test : … (forall s. Eff {Scope s} e Unit)` (`Test.fl:50`). A factory component can be typed like this:

```flick
component : (forall s. p ->{Scope s | r; e} (p ->{Read} Ui r e)^{s}) -> Component p
```

- Setup runs in a per-instance Effect Scope that the adapter opens on mount and closes on unmount (React effect cleanup). It follows the existing `uiRunWith` close order (`runtime/src/ui/react/index.ts:155-163`).
- Local atoms created during setup are region-bound values (`Atom s a`). The render closure may capture them (`^{s}`). G002 stops them from leaking into a global Ref or cache, and "Mutable storage is invariant" (spec 07:300) already covers writing them to longer-lived storage.
- Atoms must not generalize. `Ref` avoids generalization because `Ref.make` is `->{}` (`builtins/Ref.fl:1-3`; `types/generalize.rs:1-3`: "the result of a runtime call stays monomorphic"). A top-level atom must be a pure construction, because module init is pure (`effects.rs:66`). Pure bindings generalize, so a `foreign pure Atom.make` needs an explicit non-generalization rule or an invariant/expansive marker. Otherwise `empty : Atom (List a)` would be polymorphic and unsound.

## 4. Naming, profiles, source maps

- **Qualified names are available at emit.** `ItemKind::Value` emission builds `Target::Global(format!("{}.{name}", self.module))` (`codegen/js/items.rs:42,60`). Services already get a qualified runtime key: `$rt.service("Demo.Log")` (`items.rs:76-82`). Constructor ids use `qualified_id` = `module.ty.name`, prefixed with the package when there is one (`codegen/names.rs:107-113`). Locals have "unique names in the enclosing top-level definition" (`ir/mod.rs:113-114`), so `Mod.fn.local` labels for setup-local atoms are also derivable.
- **Profiles.** Debug is the reference lowering. Release runs `opt::optimize` first: fold, inline, sroa (`flick-core/src/opt/`, `build/mod.rs:197-202`). Codegen then adds worker functions `name$N` for full applications and direct canonical-instance methods (`codegen/js/release.rs:1-3`; `codegen/js/mod.rs:45-57`; `codegen/mod.rs:32-34`). In the scratch example debug and release output are identical. Inlining in release can erase a wrapper call, so a label must be a literal argument of a runtime call, not something recovered from the function name.
- **Source maps.** These are v3 maps from per-node spans (`codegen/sourcemap.rs:1-2`). The `names` array is always empty (`sourcemap.rs:66`), so devtools show JS identifiers. `flick run` uses `node --enable-source-maps` (`flick/src/commands/run.rs:112-121`).
- **Feasibility of auto-naming.** This is cheap and needs no checker work. In `items.rs`, for a top-level `Value` whose body is an application of a known builtin (`Atom.make`/`Ui.component`/`Action.make`), wrap the emitted value: `export const x = $rt.label("App.Counter.count", <value>)`. The runtime then calls `Atom.withLabel` or sets `displayName`. The same can apply to setup locals, in debug only or in both profiles.
  - `items.rs` is 18837 B (about 1.1 KB headroom under the 20000 B guard), so put the logic in a new `codegen/js/labels.rs`.
  - A simpler variant is to do it for every exported function value in debug: `Object.defineProperty(f, "name")` gives `displayName` for free.
  - Hydration keys should stay explicit or be derived from the qualified name plus `Codec`. A rename then changes the key, which is fine for SSR because it is per build.

## 5. Codec and Schema deriving for hydration

- Only `Schema` is derivable. `Codec` is not a class: `classes/derive.rs:189-190` says `// a schema witness, not a class`, and other names get C013. Records, ADTs, Option/Result, collections, generics, recursion and refinements are all covered (`spec/09-schema.md:145-170`). Grammar: `deriving Schema`, `schema T`, `codec w { … }` (09:74).
- JSON: `json : Schema a -> PureCodec Json a` "records as objects, variants as `{"_tag": "Name", "_0": …}`, `Option` as the value or `null`" (`builtins/Codec.fl:52-56`). Use it with `Codec.encode/decode` (`Codec.fl:14-46`) and `Json.stringify/parse` (`builtins/Json.fl:5,10`). Runtime: `schemaJsonCodec = (schema) => registerCodec(jsonOf(schema))` (`runtime/src/schema/index.ts:173`).
- **The ABI is native Effect Schema** (spec 09:7: "`Schema` … library types with the native Effect ABI"; `runtime/src/schema/index.ts:4`). That means `Atom.serializable({ key, schema: jsonOf(flickSchema) })` and `Hydration.dehydrate`/`hydrate` can take Flick witnesses directly, with no adapter.
- Limits: fields that are functions, `Ref`s or resources are not derivable (spec 09:191-195), and `Redacted` needs a policy. Hydrate atom **values**, never handles or handlers.
- Today **no model is serialized**. SSR reuses markup only, and both sides rebuild the initial model from `data-*` config (`runtime/src/ui/index.ts:279-297`; `docs/src/components/Playground.astro:15-18`). Nothing in `runtime/src/ui` uses Codec, Schema or Json.

## 6. Server and fullstack pieces

- `App.fl` contains only `fail : String ->{; AppError} a` (`builtins/App.fl:2-5`). `Request.fl` is Effect request batching, not HTTP (`Request.fl:16-60`).
- **`HttpApi`** (409 lines) is a typed API whose type is the record of handlers. Endpoint parts use codecs from `Schema a` (implementation-decisions.md:41).
  - `listen : ServeOptions -> Api gs -> (gs^c ->{Scope s | r; ServeError} Server s)^c` (`HttpApi.fl:280`).
  - The typed client `call base endpoint request` is at `HttpApi.fl:295`. OpenAPI generation is at `:310`.
  - Example: `tests/run/stdlib/http/api.fl:137-173`.
  - Runtime: `runtime/src/httpapi/server.ts` on `@effect/platform-node`.
- `Http.Server.serve` (`Http/Server.fl:31-34`), plus Router, Middleware, Cookies, StaticServer and TraceContext (25 modules under `builtins/Http/`).
- **No RPC.** `"effect/rpc" = { excluded … }` (`entrypoints.toml:19`). "Server functions split by effect row" would be new. The closest base is an `HttpApi` endpoint plus a typed client generated from a shared definition, where the server-only services in the row decide which side the code goes to.
- `examples/first-slice` is a console app with services, layers, `parallel` and per-test layers (`src/App/Main.fl:24-26`, `test/App/UsersTest.fl`). It is not a server.
- **`flick run`** builds each generation into `.flick/run/<n>/` and runs `node --enable-source-maps` (`flick/src/commands/run.rs:1-6,59-121`).
- **`flick dev` restarts the process; it does not hot-replace.** It polls every 200 ms. After a successful build it sends SIGTERM to the old process, waits for finalizers, and starts the new one. A failed build keeps the old one running (`flick/src/commands/dev.rs:55-82`). The spec says: "Automatic in-process hot reload that keeps old scoped handles is not part of the first version" (`spec/12-cli.md:104`).
- **Vite/Astro.** `tools/astro-flick` (`@flick-lang/astro`). `vite.mjs:40-69` hashes the `.fl` files in the module's directory, runs `flick build <file> --out-dir <cache>/<hash> --profile …`, and returns `export * from "<out>/Mod/Path.js"`. Each source is a watch file, so an edit gives Vite new modules (HMR through module invalidation, with no state preservation). The integration is in `index.mjs:7-15` and used by `docs/astro.config.mjs`. SSR and client share one build (`vite.mjs:52`).

## 7. Observability

- **Tracer.** `with_span : String -> SpanOptions -> (Eff r e a)^c ->{r; e} a` (`builtins/Tracer.fl:68`). The options record has no defaults (`tests/run/stdlib/effect/tracer.fl:35-37`). There are span inspection helpers, and `external_span` for a parent from another process.
  - The only constructible tracer is `native_tracer`, "keeps spans in memory only" (`Tracer.fl:127-128`). `with_tracer` exists (`:133`), but Flick cannot build another `Tracer` value.
  - `Http/TraceContext.fl:9,16` propagates trace headers.
- **Logger.** Formats (`Logger.fl:94-109`), console loggers (`:114-139`), `tracer_logger` (logs as span events, `:144`), `layer`/`batched`/`to_file`/`with_loggers` (`:149-172`), and `effect_log : Layer none none {Log}` (`:192`). `Log` is a plain service with `Log.console` (`Log.fl:4-23`).
- **Metric.** counter, gauge, frequency, histogram, summary and timer (`Metric.fl:99-132`). `snapshot`/`dump` read values in process (`:180,186`). There is no exporter.
- **OTel today: not wired.** `"effect/observability" = { excluded = "exporters for observability backends are outside 0.1.0" }` (`entrypoints.toml:15`). There is no OTLP dependency in `runtime/package.json` (it has effect, @effect/platform-node and pg). The only route is a hand-written `foreign effect` around Effect's OTLP layer, plus a new `Tracer` or `Layer` binding.
- Hooks for the UI: compiler-known labels (§4) can become span names (`Effect.fn("Mod.name")` / `withSpan`) on actions, and `Atom.withLabel` on atoms. That is the cheapest observability win.

## 8. CLI as a product

- `Cli.fl` (395 lines) wraps `effect/cli`:
  - arities: `switch`, `required`, `default`, `optional`, `many`, `between` (`Cli.fl:73-113`);
  - value sources: `from_config`, and `from_prompt` (ask when the value is missing) (`:120,126`);
  - file and directory checks (`:131-176`);
  - commands, subcommands, aliases, examples and actions (`:182-277`);
  - help, shell completions and `run_with` (`:328-380`).
- The `cli "name" { flag …; arg …; subcommand …; handle fun {…} -> … }` block elaborates to `Cli.*` (`reference/cli-dsl.md:11-58`; spec 02:23). Field types pick the combinator. `deriving Cli.Value` works for enums and refined types. `--help`, `--version` and `--completions` are built in.
- `Cli/Prompt.fl` (305 lines): `text`, `password`/`hidden` (both `Redacted`), `int`, `number`, `confirm`, `toggle`, `date`, `file`, `select`, `auto_complete`, `multi_select`, `custom` and `custom_events`, combined with `all`/`map`/`flat_map` (`:76-303`). `run : Prompt a ->{Tty; Terminal.QuitError} a` takes a scripted `Tty` in tests (`:30`).
- What a user can build: typed multi-command tools with config and env fallback, interactive wizards and completions, all over the same services and layers as the server. There are no examples (only `tests/run/stdlib/cli/{cli,surface,prompt,cli_exit}.fl`). There is no TUI rendering. A `Ui` renderer for the terminal would need `Ui.run_with`/`Renderer` (`Ui.fl:188-191`), or atoms that drive a terminal view.

## 9. Tests that pin current UI behavior

Current model, for orientation:

- `Ui r e` is an inert tree (`Ui.fl:1-5`).
- Views are pure, `m -> (Ui r e)^c`, and roots take a `Ref m`: `Ui.Dom.run : Container -> supervisor -> Ref m -> (m -> (Ui r e)^c)^c ->{r; none} Never` (`Ui/Dom.fl:22-25`; `Ui.run_with` `Ui.fl:188-191`).
- Handlers are only made by policies `exhaust`/`latest`/`queue n`/`concurrent n`/`prevent_default` (`Ui/Event.fl`; `runtime/src/ui/react/policies.ts:175-215`).
- The renderer is React 19 (`react/index.ts:74-166`). It skips the render when the model is the same object, and re-renders when a task settles.

**Must be rewritten or migrated:**

- `runtime/test/ui.test.ts` (19300 B, near the 20000 B guard: split it, do not add to it):
  - "a task's result renders again" (:185);
  - "same-model completions skip the view and a synchronous task burst batches changed models" (:302);
  - the lazy-view group (:329, :370, :405), if atoms replace `lazy_view`.
- `runtime/test/ui-hydration.test.ts` (:32) uses `refMake`/`refSet`.
- `tests/run/ui/counter.fl` and `directory.fl`, with their `.stdout` files.
- The E001 cases "state read while rendering", which pin a **pure** view: `tests/check/fail/ui/props.fl` and `children.fl`. These turn into `{Read}`-allowed reads.
- `tests/check/fail/ui/lazy.fl` and `tests/check/pass/ui/lazy.fl`: the `Eq` and comparator cases and the `run_with` E001/E002 cases.
- Every docs app that threads a `Ref`:
  - `docs/src/playground/{Playground,View,Toolbar,Results,Editor,Dom}.fl`;
  - `docs/src/widgets/{Copy,Elements,Hero,Controls,Search}.fl`;
  - the SSR hosts `Playground.astro` and `Header.astro`.
- `reference/ui.md` sections: Running a root (:141), Server-rendered initial views (:175), Elm-like (:225), gaps (:289). Also the `ru/` twin.

**Keep as is:**

- node constructors, exact props, slots, `NonEmpty` and `SafeUrl` (T003 cases), and the text channel (ui.test :70);
- the handler policies and U001 (:84-:176, :435; `types/ui.rs:12-28`);
- close and supervision order (:196, :225, :468, :480);
- the React-only entrypoint and StrictMode (:235, :262);
- the G001 escape cases;
- `uiRenderStatic` and `data-flick-ssr`;
- the parser fixtures `tests/syntax/expressions.fl:107-111`.

## 10. Constraints

- **Size guard.** Each changed staged file must be at most 20000 bytes (staged blob). Files that already exceed it in HEAD are exempt until they change. `docs/` at any depth is exempt; code anywhere else is checked. These are close to the limit:
  - `runtime/test/ui.test.ts` 19300;
  - `codegen/js/items.rs` 18837;
  - `codegen/js/expr.rs` 17654;
  - `types/decls.rs` 19660;
  - `types/unify.rs` 19645;
  - `types/toplevel.rs` 19117;
  - `types/calls.rs` 15756;
  - `types/infer.rs` 15408;
  - `ir/lower/expr.rs` 16103.

  The fix is to add new modules (`codegen/js/labels.rs`, `types/read.rs`, `runtime/src/ui/atoms/*.ts`) rather than grow these files.
- **`spec/02-decisions.md` rows a redesign must respect:**
  - "Pure `a -> b`, runtime `a ->{r; e} b` and runtime without requirements/errors `a ->{} b` are distinguished in the checker and the IR" (:28). A `Sync` stage would be a fourth IR kind, which needs a spec amendment in 03/04.
  - "Generalization | Only provably pure constructions; runtime results are monomorphic" (:11). See the atom-generalization risk in §3.
  - "Lifetime | Fresh regions, captures, no early close" (:16). A component scope closes on unmount, so it must use the same close protocol.
  - "Concurrency | … no detached scoped work" (:18).
  - "FFI | Fixed ABI … checked `.d.ts` subset" (:19). Atoms go through `foreign` with `reviewed` fingerprints, like `Ref.fl`.
  - **"UI | ui block, React adapter, explicit event policy | No JSX, no custom renderer, no SSR and no full hooks model" (:22).** This row must be revised. Atoms, SSR hydration and component setup scopes contradict "no SSR" and "no full hooks model". "No custom renderer" is already soft (`Ui.run_with`).
  - "CLI | … dev …" (:26), together with spec 12:104: there is no in-process hot reload, so atom-state-preserving HMR is out of scope unless the spec changes.
- **`contributing/implementation-decisions.md`:**
  - Effect pinned at exactly 4.0.5 (:15). `effect/reactivity` is "unstable"-tagged in 4.0.5 (`Atom.d.ts` `@stability unstable`).
  - Effect API coverage manifest plus linter (:16). Including `effect/reactivity` needs a new manifest decision (`entrypoints.toml:18`).
  - React 19 is an optional peer (:19).
  - HTTP has no syntax and lives in library combinators (:41). Server functions should be library-level, or need a new decision.
- **Spec 04:30.** Module init is pure and IO starts at the entry point. A top-level atom must be a pure description (Effect atoms are), and the registry is created at the run boundary. That fits per-request registries for SSR.
