# Спайк моста λRT ↔ Effect: итоги

Ветка `pc/rt-bridge-spike` поверх старой базы `pc/lambda-rt` (до её rebase), на выброс. Мост — `runtime/src/rt/bridge.ts`, обёртки по сигнатуре — `flick-core/src/types/ffi/bridge_shape.rs` и `codegen/js/foreign.rs`. Нагрузочные цифры сняты на Linux-хосте спайка и не перемерялись; функциональные прогоны, счётчики переходов и профиль CPU повторены на другой Linux-машине, и исходные результаты воспроизвелись один в один.

Режимы: `js` — сборка по умолчанию на Effect; `rt` — `flick build --rt` с прямыми стадиями (закрытый ряд без `Async` — обычная функция); `nd` — `rt` с `FLICK_NO_DIRECT=1` (каждая стадия с рядом — генератор).

## Коротко

- **Q1–Q4: мост работает.** Effect в файбере ядра приостанавливается, падает как причина, прерывается с ожиданием финализаторов; evidence `provide`, `tail` и `abort` доходит до колбэка внутри Stream; `Option`/`Result`/`Exit`/`Failure` переходят границу; слои строятся в обе стороны. Исключение — `control` в колбэке, который вызывает Effect: так нельзя в принципе (ниже).
- **Переход дешёвый, SSR медленнее не из-за него.** Переход стоит 0.1–0.7 мкс, на SSR-запрос их около десяти. Но SSR на мосту даёт ×0.84 rps и +145 мкс CPU на запрос: профиль показывает, что доля Effect не уменьшается (0.35 с в обоих режимах), а ядро добавляется сверху (файберы, owners и слои на каждый запрос). json на мосту быстрее Effect: ×1.06 rps, ×0.74 памяти. Риск 3 подтверждён для конфигурации спайка, где Ui, Atom и сервер на Effect.
- **Прямые стадии на мосту ничего не дают и ломают http.** Нагрузка `rt` и `nd` неразличима, но в `rt` падают 11 кейсов http и server: в рядах яруса B нет `Async`, хотя привязки ждут (ввод-вывод, финализаторы Effect).
- **Обёртки по сигнатуре покрывают не всё.** Обёртку требуют 572 из 2588 привязок яруса B. Недостижимы для формы из сигнатуры: значения внутри непрозрачных типов Effect (`Stream (Eff …)`, `ServiceImpl r`), словари классов, сервисы за переменной типа, `Fiber`, `RawCause`, неявное состояние файбера Effect.
- **Решение 3 (свои `Option`/`Result` с конверсией на границе) не выдерживает значений за переменной типа:** q3 с `FLICK_BRIDGE_PLAIN_DATA=1` ломается на `HashMap.get` и Schema.

## Вопросы

| | Вопрос | Ответ | Где видно |
|-|-|-|-|
| Q1 | Effect в файбере ядра: приостановка, причина, прерывание | да, вывод совпадает с `js` | `results/q/q1.*.txt` |
| Q2 | Колбэк Flick в Stream Effect: evidence `provide`/`tail`/`abort`, типизированная ошибка, прерывание снаружи | да | `results/q/q2.*.txt` |
| Q2c | `control`-операция в колбэке, handler снаружи Stream | **нет**: `a control operation ran in a fiber other than its handler's` во всех режимах ядра; на `js` это `FLICK-P012` | `results/q/q2c.rt.txt` |
| Q3 | `Option`/`Result`/`Exit`/`Failure` через границу, вложенные, через Schema | да, в `rt` и `nd` (до исправления ниже `rt` падал на длинном Stream в Sync-стадии) | `results/q/q3.*.txt` |
| Q4 | Сервис Flick в обработчике сервера Effect; слой Effect в коде ядра; порядок освобождений | да; в `rt` обработчик с закрытым рядом не читает тело; `Layer.merge` освобождает стороны по очереди, а не параллельно (`shortcut:` в `rt/layer.ts`, на `lambda-rt` исправлено в `accaadaa`) | `results/q/q4.*.txt` |

Q1, ключевое:

```
interrupting
  effect: interrupted, finalizer starts
  effect: finalizer ends
Fiber.interrupt returned
kernel finalizer of the fiber
```

Q2c — неустранимо: `callEffect` запускает на каждый вызов из Effect свой файбер ядра, и продолжение `control` должно было бы захватить кадры Stream между операцией и handler'ом, а ядро захватывает только свои кадры.

## Производительность

### Нагрузка (медианы пяти чередующихся прогонов, A = `js`, B = мост)

| Сервер | Режим | rps | p95, мс | VmHWM, МиБ | CPU, мкс/ответ | B / A |
|-|-|-|-|-|-|-|
| json | `js` | 3278 | 7.95 | 227.9 | 313 | |
| json | `rt` | 3471 | 7.51 | 169.0 | 288 | rps ×1.059, память ×0.742 |
| json | `js` | 3195 | 8.08 | 226.4 | 314 | |
| json | `nd` | 3442 | 7.48 | 168.8 | 293 | rps ×1.077, память ×0.746 |
| ssr | `js` | 1843 | 14.49 | 415.7 | 619 | |
| ssr | `rt` | 1544 | 17.46 | 393.6 | 764 | rps ×0.838, память ×0.947 |
| ssr | `js` | 1915 | 14.12 | 379.5 | 598 | |
| ssr | `nd` | 1575 | 16.91 | 391.5 | 746 | rps ×0.822, память ×1.032 |

Файлы: `results/json2.tsv`, `json-nd.tsv`, `ssr.tsv`, `ssr-nd.tsv`, медианы — `medians.py`. Первый прогон json (`results/json.tsv`) шёл при load average 15–25, его разброс 1.2–3.5k rps, и в выводы он не входит.

### Цена перехода (`results/micro-2.txt`, нс на итерацию)

| Замер | `rt` | `js` | Что это |
|-|-|-|-|
| `kernel_loop_baseline` | 89 | 9685 | цикл Flick с `Async`, без перехода |
| `from_effect_succeed` | 172 | 6172 | `Effect.succeed` из ядра: быстрый путь, +83 нс |
| `from_effect_sync` | 561 | 5913 | полный запуск файбера Effect, +473 нс |
| `from_effect_two_steps` | 764 | 6593 | два шага Effect, +675 нс |
| `effect_loop_baseline` | 114 | 112 | цикл Effect без перехода |
| `callback_plain` | 501 | 2543 | Effect вызывает функцию Flick, +387 нс |
| `callback_generator` | 688 | 7700 | то же, стадия-генератор, +574 нс |
| `callback_suspends` | 2552 | 13567 | колбэк с `yield_now` |

Один прогон в каждом режиме. Цифры `js` — цена того же кода Flick на Effect; переход на мосту дешевле, чем вызов Flick-кода на Effect сейчас.

### Переходы на запрос (`crossings.sh`, 1000 запросов; `results/count.*`, `results/sites.*`)

| | json `/item` | ssr `/render` |
|-|-|-|
| `fromEffect`, из них полных запусков | 4, 3 | 8, 7 |
| `callEffect` | 2 | 2 |
| `callSync` | 0 | 1 |
| операций сервиса Effect из ядра (`hostOp`) | 1 | 3 |
| `adopt` (слой Effect собран для ядра) | 0 | 1 |

По месту в скомпилированном коде (`FLICK_BRIDGE_COUNT=sites`):

- Круг сервер → приложение: `Http.Server.serve` получает функцию Flick (`callEffect`), а эта функция — роутер Effect, обёрнутый `fromEffect` (`Http/Router.js:39`). Один `callEffect` и один полный запуск на запрос уходят на возврат значения туда, откуда оно пришло: обёртка не узнаёт значение, которое сама обернула.
- json: `Codec.decode`, `ServerResponse.schema_json` — по одному полному запуску.
- ssr: `provide Atom.layer` собирает реестр на каждый запрос (`adopt` и запуск `Layer.buildWithMemoMap`) — это задумано самим тестом («with a registry of its own»), промаха memo map нет. Ещё `Atom.set`, `Ui.Server.render` и около трёх закрытий scope Effect из owners ядра (`settled`, `hostOwnerOf`).

### Куда уходит CPU на SSR (`results/ssr-profile.txt`, `profile.mjs cpu`)

`node --cpu-prof`, 300 запросов на прогрев и 1500 последовательных в каждом режиме. Занятое время: `rt` 1.68 с, `js` 1.42 с, то есть +144 мкс на запрос; это совпадает с +145 мкс по нагрузке.

| | `rt`, с | `js`, с |
|-|-|-|
| Effect (`node_modules/effect`) | 0.35 | 0.35 |
| ядро `runtime/dist/rt/*` без моста | 0.12 | — |
| мост `bridge.js` | 0.04 | — |
| рантайм на стороне Effect (`runtime/dist/*`) | 0.09 | 0.13 |
| GC | 0.12 | 0.08 |
| остальное: node, нативное, React, код программы и std | 0.96 | 0.86 |

Внутри ядра больше всего занимают `drive` 47 мс, сборка слоёв (`layer.js`) 18 мс, `hostOwnerOf` 11 мс, `memoOf` 6 мс, сам `fromEffect` 5 мс. Переходы как таковые — проценты разницы, основное — работа ядра на каждый запрос поверх неуменьшившейся работы Effect: файберы на `callEffect`, owner ядра со своим Scope Effect, форк memo map на каждый `provide`.

**Риск 3:** подтверждён для спайка: когда Ui, Atom и сервер на Effect, мост добавляет ядро сверху. На `pc/lambda-rt` Ui и атомы уже на ядре (`55006617`, `74f99fb7`, `4e164824`), и из SSR-пути уходят `adopt`, `Atom.set` и `Ui.Server.render` через мост; останутся круг роутера и http. Ожидание — разрыв сократится, но это не замерено.

## Ярус B в цифрах (`FLICK_BRIDGE_DUMP` + `count.py`)

| | Модулей | Привязок | Нужна обёртка | `fun`/`Eff` в параметре | в результате | `Exit`/`Failure` | `Layer` | `foreign effect` | из них без `Async` (закрытый / открытый ряд) |
|-|-|-|-|-|-|-|-|-|-|
| весь std | 186 | 2829 | 671 | 341 | 324 | 115 | 72 | 807 | 667 (402 / 265) |
| таблица яруса A | 15 | 241 | 99 | 42 | 24 | 37 | 31 | 57 | 42 (30 / 12) |
| ярус B = std без A | 171 | 2588 | 572 | 299 | 300 | 78 | 41 | 750 | 625 (372 / 253) |
| ярус B, названные в плане модули | 67 | 1211 | 240 | 174 | 57 | 21 | 27 | 179 | 137 (38 / 99) |

Последний столбец — привязки, которые в режиме `rt` могут оказаться в прямой стадии и ждать: на Effect ряд не обязан говорить `Async`, на ядре обязан.

## Падения

Прогон `stdlib-cases.sh` по `tests/run/stdlib`: до исправлений спайка (как в `results/rt-*.tsv`, воспроизвелось точно) и после (`results/after/`).

| Набор | До | После |
|-|-|-|
| http и server, `rt` | 15 ok, 12 failed | 15 ok, 12 failed |
| http и server, `nd` | 26 ok, 1 failed | 26 ok, 1 failed |
| остальное, `nd` | 99 ok, 17 failed, 2 timeout | 100 ok, 16 failed, 2 timeout |

### Только в `rt`: прямая стадия ждёт

Ошибка `a stage that runs at once waited for something that has not happened`. Место нашлось по стеку `runIn`; ни одно из них не потеря хода, все — настоящее ожидание (исправление «Effect не отдаёт ход» их не меняет):

| Кейсы | Что ждёт в стадии с закрытым рядом |
|-|-|
| `http/serve`, `http/server`, q4 | `ServerRequest.schema_body_json`: ряд `rd` берётся у кодека, обычно `none`, а тело читается из сети |
| `http/api_bodies` | `Stream.run_collect` по потоку файла |
| `http/forms`, `http/streams`, `http/types` | `Scope.run`: при закрытии ждут финализаторы Effect (временные файлы multipart) |
| `http/values`, `server/node` | `Layer.run` слоя Effect (`Http.Etag.layer`, `Server.node`) |
| `http/handlers`, `http/inject`, `http/spa` | обработчик или middleware с закрытым рядом вокруг роутера Effect |

`daac772a` на `lambda-rt` (Queue, `Layer.launch`, исполнители расписаний говорят `Async`) этих мест не касается и их не закрывает.

### В обоих режимах

| Кейс | Причина | Класс | Статус |
|-|-|-|-|
| `http/redaction_context` | Effect читает настройки маскировки из `globalThis["~effect/Fiber/currentFiber"].context` (`Redactable.js:110`); после `Deferred.await_` код ядра идёт без «текущего файбера» Effect, и параллельный файбер видит чужие имена | мост: неявное состояние Effect | открыто |
| `effect/observe` | `annotate_logs_scoped` меняет состояние файбера Effect одного перехода, и оно кончается вместе с ним | мост: неявное состояние Effect | открыто |
| `effect/metric` | метрики рантайма Effect не видят файберов ядра | неустранимо при двух рантаймах | открыто |
| `effect/effect_errors` | `uninterruptibleMask`/`interruptibleMask` ядра вызывали результат `f` как thunk | ядро, ярус A | **исправлено в спайке**; тот же код на `lambda-rt` |
| `effect/cache` | обёртка делала колбэк некаррированным (`f(...) is not a function`) | мост: обёртки | **исправлено**; теперь освобождения идут поздно: scope перехода закрывается вместе с owner'ом ядра (`settled`) |
| `effect/request` | то же каррирование | мост: обёртки | **исправлено**; дальше падает как `fibers` |
| `effect/fibers`, `effect/layer_map`, `effect/request` | `Fiber` Effect (FiberMap, LayerMap) попадает в API файберов ядра: `fiber.observe`, `child.interrupt is not a function` | мост: нет формы для `Fiber` | открыто |
| `effect/exit_cause` | `Cause.reason_stack_trace` на стороне Effect получает причину ядра | ярус A ещё на Effect | на `lambda-rt` — `bb60b2e2` |
| `effect/effect_errors_more` | `RawCause.inspect` на стороне Effect, для `RawCause` нет формы | ярус A ещё на Effect | на `lambda-rt` — `bb60b2e2` |
| `effect/deferred` (timeout) | `Deferred.complete_with` ядра отдаёт задачу как значение | ядро, ярус A | на `lambda-rt` — `4df34c3e` |
| `effect/layer` | `Layer.update_service`: реализация ядра внутри непрозрачного `ServiceImpl r` уходит в Effect без обёртки | мост: непрозрачный тип | на `lambda-rt` `Layer` на ядре (`80d669ae`, `accaadaa`) |
| `effect/stream_errors` | `Stream.update_service`: сервис за переменной `s`, его операции после `implOut` возвращают Effect | мост: сервис за переменной типа | открыто |
| `effect/stream_transform` | `Stream.flatten_effect`: `Eff` внутри `Stream`, форма до него не доходит | мост: непрозрачный тип | открыто |
| `cli/*` (5 кейсов) | словарь класса `Cli.Value` переходит без конверсии: `parse` на ядре возвращает `Result`, адаптер `runtime/src/cli/run.ts` ждёт Effect (`Not a valid effect: success(…)`) | мост: словари классов | открыто |
| `effect/txlock` (timeout) | чтения STM расходятся (1 против 2), затем зависание | не разобрано | открыто |

### Решение 3 на практике

q3 с `FLICK_BRIDGE_PLAIN_DATA=1` — свои `Option`/`Result`, как в решении 3 (`results/q/q3.rt-plain-data.txt`):

```
Effect's Some: Some 1 equal: false none: false
schema error
```

`HashMap.get` возвращает `Option` Effect за переменной `a`, и сравнение с `Some 1` ложно; Schema кодирует запись Flick с `Option` внутри и не узнаёт его. Форма из сигнатуры конвертирует `Option` там, где он виден (`List (Option Int)` в q3 проходит), но не за переменной типа и не внутри данных, в которые смотрит только Schema.

## Что исправлено в спайке

- `runtime/src/rt/bridge.ts`: файбер Effect, запущенный из ядра, не отдаёт ход по бюджету операций (`Scheduler.PreventSchedulerYield`), пока ничего не ждёт; `FLICK_BRIDGE_YIELD=1` возвращает старое поведение. Чинит q3 в `rt`.
- `codegen/js/foreign.rs`: функция уходит в Effect каррированной, как её зовёт Flick, а арность колбэка адаптирует `callback`, как без моста. Чинит `f(...) is not a function` в `cache`, `request`.
- `runtime/src/rt/effect/Effect.ts`: `uninterruptible_mask` и `interruptible_mask` на ядре. Чинит `effect_errors`.
- `FLICK_BRIDGE_COUNT=sites` — переходы по месту в скомпилированном коде; `profile.mjs` — профиль CPU по слоям (`cpu`) и отношения микрозамера (`micro`).
- Рантайм проходит `tsc` (раньше `pnpm install` падал на `bridge.ts`, а `dist` всё равно собирался); `stdlib-cases.sh` берёт `gtimeout` на macOS.

Тест `rt-kernel.test.ts` «nothing the runtime's index reaches imports Effect» падает с первого коммита спайка: `rt/index.ts` экспортирует `bridge.ts`.

## Рекомендации для шага «`effect-ts` и мост» на `pc/lambda-rt`

Оставить:

1. Обёртки по сигнатуре (`Shape`): Q1–Q4 проходят на них без ручного кода в привязках.
2. Evidence через `Ctx.host` ↔ `KERNEL`: `provide`, `tail`, `abort` доходят до колбэков внутри Stream (Q2).
3. Быстрый путь `Success`/`Failure` в `fromEffect` и `PreventSchedulerYield` для запусков из ядра.
4. Общие `Option`/`Result` с Effect (`useHostData`), пока Schema, HashMap и прочие владельцы значений за переменной типа на Effect. Решение 3 стоит пересмотреть или отложить до переноса этих модулей.

Переделать:

5. Мост — отдельная точка входа (`packages/effect-ts`, решение 4), не `rt/index`: иначе ядро тянет Effect.
6. Обёртки каррированные, адаптер арности поверх (уже в спайке).
7. Значение, вернувшееся туда, откуда пришло, разворачивается (оригинал в `WeakMap`): это убирает круг сервер → роутер, один `callEffect` и один полный запуск на каждый http-запрос.
8. Формы, которых нет: `Fiber`, `RawCause`, `Cause`; методы словарей классов (тип метода компилятор знает); сервисы — по тегу и его операциям, а не `Proxy` `implOut`, который угадывает стадию по результату. Для `Eff` и функций в аргументах непрозрачных типов Effect (`Stream`, `ServiceImpl`) — ошибка проверки при сборке на ядре, а не тихий проход.
9. Owners: `hostOwnerOf` (один Scope Effect на owner ядра) и `settled` (scope перехода уходит в owner) дают поздние освобождения (`cache`) и заметную долю CPU на SSR. С `Layer` как handler (`accaadaa`) `adopt` надо перепроектировать: слой Effect становится handler'ом, а не набором сервисов под `HOST`.
10. Неявное состояние Effect: пока код ядра идёт с `ctx.host`, ставить «текущий файбер» Effect на заглушку с контекстом этого `host` (`redaction_context`), либо перечислить такие привязки как неподдерживаемые.
11. Стадии: пока модуль яруса B на мосту, закрытые ряды, которые до него доходят, компилировать как стадии-генераторы (поведение `FLICK_NO_DIRECT`): нагрузка `rt` и `nd` неразличима, а прямые стадии ломают 11 кейсов http и server. Альтернатива — `Async` в рядах 625 привязок яруса B, это ломает исходники пользователей.
12. `control` через Effect: ошибка проверки в духе `FLICK-P012` для `control` в ряду колбэка, который уходит в `foreign effect`.

## Как воспроизвести

```bash
just e2e-prepare
tools/load/spike/build.sh q1 q2 q2c q3 q4          # dist/rt и dist/js в каждом
(cd tools/load/spike/q1 && node dist/rt/flick-server.js)
tools/load/spike/stdlib-cases.sh rt stdlib/http stdlib/server                 # rt
FLICK_NO_DIRECT=1 tools/load/spike/stdlib-cases.sh rt stdlib/http stdlib/server
(cd tools/load/ssr && flick build --rt --out-dir dist/rt) && tools/load/spike/crossings.sh
FLICK_BRIDGE_COUNT=sites node tools/load/ssr/dist/rt/flick-server.js          # и запросы
FLICK_BRIDGE_DUMP=$PWD/tools/load/spike/dump.tsv flick --project packages/std check && python3 tools/load/spike/count.py
python3 tools/load/spike/medians.py tools/load/spike/results/*.tsv
node tools/load/spike/profile.mjs micro                                      # results/micro-2.txt
node tools/load/spike/profile.mjs cpu rt js                                  # /tmp/prof/<mode>/*.cpuprofile от node --cpu-prof
```

Нагрузка — `run-all.sh` (Linux: `taskset`, `/proc`), микрозамер — `micro.sh`.
