---
title: "13. The first working slice of Flick"
---

The first executable prototype checks the compatibility of HKT, higher-rank callbacks, classes, JS numbers, deriving Schema, direct effects and source diagnostics. The work can be split into implementation stages without excluding any of these capabilities from the target language.

## End-to-end example

```flick
opaque type alias UserId = String

type alias User = {
  id : UserId;
  name : String;
}

type alias UserView = {
  id : UserId;
  title : String;
}

to_view : User -> UserView
let to_view user =
  {
    id = user.id;
    title = String.trim user.name;
  }

load_view : UserId ->{Users, Log; NotFound} UserView
let load_view id =
  let user = Users.get id
  let view = to_view user

  Log.info ("Loaded " ++ view.title)

  view

load_two : UserId -> UserId ->{Users, Log; NotFound} (UserView, UserView)
let load_two first_id second_id =
  let first = task { load_view first_id }
  let second = task { load_view second_id }

  parallel first second
```

In the example, to_view compiles to a pure TS function, load_view returns a native Effect, and load_two explicitly creates and runs deferred tasks. Users/Log are declared with the chosen service syntax; implementations are assembled by Layers and provided in an explicit entry point.

## Prototype contents

1. A strict functional core, ADTs, records, `match` and modules.

2. Kinds, HKT, annotated higher-rank callbacks, Show/Functor and a row relation to check the chosen MPTC/FD core.

3. Direct effectful calls with an ordinary `let` and first-class `Eff`.

4. Inference of error rows and services, and a typed failure handler.

5. The `Users` service with a test implementation and an external one.

6. Generation of executable code, Float/BigInt semantics and explicit FFI callback families.

7. A scoped resource, escape checking through task/closure, and one concurrent-execution operation with the correct completion order.

8. Deriving Schema on a generic type and a recursive type; source diagnostics, hover and the TS projection from the same core.

9. A small `.d.ts` importer on top of explicit bindings and a check of several portable ReScript optimizations.

10. End-to-end SQL/UI examples of the chosen row/type-level core: a PostgreSQL LEFT JOIN and a React handler with task/capture checking.

## Implementation verification criteria

- [x] Applying a constructor with the wrong kind is rejected in the source code.
  Tests: `tests/check/fail/types/kinds.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `tests/check/pass/types/kinds.fl`.

- [x] A missing or ambiguous instance gives a clear source diagnostic.
  Tests: `tests/check/fail/classes/resolution.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `tests/check/fail/classes/coherence.fl`.

- [x] `Functor` works with `Option` and `Eff r e`.
  Tests: `tests/check/pass/classes/functor.fl`; `tests/run/codegen/functor.fl`; `flick/tests/e2e.rs` (`run_cases`).

- [x] Pure functions inside the single Effect backend compile to plain JS without superfluous Effect wrappers.
  Tests: `flick-core/src/codegen/tests.rs` (`pure_functions_are_plain_js`, `runtime_stages_yield_only_on_runtime_calls`); `flick-core/src/codegen/snapshots/flick_core__codegen__tests__runtime_stages_yield_only_on_runtime_calls.snap`.

- [x] `task` does not run its body at construction; running it again runs it again. `foreign effect` also defers the call of the JS factory.
  Tests: `tests/run/codegen/task_twice.fl`; `tests/run/ffi/foreign.fl`; `tests/run/ffi/bindings.mjs`; `flick/tests/e2e.rs` (`run_cases`).

- [x] Storing a task in a variable or passing it through `identity` does not start it.
  Tests: `tests/run/codegen/task_twice.fl`; `tests/run/ffi/foreign.fl`.

- [x] A direct call binds the source result: `let user = Users.get id` gives `User`. Every currying stage preserves order; a runtime call with empty rows does not become pure.
  Tests: `tests/check/pass/effects/direct.fl`; `tests/check/fail/effects/purity.fl`; `tests/run/codegen/stage_order.fl`; `tests/run/codegen/eval_order.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `flick/tests/e2e.rs` (`run_cases`); `examples/first-slice/`; `flick/tests/first_slice.rs`.

- [x] Typed handlers and the residual error row agree with the native Cause; a safe RawCause does not expose a live JsValue, a report does not call getters/toString, and a rethrow preserves the original Cause and the allowed e.
  Tests: `tests/check/pass/effects/errors.fl`; `tests/check/fail/effects/rows.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `tests/run/codegen/errors.fl`; `tests/run/stdlib/effect/exit_cause.fl`; `tests/run/stdlib/effect/effect_errors.fl`; `flick/tests/e2e.rs` (`run_cases`); `runtime/test/cause.test.ts`; `tests/check/fail/effects/rethrow.fl`.

- [x] `provide` runs the body after the environment is set up and accounts for dependencies and layer construction errors.
  Tests: `runtime/test/layer.test.ts`; `tests/run/stdlib/effect/layer.fl`; `tests/run/codegen/services.fl`; `tests/check/fail/effects/rows.fl`; `tests/check/pass/effects/services.fl`; `examples/first-slice/`; `flick/tests/first_slice.rs`.

- [x] Typed failure, defect and interruption are not mixed.
  Tests: `runtime/test/cause.test.ts`; `runtime/test/concurrency.test.ts`; `tests/run/stdlib/effect/exit_cause.fl`; `tests/run/stdlib/effect/effect_errors.fl`; `tests/run/codegen/errors.fl`; `tests/run/codegen/defect.fl`; `tests/run/ffi/defect.fl`.

- [x] Scope releases the resource on success, error and cancellation within the chosen runtime contract.
  Tests: `runtime/test/scope.test.ts`; `tests/run/runtime/scopes.fl`; `tests/run/runtime/owners.fl`; `tests/run/runtime/releases.fl`.

- [x] `parallel` follows the chosen rules for cancellation, errors and cleanup.
  Tests: `runtime/test/concurrency.test.ts`; `runtime/test/race.test.ts`; `tests/run/runtime/concurrency.fl`; `tests/run/runtime/releases.fl`.

- [x] Hover shows source types, and the executable and editor outputs agree. Empty rows, `Unit` and `Never` are not mixed in diagnostics and the TS ABI.
  Tests: `flick-core/src/ide/tests.rs` (`hover_shows_source_types_with_rows_and_captures`, `hover_on_a_keyword_shows_the_type_of_its_whole_block`); `flick/tests/lsp_editor/mod.rs` (`hover_covers_private_functions_and_ignore`, `hover_distinguishes_errors_services_types_and_constructors`); `flick/tests/lsp_command.rs` (`hover_definition_completion_rename_and_formatting`, `diagnostics_match_flick_check_in_utf16`); `flick-core/tests/projection.rs` (`projection_and_executable_share_the_analysis`); `flick/tests/wasm_demo.rs` (`the_demo_compiles_a_module_as_flick_build_does`, `the_demo_reports_diagnostics_as_json_events`); `tests/check/fail/types/empty_rows.fl`; `flick-core/src/codegen/ts/tests.rs` (`unit_never_and_empty_rows_have_distinct_typescript_types`, `exports_have_typescript_types_without_any`); `tests/check/pass/effects/errors.fl`.

- [x] A higher-rank callback is applied to different types; a faulty monomorphic implementation is rejected; class dictionaries and external rows are not lost.
  Tests: `tests/check/pass/types/higher_rank.fl`; `tests/check/fail/types/higher_rank.fl`; `tests/run/codegen/classes.fl`; `tests/run/codegen/higher_rank.fl`; `flick/tests/e2e.rs` (`run_cases`).

- [x] Float preserves NaN, infinity, signed zero and the JS rounding order in the reference and the optimized codegen.
  Tests: `flick/tests/e2e_numbers.rs` (`numbers_match_js`); `tests/run/codegen/numbers.fl`; `flick/tests/e2e.rs` (`run_cases`); `flick-core/tests/consteval_node.rs` (`folding_matches_node`); `flick-core/src/consteval/tests.rs` (`folds_number_arithmetic_with_js_semantics`); `flick-core/src/opt/tests.rs` (`literal_expressions_fold_with_js_semantics`, `forbidden_transformations_stay_out`).

- [x] Deriving Schema works for generic, higher-kinded, recursive and mutually recursive data; unsupported automatic serialization gets a precise diagnostic.
  Tests: `tests/check/pass/schema/derive.fl`; `tests/run/schema/derive.fl`; `tests/check/fail/schema/codecs.fl`; `runtime/test/schema.test.ts`.

- [x] The JS-callback adapter respects the result, arity/this, reentrancy, environment, cancellation and lifetime of the chosen family.
  Tests: `runtime/test/callbacks.test.ts`; `tests/run/ffi/events.fl`; `tests/run/ffi/foreign.fl`; `flick/tests/e2e.rs` (`run_cases`); `tests/check/fail/ffi/metadata.fl`.

- [x] A scoped handle does not escape through a result, callback, task, container, error payload or a write to external state; an interface does not erase captures.
  Tests: `tests/check/fail/regions/escape.fl`; `tests/check/fail/regions/qualifiers.fl`; `flick-core/tests/check.rs` (`check_fixtures`, `project_fixtures`); `tests/check/projects/regions/`; `flick-core/src/iface.rs` (`tests::schema_is_pinned_and_round_trips`).

- [x] The owner finishes all children before releasing resources; async FFI waits for quiescence after cancellation, even when the underlying Promise did not stop together with the fiber. An uninterruptible child delays closing.
  Tests: `runtime/test/scope.test.ts`; `tests/run/runtime/scopes.fl`; `tests/run/runtime/owners.fl`; `flick/tests/e2e.rs` (`run_cases`).

- [x] The `.d.ts` importer shows unsupported declarations and the required policies without an implicit `any`, pure or cancellation.
  Tests: `flick-core/src/ffi/tests.rs` (`any_pure_and_cancellation_are_never_implied`, `without_policies_promises_callbacks_and_nullable_results_are_blocked`, `strict_mode_refuses_unknown`, `policies_are_checked`); `flick/tests/ffi_command.rs` (`import_review_update_and_check`); `flick/tests/snapshots/ffi_command__lookup_report.snap`; `tools/dts-importer/test/import.test.mjs`.

- [x] Local optimizations preserve currying stages, JS Number and the native Effect ABI; the reference and optimized codegen agree on observable behavior.
  Tests: `flick-core/src/opt/tests.rs` (`a_full_application_calls_the_worker_and_a_partial_one_keeps_the_closure`, `forbidden_transformations_stay_out`, `unused_pure_code_is_dropped_and_effects_stay`); `tests/run/codegen/release.fl`; `tests/run/codegen/stage_order.fl`; `flick-core/src/codegen/tests.rs` (`runtime_stages_yield_only_on_runtime_calls`); `tests/run/codegen/effect_abi.fl`; `flick/tests/e2e.rs` (`run_cases`); `tests/run`.

- [x] Strict schema export rejects an unrepresentable wire contract; this is separate from the permitted AOT interpreter fallback.
  Tests: `flick/tests/schema_export.rs` (`strict_export_refuses_what_it_cannot_state`); `runtime/test/schema.test.ts`; `tests/run/schema/contracts.fl`; `flick/tests/build_command.rs` (`schema_aot_writes_compiled_decoders_the_runner_installs`).

- [x] The OpenAPI 3.1 export of `HttpApi` is valid against the OpenAPI 3.1 specification, contains no `{}`/`any`, and rejects an unrepresentable endpoint schema with its path.
  Tests: `runtime/test/httpapi.test.ts`; `runtime/test/fixtures/openapi-3.1.schema.json`; `tests/run/stdlib/http/api.fl`; `flick/tests/schema_export.rs` (`strict_export_refuses_what_it_cannot_state`); `flick/tests/cli_run.rs` (`the_spec_examples_and_the_ci_set_pass`).

- [x] Schema independently checks missing/null/undefined/defaults/extra keys; an effectful codec does not receive a PureCodec certificate.
  Tests: `runtime/test/schema.test.ts`; `tests/run/schema/policies.fl`; `tests/check/fail/schema/codecs.fl`.

- [x] SQL rejects wrong fields, GROUP BY, alias scope, parameters and generated INSERT columns; LEFT JOIN changes nullability, and a pure build performs no I/O.
  Tests: `tests/check/pass/regions/captures.fl`; `tests/check/fail/regions/qualifiers.fl`; `tests/check/fail/regions/escape.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `tests/check/projects/regions/`; `flick-core/src/iface.rs` (`tests::schema_is_pinned_and_round_trips`).

- [x] UI rejects a runtime call in render and an escaping handler; the event policy limits work, the supervisor observes failures, and React cleanup keeps resources until join.
  Tests: `tests/check/fail/sql/queries.fl`; `tests/check/pass/sql/balances.fl`; `tests/run/sql/queries.fl`; `runtime/test/sql.test.ts`; `flick/tests/sql_postgres.rs` (`queries_against_postgres`); `tests/postgres/app.fl`; `flick-core/src/ide/tests.rs` (`captures_in_sql_are_names_of_the_program`); `tests/check/fail/sql/balances.fl`.

- [x] Curried generic combinators preserve the captures of the source container, the callback and the partial application; wrappers and iface do not erase lifetime.
  Tests: `tests/check/fail/ui/props.fl`; `runtime/test/ui-roots.test.ts`; `tests/run/ui/counter.fl`; `tests/check/pass/ui/save_button.fl`; `tests/run/ui/directory.fl`; `tests/check/fail/ui/children.fl`.

- [x] The CLI uses a single Rust checker: `check` and `build` do not start the application, do not perform effectful initialization of foreign modules and do not install dependencies; `run`, `dev` and `test` build executable JS.
  Tests: `flick/tests/cli_project.rs` (`check_and_build_run_no_code_of_the_program`); `flick/tests/lsp_command.rs` (`diagnostics_match_flick_check_in_utf16`); `flick/tests/wasm_demo.rs` (`the_demo_compiles_a_module_as_flick_build_does`); `flick/tests/cli_run.rs` (`run_builds_javascript_even_for_a_typescript_project`, `dev_replaces_the_app_only_after_its_shutdown`); `flick/tests/test_command.rs` (`outcomes_are_reported_apart`).

- [x] `dev` isolates the artifacts and resolved dependencies of the active generation; the next version starts only after the previous one's join, foreign quiescence and finalizers. A cleanup failure stops the restart and is preserved in the final Cause.
  Tests: `flick/tests/cli_run.rs` (`a_running_generation_keeps_its_files`, `dev_replaces_the_app_only_after_its_shutdown`, `dev_starts_the_next_version_after_foreign_work_settles`); `runtime/test/scope.test.ts`.

- [x] The cache separates the effective configuration, profiles, emit modes, toolchain/runtime and dependency fingerprints; `clean` preserves active generations and user files.
  Tests: `flick/tests/cli_project.rs` (`the_cache_key_follows_every_input_and_a_miss_changes_nothing`, `the_installed_runtime_and_effect_are_in_the_cache_key`, `clean_keeps_user_files_and_running_generations`); `flick/tests/cli_run.rs` (`a_running_generation_keeps_its_files`).

- [x] `install --locked` rejects a missing or stale lockfile; the actual pnpm version is checked before dependencies are changed, and an incompatible compiler/runtime/Effect blocks compilation and running.
  Tests: `flick/tests/cli_project.rs` (`a_wrong_pnpm_or_a_missing_lockfile_changes_nothing`, `a_stale_lockfile_is_refused_and_pnpm_speaks_ndjson`, `an_incompatible_runtime_blocks_the_build`, `an_incompatible_effect_blocks_build_and_run`); `flick-core/src/project/pnpm.rs` (`locked_and_offline_map_to_pnpm_flags`).

- [x] JSON diagnostics remain valid versioned NDJSON when the application, tests and package manager produce output; LSP stdout and the JSON test reporter keep their own protocol, and exit codes account for the cleanup result.
  Tests: `flick/tests/check_command.rs` (`check_exit_codes_and_json_stream`); `flick/tests/cli_run.rs` (`json_diagnostics_own_the_whole_stderr`, `json_reporter_and_json_diagnostics_keep_their_streams`, `exit_codes_follow_the_outcome_and_cleanup`); `flick/tests/cli_project.rs` (`a_stale_lockfile_is_refused_and_pnpm_speaks_ndjson`); `flick/tests/lsp_command.rs` (`receive`, `hover_definition_completion_rename_and_formatting`).

- [x] CLI eDSL: the sugar and combinators give one representation; repeated names, `Option` with a default and a type without `Cli.Value` are rejected at compile time; invalid input gives `CliError` without running the handler.
  Tests: `flick-core/src/ir/lower/cli.rs` (`the_block_and_the_combinators_lower_alike`); `tests/check/fail/cli/names.fl`; `tests/check/fail/cli/shape.fl`; `tests/check/fail/cli/types.fl`; `tests/run/stdlib/cli/cli_exit.fl`; `tests/run/stdlib/cli/cli.fl`; `tests/run/stdlib/cli/surface.fl`; `tests/check/pass/cli/blocks.fl`.

- [x] `ffi import` preserves reviewed overrides and reports coverage; `ffi check`, strict `schema export`, the `.d.ts` façade and `pack` reject unrepresentable or incompatible contracts without a silent `any`.
  Tests: `flick/tests/ffi_command.rs` (`import_review_update_and_check`); `flick-core/src/ffi/tests.rs` (`reviewed_bindings_are_checked_against_the_declarations`); `flick/tests/schema_export.rs` (`strict_export_refuses_what_it_cannot_state`); `flick/tests/dts_command.rs` (`declarations_pass_tsc_and_the_facade_separates_outcomes`); `flick/tests/cli_run.rs` (`pack_writes_a_complete_package_that_runs_nothing_on_import`).

## What must be decided before implementation

**All questions in this section are resolved for the first version.** The marks below record the chosen design; implementation readiness is checked by the separate list above.

- [x] The language name is Flick; the CLI is `flick`; sources are `.fl`; the compiler configuration is `flick.toml`.

- [x] The CLI separates `check`, `build` and running through `run`/`dev`/`test`; the `debug` and `release` profiles use one Effect backend. The shutdown contracts, JSON diagnostics, FFI/Schema tools and the implementation order are fixed in section 12.

- [x] Dependencies use the npm ecosystem, `package.json` and a single `pnpm-lock.yaml`; the first adapter is pnpm with a verifiable pinned version. There is no registry of its own and no second resolver/lockfile.

- [x] The main style is direct effectful calls with an ordinary `let`.

- [x] Call notation is the pure `a -> b` and the runtime arrow `a ->{r; e} b`.

  - r is the service requirements; e is the selected recoverable errors, not all payloads of the native Cause.

  - `a -> Eff r e b` remains a pure function that creates a description of a computation; it is a separate type, not an alternative notation for a runtime call. A lift into runtime position keeps the `Eff` result as a value.

  - The IR stores Pure or Runtime(r, e), with captures, application stages, order and source spans separately. Empty rows do not change the runtime modality.

  - `a ->{} b` remains a runtime call. `Unit` denotes the result `()`, `none` the empty row, and `Never` an uninhabited ordinary type.

- [x] A full signature above every named function, including local functions, instance methods and partial application.

- [x] Higher-rank callbacks with an explicit nested `forall` in full signatures.

- [x] Numbers: Float/BigInt and JS arithmetic; the BigInt suffix is n. Canonical Eq is SameValueZero; total Ord with NaN last, numeric relations keep JS semantics.

- [x] `task { ... }` creates an `Eff`; `await task` runs it in the current fiber; the application is started in the entry point.

  - Initialization of a source module is pure; effectful loading of a JS module requires a runtime binding.

- [x] Classes: MPTC, superclasses and FD with strict coverage/consistency and structural termination of the solver.

  - Canonical instance heads are not unified; context does not resolve overlap. No orphans, overlap, else instance or local implicits.

  - An instance belongs to the package of the class or of the external nominal constructor of the first parameter. Other semantics are nominal wrappers/explicit dictionaries.

  - Associated types and user-defined type families are not part of the first version. HKT and annotated higher-rank callbacks are included; polymorphic recursion requires an explicit full forall signature.

  - The solver has strict termination rules, caching and a diagnosable budget; there is no UndecidableInstances or selection by import order.

  - Generalization only for provably pure constructions, excluding environment variables, local skolems/regions and ambiguous constraints. Runtime bindings are monomorphic; the relaxed variance rule is deferred.

- [x] Traversal: pure List.map, sequential List.map_effect, parallel with an explicit positive limit; generic do and a single effect-polymorphic map are deferred.

- [x] A strengthened target contract for resources: a static ban on escape, verifiable lifetime of captures and of related concurrent tasks.

  - Typed errors, explicit `provide` and Effect runtime finalization are preserved.

  - A fresh scope s; T^{s,t}, T^c and T^(c + d) preserve transitive captures, including partial application and opaque wrappers. There is no early close; mutable storage is invariant. Race safety requires a resource API/synchronization.

  - Two-phase closing: stop admission, interrupt/join children, wait for foreign quiescence, then LIFO resource finalizers. Structured combinators and a mandatory scoped supervisor; a nursery is not included.

- [x] Explicit FFI bindings, a fixed data ABI and limited public TS interfaces; gradual import of supported `.d.ts`.

  - The importer generates the same FFI and explicitly reports unsupported places.

  - A binding explicitly fixes the execution mode, module initialization, ownership/retention, errors, arity/this and cancellation/quiescence. Scoped arguments with unknown retention are rejected.

  - The initial `.d.ts` subset is given in section 8; any/unknown → JsValue, anything unsupported is a blocked export with a reason. The Content Mapper does not replace the source checker.

- [x] Full deriving Schema, including generic and recursive data shapes.

  - Chosen: refined type, deriving Schema, schema T, a codec witness block; every field is given exactly once. Extra keys are rejected, projection is explicit; null/missing/defaults are separate.

  - Targets: strict JSON Schema 2020-12, Standard Schema V1, Arbitrary with overrides and a redaction-aware formatter. Strict OpenAPI 3.1 from `HttpApi`; other targets are plugins after the first version. Interpreter baseline, opt-in AOT, no implicit JIT.

  - JsValue is imported by a runtime operation into an inert Unknown with provenance; a pure codec does not traverse arbitrary getters/proxies. There is no separate DataValue.

  - Refinements are nominal and pure; external-state checks are runtime. The native Effect ABI is fixed; Ref/Redacted operations are runtime, and secrets are not exposed by deriving by default. Decimal/Duration is a checked library API without special literals.

- [x] The only backend is Effect; pure sections are optimized inside it.

- [x] Adapt suitable ReScript optimizations to our own IR; preserve currying, JS Number, the Effect ABI and execution boundaries.

- [x] Runtime: native recovery; opaque Failure/Exit/RawCause, inert reports, region-free typed payloads; structured supervision and native race/timeout. The release optimizer preserves handlers, owner, Cause, service context, cancellation and finalizers.

- [x] Data and FFI: List = dense immutable JS array; exact structural records; canonical Eq/Hash and total Float Ord; fixed ADT layout; four callback families; source iface and a limited TS façade. Modules: explicit exports, acyclic file graph, ESM; the Flick compiler is a Rust core, iface is versioned JSON.

**Outside the first version:** generic do/let\*, effect-polymorphic map, associated types and user type families, overlap/local implicits, variance generalization, linear ownership/early close, nursery and detached scoped timeout, a full TS importer, decimal/duration literals, cyclic-graph codecs, additional SQL dialects and complex SQL extensions, JSX, a custom UI renderer, SSR/hydration/RSC and the full hooks model. This is a scope boundary, not a list of unanswered questions.

- [x] Type-level minimum: FieldRow/Symbol/Constraint, finite kinds, HasField/Lacks/Project/Remove/UnionDisjoint/Map/canonical labels, MPTC/FD, opaque indices, higher-rank and captures. The SQL/UI prototypes check the accepted set.

- [x] SQL: PostgreSQL, explicit table metadata, Template/Query, pure codecs, EncodeError/DbError/DecodeError. UI: a ui block over React, exact props, typed children, pure render, explicit Exhaust/Latest/Queue/Concurrent, a mandatory supervisor and an app-owned cleanup bridge.

- [x] CLI: `cli "name" { … }` over Effect CLI, clauses `description`/`flag`/`arg`/`subcommand`/`handle`, an exact options record, `Cli.Value` with deriving, `CliError` before the handler runs, a shared `main` and cleanup.
