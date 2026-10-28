---
title: "13. Первый рабочий прототип Flick"
---

Первый исполняемый прототип проверяет, что HKT, функции обратного вызова с полиморфизмом высших рангов, классы, числовая семантика JS, автоматический вывод Schema, прямые эффектные вызовы и диагностика по исходному коду работают совместно. Эти возможности можно реализовывать поэтапно, не исключая их из целевого языка.

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

В примере to_view компилируется в чистую TS-функцию, а load_view — в функцию, возвращающую значение самого Effect. load_two явно создаёт и выполняет отложенные задачи. Сервисы Users/Log объявляются принятым синтаксисом сервисов; их реализации собираются в слои и предоставляются в явной точке входа.

## Состав прототипа

1. Функциональное ядро со строгими вычислениями, ADT, записи, `match` и модули.

2. Кинды, HKT, аннотированные функции обратного вызова с полиморфизмом высших рангов, Show/Functor и отношения между строками для проверки выбранного ядра MPTC/FD.

3. Прямые эффектные вызовы с обычным `let` и `Eff` как значение первого класса.

4. Вывод строк ошибок и сервисов, обработчик типизированной ошибки.

5. Сервис `Users` с тестовой и внешней реализациями.

6. Генерация исполняемого кода, семантика Float/BigInt и явные семейства функций обратного вызова FFI.

7. Ресурс с областью владения, проверка выхода за её пределы через задачу или замыкание и одна операция конкурентного выполнения с правильным порядком завершения.

8. Автоматический вывод Schema для обобщённых и рекурсивных типов; диагностика по исходному коду, подсказки при наведении и TS-проекция из того же ядра.

9. Небольшое средство импорта `.d.ts` на основе явных привязок и проверка нескольких переносимых оптимизаций ReScript.

10. Сквозные примеры SQL/UI для выбранного ядра строк и операций над типами: PostgreSQL LEFT JOIN и обработчик React с проверкой задач и захватов.

## Критерии проверки реализации

- [x] Применение конструктора с неверным киндом отвергается в исходном коде.
  Тесты: `tests/check/fail/types/kinds.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `tests/check/pass/types/kinds.fl`.

- [x] Отсутствующий или неоднозначный экземпляр класса даёт понятную диагностику по исходному коду.
  Тесты: `tests/check/fail/classes/resolution.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `tests/check/fail/classes/coherence.fl`.

- [x] `Functor` работает с `Option` и `Eff r e`.
  Тесты: `tests/check/pass/classes/functor.fl`; `tests/run/codegen/functor.fl`; `flick/tests/e2e.rs` (`run_cases`).

- [x] Чистые функции в единственном генераторе кода для Effect компилируются в обычный JS без лишних обёрток Effect.
  Тесты: `flick-core/src/codegen/tests.rs` (`pure_functions_are_plain_js`, `runtime_stages_yield_only_on_runtime_calls`); `flick-core/src/codegen/snapshots/flick_core__codegen__tests__runtime_stages_yield_only_on_runtime_calls.snap`.

- [x] `task` не выполняет тело при построении; повторный запуск выполняет его снова. `foreign effect` откладывает и вызов JS-фабрики.
  Тесты: `tests/run/codegen/task_twice.fl`; `tests/run/ffi/foreign.fl`; `tests/run/ffi/bindings.mjs`; `flick/tests/e2e.rs` (`run_cases`).

- [x] Сохранение задачи в переменную или передача через `identity` не запускают её.
  Тесты: `tests/run/codegen/task_twice.fl`; `tests/run/ffi/foreign.fl`.

- [x] Прямой вызов связывает исходный результат: `let user = Users.get id` даёт `User`. Каждая стадия каррирования сохраняет порядок; эффектный вызов с пустыми строками не становится чистым.
  Тесты: `tests/check/pass/effects/direct.fl`; `tests/check/fail/effects/purity.fl`; `tests/run/codegen/stage_order.fl`; `tests/run/codegen/eval_order.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `flick/tests/e2e.rs` (`run_cases`); `examples/first-slice/`; `flick/tests/first_slice.rs`.

- [x] Типизированные обработчики и остаточная строка ошибок согласованы с Cause самого Effect; безопасный RawCause не даёт доступа к исходному JsValue, report не вызывает геттеры и toString, rethrow сохраняет исходный Cause и допустимую строку ошибок e.
  Тесты: `tests/check/pass/effects/errors.fl`; `tests/check/fail/effects/rows.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `tests/run/codegen/errors.fl`; `tests/run/stdlib/effect/exit_cause.fl`; `tests/run/stdlib/effect/effect_errors.fl`; `flick/tests/e2e.rs` (`run_cases`); `runtime/test/cause.test.ts`; `tests/check/fail/effects/rethrow.fl`.

- [x] `provide` выполняет тело после установки окружения и учитывает зависимости и ошибки построения слоя.
  Тесты: `runtime/test/layer.test.ts`; `tests/run/stdlib/effect/layer.fl`; `tests/run/codegen/services.fl`; `tests/check/fail/effects/rows.fl`; `tests/check/pass/effects/services.fl`; `examples/first-slice/`; `flick/tests/first_slice.rs`.

- [x] Типизированная ошибка, дефект и прерывание не смешиваются.
  Тесты: `runtime/test/cause.test.ts`; `runtime/test/concurrency.test.ts`; `tests/run/stdlib/effect/exit_cause.fl`; `tests/run/stdlib/effect/effect_errors.fl`; `tests/run/codegen/errors.fl`; `tests/run/codegen/defect.fl`; `tests/run/ffi/defect.fl`.

- [x] Область владения освобождает ресурс при успехе, ошибке и отмене в рамках выбранного контракта среды выполнения.
  Тесты: `runtime/test/scope.test.ts`; `tests/run/runtime/scopes.fl`; `tests/run/runtime/owners.fl`; `tests/run/runtime/releases.fl`.

- [x] `parallel` соответствует выбранным правилам отмены, ошибок и освобождения ресурсов.
  Тесты: `runtime/test/concurrency.test.ts`; `runtime/test/race.test.ts`; `tests/run/runtime/concurrency.fl`; `tests/run/runtime/releases.fl`.

- [x] Подсказки при наведении показывают исходные типы, а исполняемый код и редакторная проекция согласованы. Пустые строки, `Unit` и `Never` не смешиваются в диагностике и TS ABI.
  Тесты: `flick-core/src/ide/tests.rs` (`hover_shows_source_types_with_rows_and_captures`, `hover_on_a_keyword_shows_the_type_of_its_whole_block`); `flick/tests/lsp_editor/mod.rs` (`hover_covers_private_functions_and_ignore`, `hover_distinguishes_errors_services_types_and_constructors`); `flick/tests/lsp_command.rs` (`hover_definition_completion_rename_and_formatting`, `diagnostics_match_flick_check_in_utf16`); `flick-core/tests/projection.rs` (`projection_and_executable_share_the_analysis`); `flick/tests/wasm_demo.rs` (`the_demo_compiles_a_module_as_flick_build_does`, `the_demo_reports_diagnostics_as_json_events`); `tests/check/fail/types/empty_rows.fl`; `flick-core/src/codegen/ts/tests.rs` (`unit_never_and_empty_rows_have_distinct_typescript_types`, `exports_have_typescript_types_without_any`); `tests/check/pass/effects/errors.fl`.

- [x] Функция обратного вызова с полиморфизмом высших рангов применяется к разным типам; ошибочная мономорфная реализация отвергается; словари классов и внешние строки не теряются.
  Тесты: `tests/check/pass/types/higher_rank.fl`; `tests/check/fail/types/higher_rank.fl`; `tests/run/codegen/classes.fl`; `tests/run/codegen/higher_rank.fl`; `flick/tests/e2e.rs` (`run_cases`).

- [x] Float сохраняет NaN, бесконечности, знак нуля и порядок округления JS в эталонном и оптимизированном коде.
  Тесты: `flick/tests/e2e_numbers.rs` (`numbers_match_js`); `tests/run/codegen/numbers.fl`; `flick/tests/e2e.rs` (`run_cases`); `flick-core/tests/consteval_node.rs` (`folding_matches_node`); `flick-core/src/consteval/tests.rs` (`folds_number_arithmetic_with_js_semantics`); `flick-core/src/opt/tests.rs` (`literal_expressions_fold_with_js_semantics`, `forbidden_transformations_stay_out`).

- [x] Автоматический вывод Schema работает для обобщённых, рекурсивных и взаимно рекурсивных типов, а также типов высших порядков; неподдерживаемая автоматическая сериализация получает точную диагностику.
  Тесты: `tests/check/pass/schema/derive.fl`; `tests/run/schema/derive.fl`; `tests/check/fail/schema/codecs.fl`; `runtime/test/schema.test.ts`.

- [x] Адаптер функции обратного вызова JS соблюдает требования выбранного семейства к результату, арности и this, повторному входу, окружению, отмене и времени жизни.
  Тесты: `runtime/test/callbacks.test.ts`; `tests/run/ffi/events.fl`; `tests/run/ffi/foreign.fl`; `flick/tests/e2e.rs` (`run_cases`); `tests/check/fail/ffi/metadata.fl`.

- [x] Дескриптор с областью владения не выходит за её пределы через результат, функцию обратного вызова, задачу, контейнер, данные ошибки или запись во внешнее состояние; интерфейс не стирает захваты.
  Тесты: `tests/check/fail/regions/escape.fl`; `tests/check/fail/regions/qualifiers.fl`; `flick-core/tests/check.rs` (`check_fixtures`, `project_fixtures`); `tests/check/projects/regions/`; `flick-core/src/iface.rs` (`tests::schema_is_pinned_and_round_trips`).

- [x] Владелец завершает все дочерние задачи до освобождения ресурсов. Асинхронный FFI после отмены ждёт прекращения доступа внешней операции к ресурсам (quiescence), даже если лежащий в её основе Promise не остановился вместе с файбером. Непрерываемая дочерняя задача задерживает закрытие.
  Тесты: `runtime/test/scope.test.ts`; `tests/run/runtime/scopes.fl`; `tests/run/runtime/owners.fl`; `flick/tests/e2e.rs` (`run_cases`).

- [x] Средство импорта `.d.ts` сообщает о неподдерживаемых объявлениях и требуемых политиках без неявного `any`, предположений о чистоте или отменяемости.
  Тесты: `flick-core/src/ffi/tests.rs` (`any_pure_and_cancellation_are_never_implied`, `without_policies_promises_callbacks_and_nullable_results_are_blocked`, `strict_mode_refuses_unknown`, `policies_are_checked`); `flick/tests/ffi_command.rs` (`import_review_update_and_check`); `flick/tests/snapshots/ffi_command__lookup_report.snap`; `tools/dts-importer/test/import.test.mjs`.

- [x] Локальные оптимизации сохраняют стадии каррирования, JS Number и ABI самого Effect; эталонный и оптимизированный код согласованы по наблюдаемому поведению.
  Тесты: `flick-core/src/opt/tests.rs` (`a_full_application_calls_the_worker_and_a_partial_one_keeps_the_closure`, `forbidden_transformations_stay_out`, `unused_pure_code_is_dropped_and_effects_stay`); `tests/run/codegen/release.fl`; `tests/run/codegen/stage_order.fl`; `flick-core/src/codegen/tests.rs` (`runtime_stages_yield_only_on_runtime_calls`); `tests/run/codegen/effect_abi.fl`; `flick/tests/e2e.rs` (`run_cases`); `tests/run`.

- [x] Строгий экспорт схем отвергает непредставимый контракт внешнего представления; при AOT-оптимизации отдельно допускается возврат к интерпретатору.
  Тесты: `flick/tests/schema_export.rs` (`strict_export_refuses_what_it_cannot_state`); `runtime/test/schema.test.ts`; `tests/run/schema/contracts.fl`; `flick/tests/build_command.rs` (`schema_aot_writes_compiled_decoders_the_runner_installs`).

- [x] Экспорт OpenAPI 3.1 из `HttpApi` соответствует спецификации OpenAPI 3.1, не содержит `{}`/`any` и отвергает непредставимую схему эндпоинта с указанием пути.
  Тесты: `runtime/test/httpapi.test.ts`; `runtime/test/fixtures/openapi-3.1.schema.json`; `tests/run/stdlib/http/api.fl`; `flick/tests/schema_export.rs` (`strict_export_refuses_what_it_cannot_state`); `flick/tests/cli_run.rs` (`the_spec_examples_and_the_ci_set_pass`).

- [x] Schema раздельно проверяет отсутствие поля, null, undefined, значения по умолчанию и лишние ключи; эффектный кодек не получает сертификат PureCodec.
  Тесты: `runtime/test/schema.test.ts`; `tests/run/schema/policies.fl`; `tests/check/fail/schema/codecs.fl`.

- [x] SQL отвергает неверные поля, GROUP BY, псевдонимы вне области видимости, неверные параметры и попытки задать генерируемые столбцы в INSERT; LEFT JOIN меняет допустимость NULL, чистое построение запроса не выполняет I/O.
  Тесты: `tests/check/pass/regions/captures.fl`; `tests/check/fail/regions/qualifiers.fl`; `tests/check/fail/regions/escape.fl`; `flick-core/tests/check.rs` (`check_fixtures`); `tests/check/projects/regions/`; `flick-core/src/iface.rs` (`tests::schema_is_pinned_and_round_trips`).

- [x] UI отвергает эффектные вызовы при отрисовке и обработчики, выходящие за область владения; политика событий ограничивает работу, супервизор наблюдает ошибки, очистка React сохраняет ресурсы до завершения задач.
  Тесты: `tests/check/fail/sql/queries.fl`; `tests/check/pass/sql/balances.fl`; `tests/run/sql/queries.fl`; `runtime/test/sql.test.ts`; `flick/tests/sql_postgres.rs` (`queries_against_postgres`); `tests/postgres/app.fl`; `flick-core/src/ide/tests.rs` (`captures_in_sql_are_names_of_the_program`); `tests/check/fail/sql/balances.fl`.

- [x] Каррированные обобщённые комбинаторы сохраняют захваты исходного контейнера, переданной функции и частичного применения; обёртки и iface не стирают время жизни.
  Тесты: `tests/check/fail/ui/props.fl`; `runtime/test/ui-roots.test.ts`; `tests/run/ui/counter.fl`; `tests/check/pass/ui/save_button.fl`; `tests/run/ui/directory.fl`; `tests/check/fail/ui/children.fl`.

- [x] CLI использует один проверяющий модуль на Rust: `check` и `build` не запускают приложение, не выполняют эффектную инициализацию внешних модулей и не устанавливают зависимости; `run`, `dev` и `test` собирают исполняемый JS.
  Тесты: `flick/tests/cli_project.rs` (`check_and_build_run_no_code_of_the_program`); `flick/tests/lsp_command.rs` (`diagnostics_match_flick_check_in_utf16`); `flick/tests/wasm_demo.rs` (`the_demo_compiles_a_module_as_flick_build_does`); `flick/tests/cli_run.rs` (`run_builds_javascript_even_for_a_typescript_project`, `dev_replaces_the_app_only_after_its_shutdown`); `flick/tests/test_command.rs` (`outcomes_are_reported_apart`).

- [x] `dev` изолирует артефакты и разрешённые зависимости активной версии; следующая версия запускается только после завершения дочерних задач, прекращения доступа внешних операций к ресурсам и выполнения финализаторов предыдущей. Ошибка освобождения ресурсов останавливает перезапуск и сохраняется в итоговом Cause.
  Тесты: `flick/tests/cli_run.rs` (`a_running_generation_keeps_its_files`, `dev_replaces_the_app_only_after_its_shutdown`, `dev_starts_the_next_version_after_foreign_work_settles`); `runtime/test/scope.test.ts`.

- [x] Ключ кеша учитывает итоговую конфигурацию, профили, режимы генерации кода, версии инструментов и среды выполнения и отпечатки зависимостей; `clean` сохраняет активные версии и пользовательские файлы.
  Тесты: `flick/tests/cli_project.rs` (`the_cache_key_follows_every_input_and_a_miss_changes_nothing`, `the_installed_runtime_and_effect_are_in_the_cache_key`, `clean_keeps_user_files_and_running_generations`); `flick/tests/cli_run.rs` (`a_running_generation_keeps_its_files`).

- [x] `install --locked` отвергает отсутствующий или устаревший файл фиксации зависимостей; фактическая версия pnpm проверяется до изменения зависимостей, а несовместимые версии компилятора, среды выполнения и Effect блокируют компиляцию и запуск.
  Тесты: `flick/tests/cli_project.rs` (`a_wrong_pnpm_or_a_missing_lockfile_changes_nothing`, `a_stale_lockfile_is_refused_and_pnpm_speaks_ndjson`, `an_incompatible_runtime_blocks_the_build`, `an_incompatible_effect_blocks_build_and_run`); `flick-core/src/project/pnpm.rs` (`locked_and_offline_map_to_pnpm_flags`).

- [x] Диагностика в JSON остаётся корректным NDJSON с версией схемы при выводе приложения, тестов и менеджера пакетов; stdout LSP и JSON-репортёр тестов сохраняют свой протокол, а коды завершения учитывают результат освобождения ресурсов.
  Тесты: `flick/tests/check_command.rs` (`check_exit_codes_and_json_stream`); `flick/tests/cli_run.rs` (`json_diagnostics_own_the_whole_stderr`, `json_reporter_and_json_diagnostics_keep_their_streams`, `exit_codes_follow_the_outcome_and_cleanup`); `flick/tests/cli_project.rs` (`a_stale_lockfile_is_refused_and_pnpm_speaks_ndjson`); `flick/tests/lsp_command.rs` (`receive`, `hover_definition_completion_rename_and_formatting`).

- [x] CLI eDSL: синтаксический сахар и комбинаторы дают одно представление; повтор имён, `Option` со значением по умолчанию и тип без `Cli.Value` отвергаются при компиляции; неверный ввод даёт `CliError` без запуска обработчика.
  Тесты: `flick-core/src/ir/lower/cli.rs` (`the_block_and_the_combinators_lower_alike`); `tests/check/fail/cli/names.fl`; `tests/check/fail/cli/shape.fl`; `tests/check/fail/cli/types.fl`; `tests/run/stdlib/cli/cli_exit.fl`; `tests/run/stdlib/cli/cli.fl`; `tests/run/stdlib/cli/surface.fl`; `tests/check/pass/cli/blocks.fl`.

- [x] `ffi import` сохраняет проверенные вручную переопределения и сообщает о покрытии; `ffi check`, строгий `schema export`, фасад `.d.ts` и `pack` отклоняют непредставимые или несовместимые контракты без неявной подстановки `any`.
  Тесты: `flick/tests/ffi_command.rs` (`import_review_update_and_check`); `flick-core/src/ffi/tests.rs` (`reviewed_bindings_are_checked_against_the_declarations`); `flick/tests/schema_export.rs` (`strict_export_refuses_what_it_cannot_state`); `flick/tests/dts_command.rs` (`declarations_pass_tsc_and_the_facade_separates_outcomes`); `flick/tests/cli_run.rs` (`pack_writes_a_complete_package_that_runs_nothing_on_import`).

<div id="что-требуется-решить-до-реализации"></div>

## Принятые решения для первой версии

**Все вопросы этого раздела решены для первой версии.** Отметки ниже фиксируют выбранный дизайн; готовность реализации проверяется отдельным списком выше.

- [x] Имя языка — Flick; CLI — `flick`; исходники — `.fl`; конфигурация компилятора — `flick.toml`.

- [x] CLI разделяет `check`, `build` и запуск через `run`/`dev`/`test`; профили `debug` и `release` используют один генератор кода для Effect. Контракты завершения работы, диагностики в JSON, инструментов FFI и Schema и порядок реализации зафиксированы в разделе 12.

- [x] Зависимости используют npm-экосистему, `package.json` и один `pnpm-lock.yaml`; первый адаптер — pnpm с проверяемой закреплённой версией. Собственного реестра пакетов и второго решателя зависимостей или файла их фиксации нет.

- [x] Основной стиль — прямые эффектные вызовы с обычным `let`.

- [x] Нотация вызова — чистая стрелка `a -> b` и эффектная стрелка `a ->{r; e} b`.

  - r — требуемые сервисы; e — выбранные обрабатываемые ошибки, а не все варианты данных Cause самого Effect.

  - `a -> Eff r e b` остаётся чистой функцией, создающей описание вычисления; это отдельный тип, а не альтернативная запись эффектного вызова. Подъём в эффектную позицию сохраняет результат `Eff` как значение.

  - IR хранит Pure либо Runtime(r, e), отдельно захваты, стадии применения, порядок выполнения и диапазоны исходного кода. Пустые строки не меняют эффектную модальность.

  - `a ->{} b` остаётся эффектным вызовом. `Unit` обозначает результат `()`, `none` — пустую строку, `Never` — необитаемый обычный тип.

- [x] Полная сигнатура над каждой именованной функцией, включая локальные функции, методы экземпляров классов и частичное применение.

- [x] Функции обратного вызова с полиморфизмом высших рангов и явным вложенным `forall` в полных сигнатурах.

- [x] Числа: Float/BigInt с арифметикой JS; суффикс BigInt — n. Каноническое Eq — SameValueZero; полный порядок Ord с NaN последним, операции сравнения чисел сохраняют семантику JS.

- [x] `task { ... }` создаёт `Eff`; `await task` выполняет его в текущем файбере; приложение запускается в точке входа.

  - Инициализация исходного модуля чистая; эффектная загрузка JS-модуля требует эффектной привязки.

- [x] Классы: MPTC, суперклассы и FD; строгие правила покрытия и согласованности, структурное завершение решателя.

  - Заголовки канонических экземпляров классов не унифицируются; контекст не разрешает пересечения. Запрещены экземпляры-сироты, пересечения, else instance и локальные неявные экземпляры.

  - Экземпляр принадлежит пакету класса либо пакету внешнего номинального конструктора первого параметра. Для другой семантики используются номинальные обёртки или явные словари.

  - Ассоциированные типы и пользовательские семейства типов не входят в первую версию. HKT и аннотированные функции обратного вызова с полиморфизмом высших рангов входят; полиморфная рекурсия требует явной полной forall-сигнатуры.

  - Решатель имеет строгие правила завершения, кеширование и лимит работы, о достижении которого сообщает диагностика; нет UndecidableInstances или выбора по порядку импортов.

  - Обобщение допускается только для доказанно чистых построений, с исключением переменных окружения, локальных сколемов и регионов и неоднозначных ограничений. Эффектные привязки мономорфны; ослабленное правило вариантности отложено.

- [x] Обход: чистый List.map, последовательный List.map_effect, parallel с явным положительным пределом параллелизма; обобщённый do и единый map с полиморфизмом по эффектам отложены.

- [x] Контракт ресурсов: статический запрет выхода за область владения, проверяемое время жизни захватов и связанных конкурентных задач.

  - Сохраняются типизированные ошибки, явный `provide` и финализация средствами Effect.

  - Новая область владения s; T^{s,t}, T^c и T^(c + d) сохраняют транзитивные захваты, включая частичное применение и непрозрачные обёртки. Досрочного закрытия нет; изменяемые хранилища инвариантны. Безопасность при гонках требует подходящего API ресурсов или синхронизации.

  - Двухфазное закрытие: прекратить приём новой работы, прервать дочерние задачи и дождаться их завершения, дождаться прекращения доступа внешних операций к ресурсам, затем выполнить финализаторы ресурсов в порядке LIFO. Комбинаторы структурированной конкурентности и обязательный супервизор области; модель nursery не входит.

- [x] Явные привязки FFI, фиксированный ABI данных и ограниченные публичные TS-интерфейсы; постепенный импорт поддерживаемых `.d.ts`.

  - Средство импорта генерирует тот же FFI и явно сообщает о неподдерживаемых объявлениях.

  - Привязка явно фиксирует режим выполнения, инициализацию модуля, владение и удержание, ошибки, арность и this, отмену и прекращение доступа к ресурсам (quiescence). Аргументы с областью владения и неизвестными правилами удержания отвергаются.

  - Начальное подмножество поддерживаемых `.d.ts` задано в разделе 8; any/unknown → JsValue, неподдерживаемое объявление — заблокированный экспорт с причиной. Content Mapper не заменяет проверяющий модуль исходного языка.

- [x] Автоматический вывод Schema, включая обобщённые и рекурсивные формы данных.

  - Выбраны уточнённые типы, автоматический вывод Schema, schema T и блок codec со свидетельствами. Каждое поле задаётся ровно один раз. Лишние ключи отвергаются, проекция задаётся явно; null, отсутствие поля и значения по умолчанию обрабатываются раздельно.

  - Целевые форматы и адаптеры: строгий JSON Schema 2020-12, Standard Schema V1, Arbitrary с переопределениями и форматтер, учитывающий скрытие секретов. Строгий OpenAPI 3.1 из `HttpApi`; другие форматы — плагины после первой версии. Базовый режим — интерпретатор, AOT включается явно, неявного JIT нет.

  - JsValue импортируется эффектной операцией в инертный Unknown с данными о происхождении; чистый кодек не обходит произвольные геттеры и прокси. Отдельного DataValue нет.

  - Уточнения номинальны и чисты; проверки внешнего состояния эффектны. ABI самого Effect фиксирован; операции Ref/Redacted эффектны, автоматический вывод по умолчанию не раскрывает секреты. Decimal/Duration — проверяемое библиотечное API без специальных литералов.

- [x] Единственная целевая среда выполнения — Effect; чистые участки оптимизируются при генерации кода для неё.

- [x] Адаптировать подходящие оптимизации ReScript к собственному IR; сохранить каррирование, JS Number, ABI Effect и границы выполнения.

- [x] Среда выполнения: восстановление средствами самого Effect; непрозрачные Failure/Exit/RawCause, инертные отчёты, типизированные данные ошибок без регионов; структурированное наблюдение за задачами и race/timeout самого Effect. Оптимизатор release сохраняет обработчики, владельца, Cause, окружение сервисов, отмену и финализаторы.

- [x] Данные и FFI: List = плотный неизменяемый JS-массив; точные структурные записи; канонические Eq/Hash и полный порядок Float Ord; фиксированное представление ADT; четыре семейства функций обратного вызова; интерфейс исходного языка и ограниченный TS-фасад. Модули — явные экспорты, ациклический граф файлов, ESM; компилятор Flick — ядро на Rust, iface — JSON с указанием версии.

**За пределами первой версии:** обобщённые do/let\*, map с полиморфизмом по эффектам, ассоциированные типы и пользовательские семейства типов, пересекающиеся и локальные неявные экземпляры, обобщение на основе вариантности, линейное владение и досрочное закрытие, nursery и тайм-аут с отделённой задачей в области владения, полный импорт TS, литералы десятичных чисел и длительностей, кодеки циклических графов, дополнительные диалекты и сложные расширения SQL, JSX, собственный механизм отрисовки UI, SSR, гидратация, RSC и полная модель хуков. Они исключены из объёма первой версии, а не оставлены как нерешённые вопросы.

- [x] Минимальный набор операций над типами: FieldRow/Symbol/Constraint, конечные кинды, HasField/Lacks/Project/Remove/UnionDisjoint/Map, канонические метки, MPTC/FD, непрозрачные индексы, полиморфизм высших рангов и захваты. Прототипы SQL/UI проверяют принятый набор.

- [x] SQL: PostgreSQL, явные метаданные таблиц, Template/Query, чистые кодеки, EncodeError/DbError/DecodeError. UI: блок ui поверх React, точный набор свойств, типизированные дочерние элементы, чистая отрисовка, явные Exhaust/Latest/Queue/Concurrent, обязательный супервизор и адаптер освобождения ресурсов, которым владеет приложение.

- [x] CLI: `cli "name" { … }` поверх Effect CLI, конструкции `description`/`flag`/`arg`/`subcommand`/`handle`, точная запись опций, `Cli.Value` с автоматическим выводом, `CliError` до запуска обработчика, общий `main` и освобождение ресурсов.
