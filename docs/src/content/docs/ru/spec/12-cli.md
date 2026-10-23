---
title: "12. CLI Flick"
---

CLI называется `flick`. Ниже зафиксирован интерфейс для реализации первой версии Flick; наличие команды в спецификации не означает, что она уже реализована. Все команды используют один Rust core и выбранный Effect backend.

## Имена файлов и конфигурация проекта

| Объект | Соглашение |
| - | - |
| Язык и инструменты | Flick; по-русски — «флик»; исполняемый файл — `flick` |
| Исходный файл | `.fl`, например `src/App/Main.fl` для модуля `App.Main` |
| Конфигурация компилятора | `flick.toml`; схема конфигурации имеет собственную версию |
| Пакет и зависимости | `package.json`; первый поддерживаемый package manager — pnpm |
| Зафиксированное дерево зависимостей | Один `pnpm-lock.yaml` на проект или pnpm workspace |
| Родной интерфейс | Версионированный JSON `.iface`; сохраняет полные типы и ABI Flick |
| Публичный интерфейс для TS | `.d.ts`, если запрошен и представим выбранный façade |
| Артефакты сборки | По умолчанию `dist/debug/` и `dist/release/` |
| Локальный кеш и служебные файлы | `.flick/`; кеш отделён от исходников и lockfile |

Один `.fl` файл объявляет один модуль. Для обычных исходников путь внутри source root соответствует имени модуля: `App.Users` размещается в `src/App/Users.fl`. Test root образует отдельный набор модулей с теми же правилами. Коллизия полного имени модуля диагностируется. Единственное расширение исходников в первой версии — `.fl`.

```toml
schema-version = 1

[project]
kind = "app"
source-dir = "src"
test-dir = "test"
entry = "App.Main"

[build]
out-dir = "dist"
profile = "debug"
emit = "js"
emit-dts = false
schema-aot = false
```

`kind` принимает `app` или `lib`; библиотеке `entry` не нужен. `entry` указывает модуль с экспортированным `main`. Имена пакетов, их версии и dependencies хранятся в `package.json`, настройки Flick — в `flick.toml`. Команда ищет ближайший `flick.toml` от рабочего каталога вверх; `--project <path>` явно выбирает каталог проекта или конфигурационный файл.

## Повседневные команды

| Команда | Назначение и контракт |
| - | - |
| `flick new <directory> --template app` | Создать новый каталог с конфигурацией, `package.json` и примером приложения. `--template lib` создаёт библиотеку. |
| `flick init --template app` | Добавить тот же каркас в текущий каталог; существующие файлы не перезаписываются, конфликт показывается до записи. |
| `flick check [paths...]` | Проверить синтаксис, kinds, типы, effects, instances, captures, imports и `.iface`. Без аргументов проверяет исходники и тесты проекта. |
| `flick build` | Проверить проект и выпустить ESM, source maps и родные интерфейсы в каталог выбранного профиля. |
| `flick run -- [args...]` | Собрать JS и запустить `main` из конфигурации; аргументы после `--` передать приложению. `--entry App.Main` явно выбирает модуль входа. |
| `flick dev -- [args...]` | Наблюдать за исходниками, пересобирать приложение и последовательно заменять запущенную версию после корректного shutdown. |
| `flick test [paths...]` | Собрать и выполнить тесты из test root; каждый тест получает собственный scoped owner. |
| `flick fmt [paths...]` | Форматировать исходники Flick. `--check` проверяет формат без записи; `--stdin --stdin-filepath <file>` обслуживает редактор. |
| `flick clean` | Удалить только учтённые артефакты этого проекта; `--cache` дополнительно удаляет его кеш Flick. |

`new` и `init` по умолчанию только создают файлы; установка выполняется отдельной командой `install`. `check`, `build` и сборочные стадии `run`/`dev`/`test` не устанавливают зависимости, не запускают `main` и не выполняют эффектную инициализацию foreign modules. Обычная проверка SQL не подключается к БД. Отсутствующая зависимость даёт диагностику с инструкцией установки.

Выбор `paths` задаёт корни проверки, но не отключает проверку их транзитивных imports, coherence и ABI. `check --watch` только перепроверяет код; `build --watch` обновляет артефакты; запуском и перезапуском приложения управляет `dev`.

## Зависимости и упаковка

Загрузка пакетов и выбор их версий делегируются pnpm и npm-экосистеме. Flick проверяет типизированные imports, `.iface`, canonical instances и runtime ABI поверх установленного дерева. Отдельного solver версий, registry и второго dependency lockfile у Flick нет. Начальный adapter поддерживает pnpm; adapters для других package managers можно добавить позже без изменения системы модулей.

| Команда | Назначение |
| - | - |
| `flick install` | Установить зависимости через закреплённый pnpm и проверить совместимость toolchain/runtime. |
| `flick install --locked` | Установить строго по существующему lockfile; отсутствие lockfile или расхождение с manifest — ошибка. |
| `flick install --locked --offline` | Та же установка только из локального store; недостающий пакет — ошибка. |
| `flick add <package-spec> [--dev]` | Добавить npm-совместимый пакет в dependencies или devDependencies и обновить lockfile. Импорт его JS API по-прежнему требует FFI. |
| `flick remove <package>` | Удалить прямую зависимость через package manager; оставшиеся исходные imports затем диагностирует checker. |
| `flick update [package]` | Обновить разрешённые версии в пределах manifest constraints. Зафиксированные версии compiler/runtime/Effect не меняются неявно. |
| `flick deps [--why <package>]` | Показать установленный граф, причины появления зависимости и сведения о доступных интерфейсах Flick. |
| `flick pack --out-dir artifacts` | Собрать release-пакет и локальный npm-совместимый tarball с ESM, `.iface` и объявленными TS façades. |

`install --locked` отображается на `pnpm install --frozen-lockfile`, а `--offline` — на одноимённый флаг pnpm. Эти режимы и их поведение описаны в [pnpm install](https://pnpm.io/cli/install). Manifest и зависимости используют обычный [package.json](https://pnpm.io/package_json); добавление пакетов делегируется [pnpm add](https://pnpm.io/cli/add).

Шаблон проекта закрепляет конкретные версии compiler, runtime adapter и Effect, а также версию pnpm через `packageManager`. Команды проверяют соответствие локального toolchain и runtime capability matrix из раздела 11; несовместимый ABI не обходится через TS-проекцию. Перед изменением manifest, lockfile или установленного дерева adapter проверяет фактическую версию вызываемого pnpm. Если закреплённая версия недоступна, команда завершается с кодом `2` и инструкцией установки; произвольная доступная версия не подставляется. Расширение поддерживаемых версий Effect требует проверки контрактов, а не только изменения semver range.

`pack` проверяет комплектность объявленных exports, `.iface`, dependency fingerprints и файлов пакета. Импорт упакованной библиотеки не запускает приложение. Публикация tarball выполняется обычным package manager отдельным действием.

## Сборка и оптимизации

`build`, `run`, `dev` и `test` принимают `--profile debug` или `--profile release`; по умолчанию используется профиль из конфигурации. `debug` — консервативный эталонный lowering. `release` включает разрешённые typed IR passes из раздела 8 и сохраняет тот же Effect runtime и наблюдаемые контракты.

| Параметр build | Значение |
| - | - |
| `--emit js` | Исполняемые ESM-модули; стандартный вывод backend. |
| `--emit ts` | TS-представление исполняемого кода для дальнейшего JS-tooling; это не редакторная виртуальная проекция. |
| `--emit-dts` | Проверяемый публичный TS façade. Непредставимый export требует явного wrapper или изменения выбранного façade; тихого `any` нет. |
| `--schema-aot` | Включить выбранную AOT-оптимизацию Schema; неподдержанный узел может сохранить interpreter semantics. |
| `--out-dir <directory>` | Изменить базовый каталог артефактов; профили сохраняются в отдельных подкаталогах. |
| `--watch` | Пересобирать при изменениях, сохраняя последнюю успешно опубликованную генерацию при ошибке. |

`run`, `dev` и `test` всегда строят исполняемый JS, независимо от настройки `emit` и режима вывода `build --emit ts`. Первый поддерживаемый host этих команд — Node.js. Browser/React-код выпускается как ESM и подключается к JS-host и bundler; отдельного Rust/WASM backend или автоматического запуска browser-приложения этот CLI не вводит.

Кеш учитывает исходники, compiler и language version, нормализованную эффективную конфигурацию проекта и компилятора, включая source/test roots и `schema-aot`, профиль и emit flags, версии stdlib/runtime, lockfile, `.iface` и dependency fingerprints, FFI contracts и версии задействованных TS-инструментов. Артефакты новой генерации объявляются готовыми только после успешной сборки всего её набора. Запущенная генерация использует неизменяемый набор исполняемых модулей и разрешённых зависимостей, например в `.flick/run/<generation>/`; поздний import не должен увидеть файлы следующей сборки. Этот набор сохраняется до завершения shutdown. Параллельная замена dependency tree допустима только при сохранении такого snapshot; иначе изменение отклоняется до записи с диагностикой занятого дерева. `clean` работает по manifest артефактов Flick, сохраняет активные генерации и их locks и не удаляет пользовательские файлы в общем output-каталоге.

## Запуск и безопасный перезапуск

`run` вызывает один внешний platform runner. Тип входа остаётся `main : Unit ->{; AppError} Unit`; зависимости и Layers предоставляются явно внутри `main`. CLI не превращает каждый прямой вызов или `await` в отдельный запуск runtime. Аргументы командной строки доступны через явное API платформы. Нормальный возврат `Unit`, typed `AppError`, defect и interruption завершают запуск через закрытие его корневого owner; результат команды известен только после cleanup. Тестовый runner соблюдает тот же порядок для каждого теста.

`dev` может проверять и собирать следующую версию, пока работает текущая, сохраняя изоляцию генераций, описанную выше. При ошибке сборки текущая версия продолжает работать. После успешной сборки runner закрывает admission старой версии, запрашивает interruption, ждёт join дочерних fibers и quiescence зарегистрированной foreign work, затем завершает LIFO finalizers. Новая версия запускается только после подтверждённого завершения shutdown; ошибка cleanup останавливает автоматический перезапуск.

SIGINT/SIGTERM запускают тот же протокол завершения. Пока cleanup не завершён, команда не сообщает об успешном выходе; при задержке показывает состояние shutdown. Это не hard deadline: uninterruptible work может задержать остановку. Принудительное завершение процесса ОС не считается доказательством выполнения finalizers. Автоматический in-process hot reload с сохранением старых scoped handles в первую версию не входит.

## Тесты и воспроизводимость

`test` поддерживает `--filter <text>` по полному имени теста, `--jobs <positive-number>` для явного предела параллелизма, `--watch` и `--reporter human` или `--reporter json`. По умолчанию `--jobs 1`. Повторный test run начинается после закрытия owners предыдущего запуска; отмена теста не разрешает пропустить cleanup.

Тестовая библиотека поддерживает чистые и эффектные тесты, а также отдельные property tests с явными generators. `--seed <integer>` задаёт seed для генераторов и выводится при провале; он не обещает воспроизвести расписание fibers, сеть или внешнюю БД. Assertions, typed failures, defects и cleanup failures отражаются в результате теста раздельно.

Обычный CI проверяет приложение в выбранном профиле. Для compiler/runtime Flick semantic regression suites прогоняются в `debug` и `release`, чтобы проверять сохранение установленных наблюдаемых контрактов; два успешных запуска не считаются общим доказательством эквивалентности всех программ.

## FFI Schema документация и редактор

| Команда | Контракт |
| - | - |
| `flick ffi import <package> --out <directory>` | Прочитать установленные `.d.ts`, сгенерировать поддерживаемые bindings и отчёт покрытия; `--select <names>` ограничивает поверхность. |
| `flick ffi check` | Проверить generated bindings, отдельно хранимые reviewed overrides, fingerprints и обязательные adapter policies после изменений зависимостей. |
| `flick schema export <Module.codec> --target json-schema-2020-12 --out <file>` | Строго экспортировать статически представимый JSON wire-codec, с диагностикой node/path для неподдержанного контракта. |
| `flick schema export <Module.api> --target openapi-3.1 --out <file>` | Строго экспортировать OpenAPI 3.1 для значения `HttpApi`, с диагностикой endpoint/path для неподдержанной schema. |
| `flick doc --out-dir docs` | Сформировать документацию public API: типы Flick, kinds, constraints, эффекты, captures и сигнатуры экспортируемых значений. |
| `flick lsp --stdio` | Запустить исходный language server с тем же Rust core и проектной конфигурацией. |
| `flick explain <diagnostic-code>` | Объяснить диагностику Flick и показать небольшие примеры исправлений. |
| `flick doctor` | Проверить compiler, Node.js, pnpm, runtime/Effect ABI и состояние подключённых TS/editor adapters. |
| `flick inspect <module> --emit <representation>` | Вывести `ast`, `typed-ir`, `js`, `ts`, `iface` или `mapping` для исследования codegen и source maps. |

`ffi import` разделяет supported exports, exports с недостающей policy и unsupported exports. Обязательные правила purity, execution, retention, cancellation и ownership не выводятся автоматически из `.d.ts`. Reviewed overrides не перезаписываются регенерацией. Заблокированные экспорты остаются именованными; их запрос или использование даёт ошибку, при этом независимые поддержанные exports сохраняются.

`schema export` использует тот strict wire-контракт, который задан в разделе 9. Команда не исполняет application main и произвольные пользовательские runtime-преобразования ради экспорта. Динамически построенные schemas экспортируются через явное runtime API. Standard Schema V1 и Arbitrary остаются библиотечными adapters/generators; они не выдаются за статические JSON-файлы. OpenAPI 3.1 экспортируется из статически известного значения `HttpApi` тем же strict-контрактом (`--target openapi-3.1`): документ не содержит `{}`/`any`, а неподдержанная schema endpoint'а даёт ExportError с путём. Swagger UI и Scalar — библиотечные Layers, которые отдают этот же документ, а не строят собственное приближение.

`lsp` обслуживает source diagnostics, hover, definition, completion, rename и formatting. Его stdout целиком принадлежит протоколу; логи идут в stderr. Content Mapper остаётся отдельной версионированной интеграцией и не является условием работы source checker. Отсутствие необязательного Mapper adapter в `doctor` само по себе не блокирует компиляцию.

## Общие параметры и диагностика

| Параметр | Назначение |
| - | - |
| `--project <path>` | Явно выбрать проект. |
| `--diagnostics human` или `--diagnostics json` | Формат diagnostics в stderr; JSON использует поток NDJSON с версией схемы. |
| `--warnings-as-errors` | Сделать предупреждения ошибками для результата команды. |
| `--color auto`, `--color always`, `--color never` | Управлять оформлением человекочитаемого вывода. |
| `--quiet` или `--verbose` | Сократить служебный вывод либо включить подробные сведения. |
| `--help` и `flick help <command>` | Справка по CLI или конкретной команде. |
| `--version` | Версия compiler Flick; подробная совместимость доступна через `doctor`. |

Диагностика имеет стабильный код с префиксом `FLICK-`, severity, сообщение, исходный путь, диапазон UTF-8 bytes, related spans и возможные исправления. Диапазон относится к `.fl` исходнику, если соответствие известно; ошибку внешнего JS/TS нельзя приписывать произвольному source span. LSP adapter переводит диапазон в кодировку позиций редактора.

stdout предназначен для результата команды, stdout приложения или выбранного протокола; служебные сообщения и diagnostics не вмешиваются в этот поток. При `--diagnostics json` весь stderr принадлежит одному сериализатору versioned NDJSON: compiler diagnostics, служебные logs, stderr приложения и вывод package manager становятся типизированными событиями `diagnostic`, `log`, `application-stderr`, `package-manager-output`. Для перехваченного вывода сохраняются исходный канал и текст; сырые строки дочерних процессов и ANSI-коды в этот поток не подмешиваются. В human-режиме stderr приложения может передаваться напрямую. `test --reporter json` отдельно выдаёт в stdout структурированные события тестов и включает захваченные test logs как поля событий. stdout LSP всегда принадлежит только его протоколу.

| Код завершения | Значение |
| - | - |
| `0` | Успех; для `fmt --check` форматирование уже соответствует правилам. |
| `1` | Ошибка проверки/сборки, провал тестов, несоответствие форматирования либо неуспешное выполнение приложения/cleanup. |
| `2` | Неверный вызов, конфигурация, отсутствующая зависимость, занятое дерево зависимостей или несовместимый toolchain/ABI. |
| `3` | Внутренняя ошибка compiler/tooling Flick. |
| `130` / `143` | SIGINT / SIGTERM, если причиной остановки было только interruption от сигнала и cleanup успешно завершён. |

Ошибка приложения или cleanup сохраняет код `1` и диагностику итогового `Cause`, даже если одновременно пришёл сигнал. Пока cleanup не завершён, команда не сообщает об успешном завершении или окончательном коде выхода.

## Примеры работы

```sh
flick new my-app --template app
cd my-app
flick install
flick check
flick dev -- --port 3000
```

```sh
flick fmt --check
flick check --diagnostics json
flick test --filter Users --jobs 4
flick build --profile release
flick run --profile release -- --port 3000
```

```sh
flick add date-fns
flick ffi import date-fns --out src/Foreign/DateFns
flick ffi check
flick schema export App.Domain.user_json --target json-schema-2020-12 --out schema/user.schema.json
flick doc --out-dir docs
```

Пример с `date-fns` показывает последовательность действий, а не заранее обещанное полное покрытие его API. После импорта разработчик заполняет требуемые reviewed adapter policies; `ffi check` проверяет их. `App.Domain.user_json` в примере — явно объявленный JSON codec проекта.

```sh
flick install --locked
flick fmt --check
flick check --warnings-as-errors
flick test --profile release
flick build --profile release
```

Последний набор — базовый CI. Cache miss не меняет результат команды; lockfile и toolchain pins входят в ключ кеширования. Обновление зависимостей выполняется отдельно от CI-проверки.

## Порядок реализации команд

| Этап | Состав |
| - | - |
| Первый исполняемый срез | `new`, `init`, `check`, `build`, `run`, `fmt`, help/version и минимальный `install`; один проект и Node.js host. |
| Цикл разработки | `dev`, `test`, `clean`, остальные команды зависимостей, `lsp`, `doctor`, `explain`; стабильные JSON diagnostics и инкрементальный кеш. |
| Полный инструментальный набор первой версии | `ffi import`, `ffi check`, `schema export`, `doc`, `pack`, `inspect`; соответствующие языковые возможности остаются частью выбранного дизайна v1. |
| После первой версии | REPL, отдельный benchmark runner, coverage, live docs, дополнительные package-manager adapters и автоматизированная React/bundler-интеграция. |

Этапы задают порядок реализации, а не альтернативные версии семантики. Публичные команды обращаются к общей модели проекта и одному checker; оптимизация, FFI import и editor projection не образуют независимые источники типов.
