---
title: "13. Первый рабочий срез Flick"
---

Первый исполняемый прототип проверяет совместимость HKT, higher-rank callbacks, классов, JS-чисел, deriving Schema, прямых эффектов и исходной диагностики. Работу можно разбить на этапы реализации без исключения этих возможностей из целевого языка.

## Сквозной пример

```flick
opaque type alias UserId = String

type alias User = {
  id : UserId
  name : String
}

type alias UserView = {
  id : UserId
  title : String
}

to_view : User -> UserView
let to_view user =
  {
    id = user.id
    title = String.trim user.name
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

В примере to_view компилируется в чистую TS-функцию, load_view возвращает native Effect, load_two явно создаёт и выполняет отложенные задачи. Users/Log объявляются выбранным service-синтаксисом; implementations собираются Layers и предоставляются в явном entry point.

## Состав прототипа

1. Строгое функциональное ядро, ADT, records, `match` и модули.

2. Kinds, HKT, аннотированные higher-rank callbacks, Show/Functor и row relation для проверки выбранного MPTC/FD ядра.

3. Прямые эффектные вызовы с обычным `let` и first-class `Eff`.

4. Вывод строк ошибок и сервисов, обработчик typed failure.

5. Сервис `Users` с тестовой и внешней реализациями.

6. Генерация исполняемого кода, семантика Number/BigInt и явные семейства FFI-callbacks.

7. Scoped-ресурс, проверка escape через task/closure и одна операция конкурентного выполнения с правильным порядком завершения.

8. Deriving Schema на generic и рекурсивном типах; исходные diagnostics, hover и TS-проекция из того же core.

9. Небольшой `.d.ts` importer поверх явных bindings и проверка нескольких переносимых оптимизаций ReScript.

10. Сквозные SQL/UI примеры выбранного row/type-level ядра: PostgreSQL LEFT JOIN и React handler с task/capture checking.

## Критерии проверки реализации

- [x] Применение конструктора с неверным kind отвергается в исходном коде.
  Тесты: `tests/check/fail/types/kinds.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `tests/check/pass/types/kinds.fl`.

- [x] Отсутствующий или неоднозначный instance даёт понятную исходную диагностику.
  Тесты: `tests/check/fail/classes/resolution.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `tests/check/fail/classes/coherence.fl`.

- [x] `Functor` работает с `Option` и `Eff r e`.
  Тесты: `tests/check/pass/classes/functor.fl`; `tests/run/codegen/functor.fl`; `flick/tests/e2e.rs` (`run_cases`).

- [x] Чистые функции внутри единственного Effect backend компилируются в обычный JS без лишних Effect-обёрток.
  Тесты: `flick-core/src/codegen/tests.rs` (`pure_functions_are_plain_js`, `runtime_stages_yield_only_on_runtime_calls`); `flick-core/src/codegen/snapshots/flick_core__codegen__tests__runtime_stages_yield_only_on_runtime_calls.snap`.

- [x] `task` не выполняет тело при построении; повторный запуск выполняет его снова. `foreign effect` откладывает и вызов JS-фабрики.
  Тесты: `tests/run/codegen/task_twice.fl`; `tests/run/ffi/foreign.fl`; `tests/run/ffi/bindings.mjs`; `flick/tests/e2e.rs` (`run_cases`).

- [x] Сохранение задачи в переменную или передача через `identity` не запускают её.
  Тесты: `tests/run/codegen/task_twice.fl`; `tests/run/ffi/foreign.fl`.

- [x] Прямой вызов связывает исходный результат: `let user = Users.get id` даёт `User`. Каждая стадия каррирования сохраняет порядок; runtime-вызов с пустыми строками не становится чистым.
  Тесты: `tests/check/pass/effects/direct.fl`; `tests/check/fail/effects/purity.fl`; `tests/run/codegen/stage_order.fl`; `tests/run/codegen/eval_order.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `flick/tests/e2e.rs` (`run_cases`); `examples/first-slice/`; `flick/tests/first_slice.rs`.

- [x] Typed handlers и residual error row согласованы с native Cause; safe RawCause не раскрывает live JsValue, report не вызывает getters/toString, rethrow сохраняет original Cause и допустимую e.
  Тесты: `tests/check/pass/effects/errors.fl`; `tests/check/fail/effects/rows.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `tests/run/codegen/errors.fl`; `tests/run/stdlib/effect/exit_cause.fl`; `tests/run/stdlib/effect/effect_errors.fl`; `flick/tests/e2e.rs` (`run_cases`); `runtime/test/cause.test.ts`; `tests/check/fail/effects/rethrow.fl`.

- [x] `provide` выполняет тело после установки окружения и учитывает зависимости и ошибки построения слоя.
  Тесты: `runtime/test/layer.test.ts`; `tests/run/stdlib/effect/layer.fl`; `tests/run/codegen/services.fl`; `tests/check/fail/effects/rows.fl`; `tests/check/pass/effects/services.fl`; `examples/first-slice/`; `flick/tests/first_slice.rs`.

- [x] Typed failure, defect и interruption не смешиваются.
  Тесты: `runtime/test/cause.test.ts`; `runtime/test/concurrency.test.ts`; `tests/run/stdlib/effect/exit_cause.fl`; `tests/run/stdlib/effect/effect_errors.fl`; `tests/run/codegen/errors.fl`; `tests/run/codegen/defect.fl`; `tests/run/ffi/defect.fl`.

- [x] Scope освобождает ресурс при успехе, ошибке и отмене в рамках выбранного runtime-контракта.
  Тесты: `runtime/test/scope.test.ts`; `tests/run/runtime/scopes.fl`; `tests/run/runtime/owners.fl`; `tests/run/runtime/releases.fl`.

- [x] `parallel` соответствует выбранным правилам отмены, ошибок и cleanup.
  Тесты: `runtime/test/concurrency.test.ts`; `runtime/test/race.test.ts`; `tests/run/runtime/concurrency.fl`; `tests/run/runtime/releases.fl`.

- [x] Hover показывает исходные типы, а исполняемый и редакторный выходы согласованы. Пустые rows, `Unit` и `Never` не смешиваются в диагностике и TS ABI.
  Тесты: `flick-core/src/ide/tests.rs` (`hover_shows_source_types_with_rows_and_captures`, `hover_on_a_keyword_shows_the_type_of_its_whole_block`); `flick/tests/lsp_editor/mod.rs` (`hover_covers_private_functions_and_ignore`, `hover_distinguishes_errors_services_types_and_constructors`); `flick/tests/lsp_command.rs` (`hover_definition_completion_rename_and_formatting`, `diagnostics_match_flick_check_in_utf16`); `flick-core/tests/projection.rs` (`projection_and_executable_share_the_analysis`); `flick/tests/wasm_demo.rs` (`the_demo_compiles_a_module_as_flick_build_does`, `the_demo_reports_diagnostics_as_json_events`); `tests/check/fail/types/empty_rows.fl`; `flick-core/src/codegen/ts/tests.rs` (`unit_never_and_empty_rows_have_distinct_typescript_types`, `exports_have_typescript_types_without_any`); `tests/check/pass/effects/errors.fl`.

- [x] Higher-rank callback применяется к разным типам; ошибочная мономорфная реализация отвергается; class dictionaries и внешние rows не теряются.
  Тесты: `tests/check/pass/types/higher_rank.fl`; `tests/check/fail/types/higher_rank.fl`; `tests/run/codegen/classes.fl`; `tests/run/codegen/higher_rank.fl`; `flick/tests/e2e.rs` (`run_cases`).

- [x] Number сохраняет NaN, infinity, signed zero и JS-порядок округления в эталонном и оптимизированном codegen.
  Тесты: `flick/tests/e2e_numbers.rs` (`numbers_match_js`); `tests/run/codegen/numbers.fl`; `flick/tests/e2e.rs` (`run_cases`); `flick-core/tests/consteval_node.rs` (`folding_matches_node`); `flick-core/src/consteval/tests.rs` (`folds_number_arithmetic_with_js_semantics`); `flick-core/src/opt/tests.rs` (`literal_expressions_fold_with_js_semantics`, `forbidden_transformations_stay_out`).

- [x] Deriving Schema работает для generic, higher-kinded, recursive и mutually recursive данных; неподдерживаемая автоматическая сериализация получает точную диагностику.
  Тесты: `tests/check/pass/schema/derive.fl`; `tests/run/schema/derive.fl`; `tests/check/fail/schema/codecs.fl`; `runtime/test/schema.test.ts`.

- [x] JS-callback adapter соблюдает результат, arity/this, reentrancy, окружение, отмену и lifetime выбранного семейства.
  Тесты: `runtime/test/callbacks.test.ts`; `tests/run/ffi/events.fl`; `tests/run/ffi/foreign.fl`; `flick/tests/e2e.rs` (`run_cases`); `tests/check/fail/ffi/metadata.fl`.

- [x] Scoped-handle не выходит через результат, callback, task, контейнер, error payload или запись во внешнее состояние; interface не стирает captures.
  Тесты: `tests/check/fail/regions/escape.fl`; `tests/check/fail/regions/qualifiers.fl`; `flick-core/tests/check.rs` (`check_fixtures`, `project_fixtures`); `tests/check/projects/regions/`; `flick-core/src/iface.rs` (`tests::schema_is_pinned_and_round_trips`).

- [x] Owner завершает всех детей до release ресурсов; async FFI ждёт quiescence после отмены, даже когда underlying Promise не остановился вместе с fiber. Uninterruptible child задерживает закрытие.
  Тесты: `runtime/test/scope.test.ts`; `tests/run/runtime/scopes.fl`; `tests/run/runtime/owners.fl`; `flick/tests/e2e.rs` (`run_cases`).

- [x] `.d.ts` importer показывает unsupported declarations и требуемые политики без неявного `any`, pure или cancellation.
  Тесты: `flick-core/src/ffi/tests.rs` (`any_pure_and_cancellation_are_never_implied`, `without_policies_promises_callbacks_and_nullable_results_are_blocked`, `strict_mode_refuses_unknown`, `policies_are_checked`); `flick/tests/ffi_command.rs` (`import_review_update_and_check`); `flick/tests/snapshots/ffi_command__lookup_report.snap`; `tools/dts-importer/test/import.test.mjs`.

- [x] Локальные оптимизации сохраняют currying stages, JS Number и native Effect ABI; эталонный и оптимизированный codegen согласованы на наблюдаемом поведении.
  Тесты: `flick-core/src/opt/tests.rs` (`a_full_application_calls_the_worker_and_a_partial_one_keeps_the_closure`, `forbidden_transformations_stay_out`, `unused_pure_code_is_dropped_and_effects_stay`); `tests/run/codegen/release.fl`; `tests/run/codegen/stage_order.fl`; `flick-core/src/codegen/tests.rs` (`runtime_stages_yield_only_on_runtime_calls`); `tests/run/codegen/effect_abi.fl`; `flick/tests/e2e.rs` (`run_cases`); `tests/run`.

- [x] Strict schema export отвергает непредставимый wire-контракт; это отделено от допустимого AOT interpreter fallback.
  Тесты: `flick/tests/schema_export.rs` (`strict_export_refuses_what_it_cannot_state`); `runtime/test/schema.test.ts`; `tests/run/schema/contracts.fl`; `flick/tests/build_command.rs` (`schema_aot_writes_compiled_decoders_the_runner_installs`).

- [x] OpenAPI 3.1 export `HttpApi` валиден по спецификации OpenAPI 3.1, не содержит `{}`/`any` и отвергает непредставимую schema endpoint'а с путём.
  Тесты: `runtime/test/httpapi.test.ts`; `runtime/test/fixtures/openapi-3.1.schema.json`; `tests/run/stdlib/http/api.fl`; `flick/tests/schema_export.rs` (`strict_export_refuses_what_it_cannot_state`); `flick/tests/cli_run.rs` (`the_spec_examples_and_the_ci_set_pass`).

- [x] Schema независимо проверяет missing/null/undefined/defaults/extra keys; effectful codec не получает PureCodec сертификат.
  Тесты: `runtime/test/schema.test.ts`; `tests/run/schema/policies.fl`; `tests/check/fail/schema/codecs.fl`.

- [x] SQL отвергает неверные поля, GROUP BY, alias scope, параметры и generated INSERT columns; LEFT JOIN меняет nullability, pure build не выполняет I/O.
  Тесты: `tests/check/pass/regions/captures.fl`; `tests/check/fail/regions/qualifiers.fl`; `tests/check/fail/regions/escape.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `tests/check/projects/regions/`; `flick-core/src/iface.rs` (`tests::schema_is_pinned_and_round_trips`).

- [x] UI отвергает runtime-call в render и escaping handler; event policy ограничивает работу, supervisor наблюдает failures, React cleanup сохраняет ресурсы до join.
  Тесты: `tests/check/fail/sql/queries.fl`; `tests/check/pass/sql/balances.fl`; `tests/run/sql/queries.fl`; `runtime/test/sql.test.ts`; `flick/tests/sql_postgres.rs` (`queries_against_postgres`); `tests/postgres/app.fl`; `flick-core/src/ide/tests.rs` (`captures_in_sql_are_names_of_the_program`); `tests/check/fail/sql/balances.fl`.

- [x] Curried generic combinators сохраняют captures исходного container, callback и partial application; wrappers и iface не стирают lifetime.
  Тесты: `tests/check/fail/ui/props.fl`; `runtime/test/ui.test.ts`; `tests/run/ui/counter.fl`; `tests/check/pass/ui/save_button.fl`; `tests/run/ui/directory.fl`; `tests/check/fail/ui/children.fl`.

- [x] CLI использует один Rust checker: `check` и `build` не запускают приложение, не выполняют эффектную инициализацию foreign modules и не устанавливают зависимости; `run`, `dev` и `test` собирают исполняемый JS.
  Тесты: `flick/tests/cli_project.rs` (`check_and_build_run_no_code_of_the_program`); `flick/tests/lsp_command.rs` (`diagnostics_match_flick_check_in_utf16`); `flick/tests/wasm_demo.rs` (`the_demo_compiles_a_module_as_flick_build_does`); `flick/tests/cli_run.rs` (`run_builds_javascript_even_for_a_typescript_project`, `dev_replaces_the_app_only_after_its_shutdown`); `flick/tests/test_command.rs` (`outcomes_are_reported_apart`).

- [x] `dev` изолирует артефакты и разрешённые зависимости активной генерации; следующая версия запускается только после join, foreign quiescence и finalizers предыдущей. Cleanup failure останавливает перезапуск и сохраняется в итоговом Cause.
  Тесты: `flick/tests/cli_run.rs` (`a_running_generation_keeps_its_files`, `dev_replaces_the_app_only_after_its_shutdown`, `dev_starts_the_next_version_after_foreign_work_settles`); `runtime/test/scope.test.ts`.

- [x] Кеш разделяет эффективную конфигурацию, профили, emit modes, toolchain/runtime и dependency fingerprints; `clean` сохраняет активные генерации и пользовательские файлы.
  Тесты: `flick/tests/cli_project.rs` (`the_cache_key_follows_every_input_and_a_miss_changes_nothing`, `the_installed_runtime_and_effect_are_in_the_cache_key`, `clean_keeps_user_files_and_running_generations`); `flick/tests/cli_run.rs` (`a_running_generation_keeps_its_files`).

- [x] `install --locked` отвергает отсутствующий или устаревший lockfile; фактическая версия pnpm проверяется до изменения зависимостей, а несовместимые compiler/runtime/Effect блокируют компиляцию и запуск.
  Тесты: `flick/tests/cli_project.rs` (`a_wrong_pnpm_or_a_missing_lockfile_changes_nothing`, `a_stale_lockfile_is_refused_and_pnpm_speaks_ndjson`, `an_incompatible_runtime_blocks_the_build`, `an_incompatible_effect_blocks_build_and_run`); `flick-core/src/project/pnpm.rs` (`locked_and_offline_map_to_pnpm_flags`).

- [x] JSON diagnostics остаются валидным versioned NDJSON при выводе приложения, тестов и package manager; stdout LSP и JSON test reporter сохраняют свой протокол, а коды завершения учитывают результат cleanup.
  Тесты: `flick/tests/check_command.rs` (`check_exit_codes_and_json_stream`); `flick/tests/cli_run.rs` (`json_diagnostics_own_the_whole_stderr`, `json_reporter_and_json_diagnostics_keep_their_streams`, `exit_codes_follow_the_outcome_and_cleanup`); `flick/tests/cli_project.rs` (`a_stale_lockfile_is_refused_and_pnpm_speaks_ndjson`); `flick/tests/lsp_command.rs` (`receive`, `hover_definition_completion_rename_and_formatting`).

- [x] CLI eDSL: сахар и combinators дают одно представление; повтор имён, `Option` с default и тип без `Cli.Value` отвергаются при компиляции; неверный ввод даёт `CliError` без запуска handler.
  Тесты: `flick-core/src/ir/lower/cli.rs` (`the_block_and_the_combinators_lower_alike`); `tests/check/fail/cli/names.fl`; `tests/check/fail/cli/shape.fl`; `tests/check/fail/cli/types.fl`; `tests/run/stdlib/cli/cli_exit.fl`; `tests/run/stdlib/cli/cli.fl`; `tests/run/stdlib/cli/surface.fl`; `tests/check/pass/cli/blocks.fl`.

- [x] `ffi import` сохраняет reviewed overrides и сообщает coverage; `ffi check`, strict `schema export`, `.d.ts` façade и `pack` отклоняют непредставимые или несовместимые контракты без тихого `any`.
  Тесты: `flick/tests/ffi_command.rs` (`import_review_update_and_check`); `flick-core/src/ffi/tests.rs` (`reviewed_bindings_are_checked_against_the_declarations`); `flick/tests/schema_export.rs` (`strict_export_refuses_what_it_cannot_state`); `flick/tests/dts_command.rs` (`declarations_pass_tsc_and_the_facade_separates_outcomes`); `flick/tests/cli_run.rs` (`pack_writes_a_complete_package_that_runs_nothing_on_import`).

## Что требуется решить до реализации

**Все вопросы этого раздела решены для первой версии.** Отметки ниже фиксируют выбранный дизайн; готовность реализации проверяется отдельным списком выше.

- [x] Имя языка — Flick; CLI — `flick`; исходники — `.fl`; конфигурация compiler — `flick.toml`.

- [x] CLI разделяет `check`, `build` и запуск через `run`/`dev`/`test`; профили `debug` и `release` используют один Effect backend. Контракты shutdown, JSON diagnostics, FFI/Schema tools и порядок реализации зафиксированы в разделе 12.

- [x] Зависимости используют npm-экосистему, `package.json` и один `pnpm-lock.yaml`; первый adapter — pnpm с проверяемой закреплённой версией. Собственного registry и второго resolver/lockfile нет.

- [x] Основной стиль — прямые эффектные вызовы с обычным `let`.

- [x] Нотация вызова — чистая `a -> b` и runtime-стрелка `a ->{r; e} b`.

  - r — requirements сервисов; e — selected recoverable errors, не все payloads native Cause.

  - `a -> Eff r e b` остаётся чистой функцией, создающей описание вычисления; это отдельный тип, а не альтернативная запись runtime-вызова. Lift в runtime-позицию сохраняет результат `Eff` как значение.

  - IR хранит Pure либо Runtime(r, e), отдельно captures, стадии применения, order и source spans. Пустые rows не изменяют runtime-модальность.

  - `a ->{} b` остаётся runtime-вызовом. `Unit` обозначает результат `()`, `none` — пустую строку, `Never` — необитаемый обычный тип.

- [x] Полная сигнатура над каждой именованной функцией, включая локальные функции, методы instances и частичное применение.

- [x] Higher-rank callbacks с явным вложенным `forall` в полных сигнатурах.

- [x] Числа: Number/BigInt и JS arithmetic; BigInt suffix n. Canonical Eq — SameValueZero; total Ord с NaN последним, numeric relations сохраняют JS-семантику.

- [x] `task { ... }` создаёт `Eff`; `await task` выполняет его в текущем fiber; приложение запускается в entry point.

  - Инициализация исходного модуля чистая; эффектная загрузка JS-модуля требует runtime-binding.

- [x] Классы: MPTC, superclasses и FD с strict coverage/consistency и структурным завершением solver.

  - Canonical instance heads не унифицируются; context не разрешает overlap. No orphans, overlap, else instance и local implicits.

  - Instance принадлежит пакету класса либо внешнего nominal constructor первого параметра. Другие semantics — nominal wrappers/explicit dictionaries.

  - Associated types и пользовательские type families не входят в первую версию. HKT и annotated higher-rank callbacks входят; polymorphic recursion требует явной полной forall-сигнатуры.

  - Solver имеет строгие правила завершения, кеширование и диагностируемый бюджет; нет UndecidableInstances или выбора по import order.

  - Generalization только для доказанно pure построений, с исключением переменных окружения, local skolems/regions и ambiguous constraints. Runtime bindings мономорфны; relaxed variance rule отложено.

- [x] Обход: pure List.map, последовательный List.map_effect, parallel с явным positive limit; generic do и единый effect-polymorphic map отложены.

- [x] Усиленный целевой контракт ресурсов: статический запрет escape, проверяемое время жизни захватов и связанных конкурентных задач.

  - Сохраняются typed errors, явный `provide` и runtime-финализация Effect.

  - Fresh scope s; T^{s,t}, T^c и T^(c + d) сохраняют transitive captures, включая partial application и opaque wrappers. Early close отсутствует; mutable storage инвариантно. Race safety требует resource API/синхронизации.

  - Двухфазное закрытие: остановить admission, interrupt/join children, дождаться foreign quiescence, затем LIFO resource finalizers. Structured combinators и обязательный scoped supervisor; nursery не входит.

- [x] Явные FFI-bindings, фиксированный ABI данных и ограниченные публичные TS-интерфейсы; постепенный импорт поддерживаемых `.d.ts`.

  - Importer генерирует тот же FFI и явно сообщает о неподдержанных местах.

  - Binding явно фиксирует execution mode, module initialization, ownership/retention, errors, arity/this и cancellation/quiescence. Scoped arguments с неизвестным retention отвергаются.

  - Начальный `.d.ts` subset задан в разделе 8; any/unknown → JsValue, неподдержанное — blocked export с причиной. Content Mapper не заменяет source checker.

- [x] Полноценный deriving Schema, включая generic и рекурсивные формы данных.

  - Выбраны refined type, deriving Schema, schema T, codec witness-блок; каждое field задано ровно один раз. Extra keys reject, projection explicit; null/missing/defaults раздельны.

  - Targets: strict JSON Schema 2020-12, Standard Schema V1, Arbitrary с overrides и redaction-aware formatter. Strict OpenAPI 3.1 из `HttpApi`; другие targets — плагины после первой версии. Interpreter baseline, opt-in AOT, без implicit JIT.

  - JsValue импортируется runtime-операцией в inert Unknown с provenance; pure codec не обходит arbitrary getters/proxies. Отдельного DataValue нет.

  - Refinements nominal и pure; external-state checks — runtime. Native Effect ABI фиксирован; Ref/Redacted операции runtime, секреты не раскрываются deriving по умолчанию. Decimal/Duration — checked library API без специальных literals.

- [x] Единственный backend — Effect; чистые участки оптимизируются внутри него.

- [x] Адаптировать подходящие оптимизации ReScript к собственному IR; сохранить currying, JS Number, Effect ABI и границы выполнения.

- [x] Runtime: native recovery; opaque Failure/Exit/RawCause, inert reports, region-free typed payloads; structured supervision и native race/timeout. Release optimizer сохраняет handlers, owner, Cause, service context, cancellation и finalizers.

- [x] Данные и FFI: List = dense immutable JS array; exact structural records; canonical Eq/Hash и total Number Ord; fixed ADT layout; четыре callback families; source iface и ограниченный TS façade. Модули — explicit exports, acyclic file graph, ESM; compiler Flick — Rust core, iface — versioned JSON.

**За пределами первой версии:** generic do/let\*, effect-polymorphic map, associated types и user type families, overlap/local implicits, variance-generalization, linear ownership/early close, nursery и detached scoped timeout, полный TS importer, decimal/duration literals, cyclic-graph codecs, дополнительные SQL dialects и сложные SQL extensions, JSX, собственный UI renderer, SSR/hydration/RSC и полная hooks-модель. Это граница объёма, а не список вопросов без ответа.

- [x] Type-level минимум: FieldRow/Symbol/Constraint, finite kinds, HasField/Lacks/Project/Remove/UnionDisjoint/Map/canonical labels, MPTC/FD, opaque indices, higher-rank и captures. SQL/UI prototypes проверяют принятый набор.

- [x] SQL: PostgreSQL, explicit table metadata, Template/Query, pure codecs, EncodeError/DbError/DecodeError. UI: ui-блок поверх React, exact props, typed children, pure render, explicit Exhaust/Latest/Queue/Concurrent, обязательный supervisor и app-owned cleanup bridge.

- [x] CLI: `cli "name" { … }` поверх Effect CLI, clauses `description`/`flag`/`arg`/`subcommand`/`handle`, точный record опций, `Cli.Value` с deriving, `CliError` до запуска handler, общий `main` и cleanup.
