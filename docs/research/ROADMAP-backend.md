# Роадмап: Go-backend и переносимое ядро исполнения

Статус: рабочий; сделаны G0–G4 (веха «Чистый Flick в Go») и G10a — по решению владельца раньше G7; из G13 сделаны директивы `//line`. Основание — [alternative-backend.md](alternative-backend.md); решения §6 приняты 2026-10-28 и закрывают вопросы его §11.
Язык не в релизе, поэтому изменения спецификации допустимы и deprecated-релизов не делаем.

Обозначения: ✅ сделано (коммит), 🔶 частично / в работе, ⬜ не начато, ⏸ условный пункт (запускается только при выполнении условия).

## 1. Цели и принципы

1. **Один язык, несколько backend.** Flick определяет собственную семантику исполнения; Effect — одна её реализация (JS-target), Go — вторая. Frontend, типы, классы, регионы, IDE — общие.
2. **Самостоятельный executable.** `flick build` для Go-target даёт один бинарник без Node и JS/Effect; GC, scheduler, сеть — из Go runtime.
3. **Паритет, а не подмножество.** Поддерживаемая программа ведёт себя на обоих target одинаково: те же `.stdout`, те же ошибки, тот же порядок cleanup. Cause, Layer и Schema — с паритетом Effect (решения §6.4–6.6).
4. **Неподдерживаемое — диагностика при сборке.** Модуль, которого нет на target, — ошибка компилятора, а не runtime-заглушка.
5. **Сначала срез, потом ширина.** Вертикальный срез G7 проверяет риски runtime до вложений в stdlib; по его итогам — go/no-go.
6. **Без универсальной plugin-архитектуры.** Границы между backend выделяются по мере надобности второго конкретного target.

## 2. Целевой вид

```toml
[build]
emit = "go"          # рядом с "js" и "ts"; имя ключа фиксирует G0
```

```sh
flick build --release            # dist/<entry> — статический executable
GOOS=darwin GOARCH=arm64 flick build --release
```

Что даёт target:

| Область | JS/Effect | Go |
|---|---|---|
| Исполнение эффектов | Effect fibers | goroutines + runtime Flick (`runtime-go/`) |
| Ошибки | Effect `Cause` | порт `Cause` с тем же деревом и рендером |
| Сервисы | `Context`/`Layer` | окружение + `Layer` с identity-sharing и memoization |
| `Ref` | ячейка в одном потоке | атомарный `get`/`set`/`modify` |
| Числа, строки | JS `Number`/`BigInt`, UTF-16 | эмуляция той же семантики (G3) |
| stdlib | вся | ядро, `Cli`, файлы/env/процессы, HTTP сервер и клиент, `Sql` (PostgreSQL), `Schema` |
| Только JS | — | `Ui`, `Atom`, `Action`, `Rpc`, `Otlp`, `DevTools`, JS FFI — диагностика G11 |

## 3. Треки

`G` — Go-backend, `N` — свой runtime + C/LLVM (условный). Внутри трека порядок = приоритет.

### Трек G — Go-backend

**G0. Решение в спецификации.** ✅ `1289392` (ключ — `[build] emit = "go"`; неизвестное значение уже отвергает разбор `Emit`)
- AC: `spec/01-goals.md` и `spec/02-decisions.md` (en + ru) описывают переносимое ядро и Effect как один backend; JS-числа и доступ к npm — свойства JS-target, а не языка.
- AC: спека фиксирует для обоих target: атомарный `Ref`, полный `Cause`, семантику `Layer`, cold `task`, structured concurrency; запись с причиной в `implementation-decisions.md`.
- AC: выбран ключ конфигурации (`[build] emit = "go"` или отдельный), неизвестное значение — диагностика.

**G1. IR без JS.** ✅ `603c392` (`Lit::Int`/`Float`/`BigInt` — значения, `-2147483648` — один литерал; JS печатает их из значения, `0xff` → `255`; обобщённый литерал вне `Int` — `T003`, раньше молча уходил в `from_int`; `foreign` помечен как JS-only; тест `ir_does_not_depend_on_a_backend`; e2e `tests/run/codegen/literals.fl`)
- AC: числовые литералы в IR хранят значение, а не JS-написание; режимы foreign-вызовов отделены от IR или помечены как JS-only.
- AC: `flick-core/src/ir` не зависит от `codegen/js` (проверка импортов в тесте); JS-вывод `tests/run` и снапшоты кодгена без изменений.

**G2. Каркас Go-кодгена, чистое подмножество.** ✅ `63a45c7` (`codegen/go` + `runtime-go/flickrt`; компилируется только достижимое из `main`, остальное — `P012` с названием конструкции или `foreign`; нет `go` или упал `go build` — `P029`; манифест `tests/run/portable.txt`, непомеченные случаи харнесс печатает как `skip go: … (not marked portable)`)
- AC: `codegen/go` выводит Go-модуль из checked IR; `go build` собирает его; `go` вызывается из `flick build`, его отсутствие — понятная диагностика.
- AC: функции, замыкания с захватом, каррирование и частичное применение, ADT/записи/кортежи, `match`, словари классов, `List`/`Option`/`Result`.
- AC: харнесс паритета: каждый `tests/run/**/*.fl`, помеченный переносимым, собирается обоими target, `.stdout` совпадает; непомеченные — пропуск с причиной, а не тихий успех.

**G3. Числа и строки.** ✅ `5ef5113` (строка — WTF-8 с подсчётом code units UTF-16; Go-runtime и свёртка констант проверяются одним корпусом вывода Node `runtime-go/flickrt/testdata/numbers.txt`; привязано ядро `String`, регистр, локали и `normalize` — в G10a; e2e `codegen/strings_utf16`)
- AC: `Int` — 32-битный wrap-around как в `codegen.md`, деление на ноль — `0`; `Float` — `float64`; `BigInt` — `math/big`.
- AC: `==` — SameValueZero (`NaN == NaN`, `0 == -0`); `show` для `Float` пишет число как JS `String` (`20.009999999999998`, `Infinity`, `NaN`), а не как `strconv`.
- AC: строковые операции считают UTF-16 code units как JS (`String.length`, индексы, срезы); тест на суррогатные пары; constant folding даёт одинаковый результат для обоих target.

**G4. Stack safety.** ✅ `d25d989` (анализ хвостовых вызовов вынесен в `codegen/tail.rs`, общий для обоих backend; самохвостовой вызов — `for`, взаимный — одна функция с `switch`; как и на JS, только чистые вызовы, остальная рекурсия — на стеке Go до 1 ГБ; e2e `codegen/deep_recursion` на глубине 10⁷; `008797e` — в Go циклом становится и хвостовой вызов, последняя стадия которого выполняет эффекты: на JS он не растит стек, но держит каждый шаг в куче, так что эффектная рекурсия там ограничена памятью, тест — 10⁶)
- AC: самохвостовая рекурсия — цикл; взаимная хвостовая — трамплин или эквивалент; тест глубины 10⁷ на обоих target.

**G5. Ядро эффектов.** ✅ `41eaad2`, `ce7e454`, `f173b02` (эффектный код передаёт `*flickrt.Fiber` первым параметром — полный вызов остаётся прямым вызовом Go, хвостовые циклы G4 сохраняются; `task` — `flickrt.Eff`, каждый `await` выполняет тело заново; ошибка — panic с `*flickrt.Cause` (fail/die/interrupt, sequential/parallel), `try` по правилам `cause.ts`, отчёт `main` как `conclude`; сервис — ключ `M.S`, поиск в окружении волокна при выполнении операции; `provide` с `Layer.succeed`/`merge`/`override`, остальной `Layer` — G8; e2e `codegen/{errors,services,task_twice,question,…}`, `bootstrap/app_fail`; рендер `Cause` сверен на переносимых случаях с ошибками, не на всём корпусе `tests/run`)
- AC: `task`/`Eff` — холодное значение: создание ничего не запускает, два `await` повторяют тело; Sync-вызовы и runtime-вызовы по стадиям IR.
- AC: сервисы — окружение исполнения; lookup происходит в момент, заданный семантикой, тест на `provide` внутри и снаружи `task`.
- AC: порт `Cause` (fail/die/interrupt, sequential/parallel) с рендером, совпадающим с `runtime/src/cause.ts` на корпусе ошибок `tests/run`.

Текущий Go ABI передаёт скрытый `*flickrt.Fiber` также чистым функциям, словарям, замыканиям и `pure`/`sync` Go FFI. `Fn`, `Poly`, `Apply` и `Inst` используют волокно вызова, а не создания функции; language-stage/effect bits не меняются, чистые хвостовые циклы остаются без новых точек прерывания. Это нужно для контекстного `show Url`, в том числе внутри сохранённых Combiner/Reducer/Order/EqHash callbacks; регрессия — `stdlib/http/redaction_context`.

**G6. Конкурентность и scopes.** ✅ `4f1c8ef`, `b70080a`, `fcf5b32`, `d2f2827`, `eaf267b`, `8c932cf`, `fd8aa60` (волокно — goroutine; `Fiber.spawn`/`join`/`await_`/`interrupt*`, fail-fast `parallel*` и конкурентный `Effect.all` оставляют причину первой ошибки, как Effect 4, сверено в Node; `race_*`, `race_all*`, `timeout*`; прерывание проверяется в `Run`, в блокирующих ожиданиях и в голове эффектного хвостового цикла; `uninterruptible(_mask)`, `ensuring`, `on_exit`/`on_interrupt`; owner закрывается как `owner.ts` и ждёт прерванные foreign-вызовы до финализаторов; значения `Exit` и `Cause`; `Ref` под mutex, `Deferred`, `Latch`, `Semaphore`, `PartitionedSemaphore`, `SynchronizedRef`/`SubscriptionRef` (без потока `changes`), `FiberSet`/`FiberMap`/`FiberHandle`, `cached*`, `ExecutionPlan`; программа даёт Go-сторону своих foreign-привязок файлом `x.go` рядом с `x.mjs`, `async` получает `context`, отменяемый прерыванием; e2e `runtime/{parallel,scope_close,scope_releases,scope_owners,handoff,exits,hung_io,concurrency,scopes,releases,owners}`, `stdlib/effect/{deferred,ref,fibers}`, `codegen/{release,for_loop}`; `go test -race` чистый. В тестах `deferred` и `fibers` порядок, который на JS даёт планировщик, сделан явным паузами — вывод на JS тот же. Scheduler на Go — непрозрачное значение: уступки никто не наблюдает)
- AC: fork/join, fail-fast parallel: ошибка ветви прерывает остальные, ждёт их завершения, ошибки cleanup попадают в `Cause`.
- AC: отмена через точки прерывания (каждый runtime-вызов и I/O); неотменяемые участки — явные; тест на зависший I/O, прерываемый таймаутом.
- AC: scope повторяет `runtime/src/owner.ts`: перестаёт принимать работу, останавливает и ждёт детей и supervisor-задачи, затем финализаторы в обратном порядке; финализатор не опережает ребёнка (тест).
- AC: `Ref` атомарен; `modify` из 1000 goroutines даёт точную сумму; `go test -race` по runtime чистый.
- AC: хвостовой вызов, ставший в Go циклом (G4), — точка прерывания: в голове цикла проверка отмены, иначе цикл без других runtime-вызовов нельзя прервать.

**G7. Вертикальный срез и go/no-go.** ✅ `15c3404`, `266d7cc`, `e2f66dd`, `9d5f335`, `eed264d`, `90c9a35` (срез на коде Flick — `runtime/slice`: ADT/match/замыкание/словарь, холодный `task`, `Ref`, разделяемый 50 ветвями `parallel_all`, хвостовая рекурсия на 10⁷, `parallel` с ошибкой ветви; файл в scope — `runtime/scoped_file`, сокет — слушающий сокет `Http.Server.serve` в `stdlib/http/serve`, оба освобождаются при успехе, ошибке и прерывании; `main` на Go обрабатывает SIGINT/SIGTERM с кодами 130/143, тест `flick/tests/go_main.rs`; запуск без Node — исполняемый файл)

Замеры: `just load-json go` и `just load-json js` — сервер `tools/load/json`, `GET /item/:id` декодирует хранимый JSON-документ производной схемой и отвечает JSON записи через `schema_json`; клиент — `tools/load/index.mjs` (Node, замкнутый цикл, медиана трёх прогонов по 10 с, затем 30 с при c = 64), старт — `tools/load/startup.mjs` (от запуска до первого ответа 200, медиана 10). Машина: Ryzen 9 9950X3D (16 ядер / 32 потока), WSL2, Node 24.21.0, Go 1.26.10, профиль release; на машине шла другая работа (load average ≈ 6,5), так что сравнивать стоит отношения.

| | JS-target (Node) | Go-target |
| - | -: | -: |
| req/s, c = 1 | 2 208 | 2 300 |
| req/s, c = 16 | 17 245 | 12 966 |
| req/s, c = 64 | 20 430 | 14 528 |
| req/s, 30 с при c = 64 | 20 981 | 12 569 |
| p50 / p99 при c = 64, мс | 2,44 / 12,41 | 3,73 / 15,67 |
| RSS до нагрузки / пик / после 30 с простоя, МиБ | 273 / 283 / 288 | 24 / 26 / 23 |
| старт до первого ответа, мс | 237 | 8,9 |
| что поставляется | 133 КБ сборки + `node_modules` (Effect, runtime) + Node | один исполняемый файл 9,1 МБ |
| ошибок | 0 из 1 981 723 | 0 из 1 394 204 |

Решение (2026-10-29): продолжаем G8+, условие N0 не выполнено. Go даёт в ~11 раз меньше памяти, старт в ~27 раз быстрее и один файл без Node, а нагрузку без ошибок держит с той же задержкой при одном клиенте; под конкурентной нагрузкой пропускная способность на ~30 % ниже, p99 выше на ~25 %. Это цена runtime без оптимизаций (боксинг `Value`, копии окружения сервисов и записей на запрос, разбор документа без компиляции схемы), а не потолок Go: профилирование и снижение аллокаций — отдельная задача перед G10c, по тем же `just load-json`.

Профилирование и снижение аллокаций (2026-10-29, `258ce24`, `4c1947e`, `3848c98`): запрос `load-json` в `BenchmarkLoadRequest` — 3,0 мкс, 71 аллокация и 2,9 КБ вместо 13,4 мкс, 322 и 22 КБ (запись строится на месте, порядок ключей схемы считается один раз, JSON ответа пишется сразу в буфер, на запрос нет горутины-наблюдателя и копии окружения сервисов), Int и Float в арифметике, сравнениях и хвостовых циклах — `int32`/`float64` Go по типам в IR, сборка мусора при 4× живой куче, если `GOGC` не задан. Таблица выше снята однопоточным клиентом, который сам упирается в ~20 тыс. req/s; теперь `load-json` гоняет клиент в 8 потоках (`--workers`). Та же машина (load average ≈ 3–14):

| | JS-target (Node) | Go-target |
| - | -: | -: |
| req/s, c = 1 | 2 268 | 2 449 |
| req/s, c = 16 | 20 681 | 28 714 |
| req/s, c = 64 | 35 783 | 49 908 |
| req/s, 30 с при c = 64 | 34 151 | 48 292 |
| p50 / p99 при c = 16, мс | 0,67 / 1,87 | 0,51 / 1,10 |
| p50 / p99 при c = 64, мс | 1,77 / 6,04 | 1,00 / 6,29 |
| RSS до нагрузки / пик / после 30 с простоя, МиБ | 311 / 313 / 313 | 36 / 39 / 34 |
| старт до первого ответа, мс | 226 | 10,3 |

Go под конкурентной нагрузкой на ~40 % быстрее Node при p99 не хуже и в ~8 раз меньшей памяти. Обработчик Flick теперь — около 16 % CPU сервера, остальное — `net/http`, планировщик и GC Go.

Разнесённые CPU (2026-10-29, `cad5b13`): профиль под нагрузкой показал, что при клиенте на той же машине ~35 % CPU сервера уходит на планировщик Go (32 P крадут работу друг у друга), ~19 % — на syscalls, а код Flick — ~17 %; результат зависел от того, сколько ядер занял клиент и соседние процессы. Теперь `just load-json` запускает сервер на `server_cpus` (по умолчанию 0–7, 4 ядра SMT), а клиент — на `client_cpus` (8–31), как в бенчмарках с отдельной машиной для нагрузки; Go сам берёт GOMAXPROCS из affinity. Запись небольшой ширины — одна аллокация вместо трёх, запрос держит волокно, состояние и `ServerRequest` в одной: 71 → 61 аллокация на запрос в `BenchmarkLoadRequest`. Та же машина (load average ≈ 9–20):

| | JS-target (Node) | Go-target |
| - | -: | -: |
| req/s, c = 1 | 2 330 | 1 965 |
| req/s, c = 16 | 18 461 | 29 796 |
| req/s, c = 64 | 24 769 | 61 572 |
| req/s, 30 с при c = 64 | 15 393 | 61 888 |
| p50 / p99 при c = 16, мс | 0,76 / 2,32 | 0,50 / 0,90 |
| p50 / p99 при c = 64, мс | 2,27 / 8,39 | 0,84 / 3,67 |
| RSS до нагрузки / пик / после 30 с простоя, МиБ | 313 / 317 / 317 | 28 / 29 / 28 |
| старт до первого ответа, мс | — | 9,8 |

Node на 8 потоках — один поток JS, так что с разнесением он упирается в одно ядро, а soak JS попал на всплеск фоновой нагрузки (load average 20). При клиенте на тех же CPU и load average 20–30 тот же Go-сервер давал 38–44 тыс. req/s и p99 7–9 мс: числа без разнесения мерят шум машины, а не runtime.

Микробенчмарки (2026-10-30, `just bench`): программы `tools/bench/*.fl` в профиле release на обеих целях — списки (200 тыс. `Int`: map, filter, fold, sort, reverse, zip, три раунда), `HashMap` на 10 тыс. строковых ключей (вставка, поиск, изменение, удаление, свёртка), строки (`++` по одному куску 20 тыс. раз, `String.join` 200 тыс. чисел, split, replace_all, to_upper) и JSON (5 тыс. записей производной схемы: кодирование, `Json.stringify`, `Json.parse`, декодирование, пять раундов). На Go нет часов, доступных программе, поэтому время — весь процесс от запуска до выхода, медиана 9 прогонов после разогрева; строка `startup` — пустая программа, её время входит в каждую строку. Обе цели печатают одну контрольную сумму, иначе прогон падает. Та же машина, Node 24.21.0, Go 1.26.10; на машине шли сборки других сессий (load average 19,9 в начале и 8,6 в конце прогона), так что сравнивать стоит порядки:

| | JS-target (Node), мс | Go-target, мс | Go / JS |
| - | -: | -: | -: |
| startup | 176 | 3 | 0,02 |
| hashmap | 241 | 847 | 3,51 |
| json | 312 | 47 | 0,15 |
| lists | 518 | 742 | 1,43 |
| strings | 198 | 148 | 0,75 |

За вычетом старта Node работа JSON на Go втрое быстрее, списки — вдвое медленнее, строки — в ~6 раз медленнее (больше половины этого — `++` по одному куску: Go копирует строку целиком, V8 склеивает rope), а `HashMap` — на порядок: таблица Go — отсортированный массив, который каждая вставка, изменение и удаление копирует целиком, так что n вставок стоят O(n²) (100 тыс. ключей — 100 с против 0,9 с на Node). Кандидаты на работу перед G10c: персистентная `HashMap` (HAMT, как в Effect) и построение строк без квадратичного копирования.

После HAMT (`f1e64a4`) и роста строки на месте при `++` (`84eecf6`), 2026-10-31, `just bench` с 9 прогонами на другой машине (Neoverse-N1, linux/arm64, Node 24.22.1, Go 1.26.10, load average 15–22), поэтому сравнивать стоит отношения:

| | JS-target (Node), мс | Go-target, мс | Go / JS |
| - | -: | -: | -: |
| startup | 541 | 7 | 0,01 |
| hashmap | 739 | 61 | 0,08 |
| json | 909 | 153 | 0,17 |
| lists | 1601 | 2069 | 1,29 |
| strings | 770 | 399 | 0,52 |

`HashMap` на Go теперь на порядок быстрее Node (было в 3,5 раза медленнее), строки — вдвое быстрее; медленнее Node остались только списки.
- AC: пункты §10 исследования на настоящем коде Flick: ADT/match/замыкание/словарь; cold `task`; scoped-файл и сокет с освобождением при успехе, ошибке и отмене; parallel с ошибкой ветви; разделяемый `Ref`; хвостовая рекурсия; запуск без Node.
- AC: замеры на одном прикладном сценарии (HTTP-сервер + JSON): RPS, p99, RSS, размер бинарника, время старта — против JS-target; таблица в этом файле.
- AC: записано решение: продолжаем G8+, или условие N0 выполнено.

**G8. `Layer` с паритетом.** ✅ `a79bdb8`, `f98f7b8` (слой — значение с идентичностью; memo map строит каждый слой один раз в собственный scope и держит его, пока открыт хоть один scope, который его запросил; `provide` форкает текущий memo map, так что вложенный `provide` делит построенное снаружи; комбинатор строит в scope, форкнутый от вызывающего, и закрывает его при ошибке; весь `Layer`, `Context` с references, спаны `Layer.Trace` и минимальный `Tracer`, `LayerMap`, `LayerRef`, `ErrorReporter`, слушатели `Events`; переносимы `stdlib/effect/{layer,layer_map}`. `merge`/`merge_all` строят слои конкурентно (первая ошибка прерывает остальные сборки) и освобождают их параллельно, `idleTimeToLive` у `LayerMap`/`LayerRef` держит ключ без держателей до истечения срока и отпускает оставшиеся при закрытии scope карты — регрессии в `layer_test.go`. Не портировано: спаны не экспортируются)
- AC: построение, identity-based sharing и memoization, scoped-слои и порядок освобождения как в Effect; `tests/run` по слоям переносимы.
- Регрессии `runtime-go/flickrt/layer_test.go` проверяют direct `fromBuild` и memoized слой: failing body после успешной сборки даёт ровно одну причину, body/build-fail с cleanup-die — ровно две в порядке исходная/cleanup. Layer-finalizers возвращают только новые cleanup-причины, но наблюдают исходный exit; освобождение выполняется один раз. Переносимый `stdlib/effect/layer` проверяет `Some (true, true, 2)`, а не только флаги. Это не закрывает перечисленные выше отличия конкурентного построения и TTL.

**G9. `Schema` и кодеки с паритетом.** ✅ `e2f66dd`, `b3e872b`, `7750636`…`c4236e4` (свидетели схем и refined-типы, канонический JSON-кодек с текстами issues JS-target, `Json.parse`/`stringify` с сообщениями V8; блоки `codec` и `Codec.json_schema` — ещё в G10c; политики блоков `option_as_null`/`option_as_nullish`/`on_missing`/`on_missing_effect`/`field_effect`, `Redacted.json_reveal`/`json_masked`, Standard Schema (`Codec.standard`/`standard_async`, проверка циклов во входе), `Schema.opaque`/`validate`/`chunk`/`decimal`/`duration`; модуль `Check` с ключевыми словами JSON Schema и комбинаторы кодеков Effect Schema; JSON-кодеки `Duration`, `BigDecimal`, `BigInt`, `Chunk`, `HashSet`/`HashMap`, `ByteSize`, `DateTime` с зонами, байтов и `Graph`; `deriving Url` и `Route.print`/`parse`; весь `Arbitrary` — порт движка Effect (`e469aff1`: seed, xoshiro128** на попытку, деревья shrinking, компилятор схем с ценой и рекурсией, числа/строки/шаблоны/BigInt/BigDecimal, replay), побайтно сверенный с Effect на Node (`testdata/arbitrary.txt`), и проверки `Check.apply`/`int`/`finite`/`between`/`min_length`/`max_length`/`pattern`, чьи ограничения читает генерация; диалекты `JsonSchema` (draft-04/07/2020-12, OpenAPI 3.0/3.1), `JsonPointer`/`JsonPatch`, `Differ`; весь `Net`. Переносимы все `tests/run/schema` и `stdlib/effect/{codec,schema,json_schema,schema_types,arbitrary}`. Не портированы: `deriving Form` (модуль `Form` на UI-атомах), генераторы `Arbitrary` для `Unknown`, `Duration`, `Chunk`, `HashSet`/`HashMap`, `DateTime` и отчёт `format_check_failure` через `Formatter.format` Effect (`Graph` целиком — в G10e). Расхождения помечены `shortcut:` в runtime)
- AC: `deriving Schema`, decode/encode, JSON-кодеки, тексты ошибок декодирования совпадают с JS-target; переносимы `tests/run/schema`.

**G10. stdlib.** ⬜ (после G7; каждый подпункт — переносимые `tests/run/stdlib/*`)
- **G10a.** ✅ `915e69d` `634ab28` `4de9b39` `873b8c4` `f2f4f75` Prelude, `List`, `Option`, `Result`, `String`, числа, `Seq`, коллекции, `Decimal`. (Ленивый `for` и `Seq`; `HashMap`/`HashSet` в порядке HAMT по `Hash.hash` из Effect, тест `stdlib/hash_order`; `BigDecimal`/`BigInt`/`Duration` по корпусу результатов Effect из Node; запись хранит порядок ключей JS; нормализация и регистр по таблицам из ICU Node (`tools/go-unicode/gen.mjs`); `RegExp` на RE2. Переносимы `stdlib/{seqs,collections,monoids,decimals,hash_order}` и `stdlib/effect/{array,option,number,string,order,combine,hash_map,hash_set}`. `stdlib/seq`, `stdlib/effect/{string_locale,record,duration,function}` тоже переносимы.)
- **G10b.** ✅ `bea06b9`…`b1a718d` `Cli` (блок `cli`, `Cli/Prompt`), файлы, env, процессы, сигналы и код выхода — аналог `Runner.Node`. Переносимы `stdlib/cli/*`, `stdlib/process/child_process` и платформенные кейсы ниже; оставшиеся расхождения помечены `shortcut:` в runtime.
  - `FileSystem.node`: бывшие runtime-заглушки `access`, `copy`, `copy_file`, `chmod`, `chown`, `glob`, `link`, `open`, `read_link`, `real_path`, `symlink`, `truncate`, `utimes` заменены реальными операциями; переносим `stdlib/effect/filesystem_node`. `open` scoped, POSIX `noFollow` атомарен; Windows возвращает для него штатный `BadArgument`, а `chown` сообщает ошибку Go вместо успешного no-op из Node. У copy проверяются same-file/descendant и ограничивается чтение размером источника; glob учитывает braces/extglobs, root/excludes, dot/symlink и платформенные правила регистра. Непортированные foreign-методы файловых дескрипторов отвергаются сборкой (`P012`), метаданные `stat` остаются частичными; весь G10b этим не закрыт.
    - Runtime-регрессии проходят на объявленном Go 1.23.0 и локальном Go; `print/graph` учитывают Cn через комплемент graphic/space-категорий, не полагаясь на более полную `unicode.C` новых Go. Минимум в `go.mod` и CI не повышен.
  - Платформа (`bea06b9`, `cc25917`, `650c7ec`, `0332a2c`, `0cae7fe`): `Path` (POSIX-пути Effect), `Stdio`, `Terminal`, `Console` как консоль Node (группы, счётчики, таймеры, `table`), `LogLevel`, `Config` и `ConfigProvider` целиком как в Effect (тексты `SchemaError`, env/.env/JSON/каталог, `layer_add`), `untagged`-типы в Go-кодгене, методы открытого файла `FileSystem` с собственной позицией и полный `stat` (dev, ino, nlink, uid/gid, blocks; birthtime на macOS и Windows), `Stream.Platform` (`read_file`, `stdin`, `watch` — опросом раз в 50 мс, в стандартной библиотеке Go нет уведомлений ФС), часть `Stream` и неограниченный `PubSub`. Переносимы `stdlib/effect/{platform,console,config,filesystem,stream_platform}`, `codegen/abi`.
  - Процессы: новый модуль `ChildProcess` на обоих target — обёртка `effect/process` (точка входа больше не исключена: манифест `stdlib/Effect/surface/process`, снимок пересобран). `Command` (`make`, `set_cwd`, `set_env`, `set_stdin`, `prefix`, `pipe_to`, `inherit_output`), `string`/`lines`/`exit_code`, `spawn` в scope с `stdout`/`stderr`/`wait`/`is_running`/`kill`/`kill_with`. JS — спавнер Node из `@effect/platform-node`; Go — `os/exec`, своя группа процессов (`Setpgid`; на Windows `CREATE_NEW_PROCESS_GROUP` и `taskkill /T`), закрытие scope и ненулевой выход добивают группу, как у Effect. Переносим `stdlib/process/child_process` (`sh` на POSIX, `cmd` на Windows). Расхождения: вывод копится в неограниченную очередь (Node приостанавливает непрочитанный поток), stderr не последней команды конвейера отбрасывается.
  - `Cli` — порт CLI Effect 4 в runtime, тексты побайтно как на JS: лексер и парсер (глобальные флаги где угодно, общие флаги родителя, `--no-`, `optional_value`, подсказки), параметры всех кратностей с fallback на `Config` и `Prompt`, `file`/`directory`/`path`/`file_text`/`file_parse`, тексты `CliError`, `HelpDoc` и форматтер Effect, `run_with` с настройками и форматтером Flick, `help_doc`, `display_width`, completions bash/zsh/fish/PowerShell; `deriving Cli.Value`. `Cli.Prompt` — все виды подсказок Effect на сервисе `Tty` (плюс минимальные `DateTime` и `Queue`). `--log-level` задаёт минимальный уровень обработчика (`Effect.log_*`, `Logger.with_loggers`), `--trace` отправляет span команды по OTLP/HTTP JSON, `--wizard` — порт `internal/wizard.ts` (Go-тест сверяет вывод со сценарием на Node). Переносимы `stdlib/cli/{cli,cli_exit,surface,prompt,trace}`. Из CLI на Go ничего не отсутствует; расхождения помечены `shortcut:`: под `--trace` экспортируется только span команды (без дочерних span, логов и метрик), INI/TOML не читаются, YAML — подмножество, ширина графем и emoji приближённая.
- **G10c.** ✅ HTTP: `Http.Server`, `Http.Router`, клиент; e2e `tests/run/stdlib/http` и `server`. (`9d5f335`, `250ac16`, `388ff0a`, `d18ddfd`, `20dd27b`, `9f8b4f5`, `8180f61`, `1b32868`, `3a521db`: заголовки, cookies, `Url` по URL Standard как в Node, `UrlParams`, `Mime`, `Etag`, `Http.Codecs`; запросы и ответы сервера с заголовками и cookies, роутер с настройками find-my-way (статичный сегмент сильнее параметра, `HEAD` → `GET`, префиксы), middleware, `codec`-блоки, `Logger`; клиент на `net/http` с фильтрами, retry, cookies-ref, редиректами, отменой запроса прерыванием; multipart с лимитами и ошибками Effect, файлы в scope, `StaticServer`, `TraceContext`, сервис `FileSystem`; потоки — минимальный pull-`Stream` с чанками, стриминг ответов сервера и тела клиента по требованию, multipart по частям, `compression_web`; `HttpApi` — серверы, типизированные клиенты, OpenAPI и `Codec.json_schema`, побайтно сверенные с JS. Переносимы `stdlib/http/{headers,values,serve,handlers,client,forms,types,server,streams,lifecycle,api,api_bodies,redaction_context,router_patterns}` и `stdlib/effect/schema_http`; последний записанный замер `load-json` — 61 аллокация, `just load-json go` после всего G10c: 63 014 req/s, p50 / p99 0,81 / 3,60 мс при c = 64, RSS пик 27,7 МиБ, старт 9,1 мс, исполняемый файл 9,6 МБ. Не портированы `stdlib/server/{server_fn,auth,origin}` — им нужен `Rpc` (`server fn`), и `stdlib/server/{trace,streamed}`, `ui/server` — `Ui`/`Atom`/`Otlp`: по G11 это js-only. Расхождения с JS помечены `shortcut:` в runtime: status text всегда стандартный (`net/http`), порядок заголовков запроса теряется, сжатие только gzip/deflate, нет логгера по умолчанию, потоковое тело запроса клиента собирается до отправки, foreign-методы файловых дескрипторов `FileSystem` и часть полей `stat` не поддержаны, Swagger UI/Scalar грузят бандл с CDN, а не встраивают его)
  - Router проверяет regex-ограничения и составные параметры до выбора handler, сохраняет имена параметров и приоритет узлов. Ограничения — RE2: lookaround/backreferences и смешанные regex/составные сегменты, требующие lookaround для многосимвольного разделителя, явно отвергаются при создании route, а не становятся неограниченным `:param`.
  - `Http.UrlParams.with_redacted_names` заменяет defaults в текущем fiber; вложенные scope восстанавливаются, конкурентные изолированы. `show Url` и URL в client/server errors учитывают настройку; ошибка хранит замаскированную строку после выхода из scope, исходный URL не меняется.
- **G10d.** ✅ `Sql` на PostgreSQL: запросы, транзакции, отмена; `tests/postgres` проходит на Go-target. (Драйвер — pgx v5.7.6, последняя версия с `go 1.23`; пакет `flickpg` регистрирует пул в `flickrt` и импортируется в `main.go` только программой, которая достигает `Database.pg`, — тогда `flick build` пишет `go.mod` с `require` и `go.sum`, а первый `go build` скачивает pgx в кэш модулей Go (без сети — понятная `P029`); остальные программы драйвер не несут и собираются офлайн. Шаблоны, привязка и коды значений — порт `runtime/src/sql`; прерванный запрос отменяется CancelRequest'ом (`CancelRequestContextWatcherHandler`) и соединение закрывается; транзакции с savepoint и `commit_unknown`; `statement_timeout`, тайм-аут захвата соединения. `flick/tests/sql_postgres.rs` гоняет `app` и `tx` на обоих target, Go-тест `runtime-go/flickpg` — против `FLICK_PG_TEST_URL`. Программа с `Database` — 11,1 МБ; release-`flick` 10,9 МБ — драйвер в нём только исходниками `flickpg`)
- **G10e.** 🔶 Остаток `stdlib/effect/*` на Go-target, по группам (кейсы, что есть в runtime, чего нет); приёмка — пустой `PENDING` теста `go_bindings_cover_builtins`:
  - Stream/Channel/Sink — `stream`, `stream_combine`, `stream_errors`, `stream_transform`, `stream_sink`, `channel*`, `sink`: ядро Pull/Channel/Stream/Sink как в Effect 4 (`0b78512c`: `pull.go`, `channel*.go`, `sink.go`, `stream.go`; HTTP, платформа и процессы — на нём; кейсы `channel_core`, `stream_core`); весь `Channel`, `ChannelSchema` и `Sink` (`9195d831`…`fee702dd`: кейсы `channel`, `channel_transform`, `channel_control`, `channel_schema`, `sink`, `sink_more`; части с Web Streams — JS-only `channel_web`); весь `Stream` и `SubscriptionRef.changes` (`54ada9ae`…`0624ff4d`: кейсы `stream`, `stream_transform`, `stream_errors`, `stream_combine`, `stream_sink`, порядок вывода в них не зависит от планировщика; части с Web Streams — JS-only `stream_web`). Все группы ✅.
  - Queue/PubSub — `queue` (и косвенно потоки): весь `Queue` как в Effect 4 (`19b5c6b0`: стратегии suspend/dropping/sliding, конец очереди, `take` — Pull; кейсы `queue`, `pubsub`); весь `PubSub` — ограниченные хабы, стратегии back-pressure/dropping/sliding, replay (`9ae18732`).
  - Tx*/STM — `txref`, `txqueue`, `txhashmap`, `txhashset`, `txchunk`, `txdeferred`, `txlock`, `txpubsub`: ✅ `f394997f`…`24882156` журнал попытки с проверкой версий при чтении (TL2), публикация под глобальным lock, `retry` ждёт записи затронутых ссылок с парковкой для `TestClock`; все `Tx*`, стресс-тест под `-race`; все восемь кейсов переносимы. `Stream.Platform.from_tx_subscription_ref` — со `Stream`.
  - Schedule — `schedule`, `cron`: весь `Schedule` Effect 4 (`4aac43c0`: один `step`, задержки, выходы, комбинаторы; HTTP retry на нём), включая `Schedule.cron` (`c547fc9c`).
  - Clock/Random/TestClock — `clock_random`, часть `effect_core`: `Clock` (`8364fdde`; через него `sleep`, тайм-ауты, TTL, логгер, `DateTime.now`) и `TestClock` (`48f711a3`: блокирующие ожидания паркуются, `Test.clock_test` на Go); `Random` — побитовый ISAAC Effect в окружении волокна (`d56f5c27`, кейс `clock_random`, jitter `Schedule` на нём).
  - DateTime/Cron — `datetime`, `cron`: ✅ `776f97cf`…`6f5f346d` `DateTime` на семантике JS `Date` (переходы времени, разрешение неоднозначного времени, `now` по часам волокна), зоны через `time.LoadLocation` с именами как в V8 — `time/tzdata` встраивает только программа, дошедшая до зон (+463 КБ); `format_intl` по таблицам 52 локалей из ICU Node (`tools/go-intl/gen.mjs`, генератор сверяет их с `Intl`); `Cron` с текстами `CronParseError`; переносимы `datetime`, `datetime_more`, `cron`, `cron_more`. Расхождения — `shortcut:` (legacy-форматы дат, Unicode-расширения тега, имена метазон в стилях `full`/`long`).
  - Logger/Metric/Tracer — `logger`, `metric`, `tracer`, `observe`: ✅ `3dfbca69`…`f62102c6` `Logger` (logfmt, structured, pretty, batch; логгеры по умолчанию как на JS), `Metric` с квантилями и гистограммами Effect, `Tracer` с атрибутами, ссылками и дочерними спанами, `References`, `PrometheusMetrics`; `--trace` отправляет все спаны команды, серверный спан продолжает трассу из заголовков запроса; переносимы `logger`, `metric`, `tracer`, `observe`, `observability/prometheus`. Не портирован экспорт логов и метрик по OTLP (`shortcut:` в `cli_settings.go`).
  - Ресурсы, кеши, запросы — `cache`, `request`, `runtime/leases`: ✅ `835c0640`…`f448f778` `Resource`, `ScopedRef`, `RcRef`, `RcMap`, `Pool`, `Cache`, `ScopedCache` (захват под маской захвата, TTL по часам волокна), `Request`/`RequestResolver` с порядком батчей Effect — его даёт очередь волокон до первой приостановки (`startup.go`), которую ждёт только `Request`.
  - Effect/Cause — `effect_core`, `effect_errors`, `exit_cause`: длинный хвост комбинаторов (`filter*`, `catch_cause*`, `match_cause*`, `validate`, `partition`, `Cause.pretty`, …).
  - Структуры — `chunk`, `graph`, `graph_algorithms`, `trie`, `hash_ring`, `cache`, `request`; `Crypto` — `crypto`; `MutableRef` — `mutable_ref`; `bytesize`: ✅ `89db7f60`…`bf5a9755` `HashRing`, `Crypto` (UUID v7 и ULID по часам волокна), `Trie`, `ByteSize`, `Chunk`, проверки `Check`, `Graph` с алгоритмами и текстами GraphViz/Mermaid; переносимы `hash_ring`, `crypto`, `trie`, `bytesize`, `chunk`, `check`, `graph`, `graph_algorithms`, `mutable_ref`; `cache` и `request` — в группе ресурсов.

**G11. Диагностика JS-only.** ✅ `957799c` (`FLICK-P030` уже в `flick check` при `emit = "go"`: импорт `Ui`, `Atom`, `Reactivity.Atom`, `Action`, `Rpc`, `Otlp`, `DevTools`, `Js` и модулей под ними, runner `Runner.Node`/`Browser`/`Cloudflare`; `.iface` получил поле `targets` (`["js"]` у всего, что пишет `flick build`; без версии схемы — поле необязательное), пакетный модуль без `go` в `targets` — тоже P030. Фикстура — проект `tests/check/projects/go_js_only`: у `tests/check/fail` нет `flick.toml`, а target задаёт он. Локальный `foreign` из `.mjs` без `.go` рядом по-прежнему `P012` при сборке: он виден только по достижимому коду)
- AC: импорт `Ui`, `Atom`, `Action`, `Rpc`, `Otlp`, `DevTools`, JS FFI или runner'а `Node`/`Browser`/`Cloudflare` при `emit = "go"` — новый код ошибки с указанием модуля и target; фикстура в `tests/check/fail`.
- AC: в `.iface` модуль отмечает поддерживаемые target; пакет без Go-сборки в Go-приложении — диагностика.

**G12. Платформы.** 🔶 `b1436c9` (CI: job `go` — матрица linux/amd64, linux/arm64, darwin/arm64, windows/amd64: `go vet`/`go test` runtime и переносимые кейсы харнесса без Node, `FLICK_E2E_GO_ONLY`; сборка без cgo — `CGO_ENABLED=0`, если не задан; `.exe` по целевому `GOOS`; ошибки файлов на windows сопоставляются Node по смыслу (`fs.ErrNotExist` → `ENOENT`), а не по errno — запуск `.exe` на настоящем Windows показал `ENOTDIR` вместо `ENOENT`; e2e `runtime/paths`; Go-тест на windows: Ctrl+Break закрывает scope `main` и даёт код 130. Не проверено: матрица ни разу не прошла в CI — у аккаунта исчерпан бюджет GitHub Actions, джобы не стартуют; darwin и arm64 — только `go vet` кросс-сборкой. Процессы на windows — `ChildProcess` (G10b) и Go-тест `runtime-go/flickrt/process_test.go` в той же матрице; на настоящем Windows не запускался. Сборка всех `GOOS`×`GOARCH` из `go tool dist list` не делается — по решению только эти четыре платформы)
- AC: CI собирает `GOOS`×`GOARCH` из всех платформ, которые даёт Go (`go tool dist list`, кроме `js/wasm`/`wasip1`); прогон харнесса паритета на linux/amd64, linux/arm64, darwin/arm64, windows/amd64.
- AC: файловые пути, сигналы и процессы на windows — явные тесты, а не только сборка.

**G13. Инструменты.** ✅
- AC: `//line`-директивы: паника и стек указывают в `.fl`; `flick run` и `flick test` работают с Go-target. (✅ `78d5a25`, `4572995`: `//line` и строки `.fl` в стеке дефекта; `flick run` и `flick dev` собирают исполняемый файл в поколение запуска и запускают его, `dev` пересобирает и перезапускает по изменению; `flick test` собирает один `flick-test`, который гоняет `tests` всех тестовых модулей — `RunTests` в runtime с опциями, seed (mulberry32), property-тестами и выводом node-раннера, побайтно сверено на human и json; тест `flick/tests/go_main.rs`. Не портирован `clock_test`: в Go-runtime нет виртуального времени)
- AC: решено и записано: требуется установленный Go или toolchain поставляется с `flick`. (✅ как Gleam с Erlang/Node: `flick` toolchain не поставляет, для сборки нужен Go 1.23+ в `PATH` (`P029`), собранному файлу не нужно ничего)

### Трек N — свой runtime + C/LLVM ⏸

Условие N0: по итогам G7 Go не годится (размещение данных, GC, размер или латентность не устраивают по замерам) или появилось требование независимости от Go runtime.

**N1. Runtime на Rust с C ABI.** ⏸
- AC: значения (tagged `Value`), готовый GC, задачи, scheduler, I/O, таймеры; семантика G5–G6 и тесты те же.
- AC: решение о `unsafe_code = "forbid"` для runtime-crate записано.

**N2. Генерация C через Clang.** ⏸
- AC: тот же харнесс паритета, что у G2; executable без Go и Node.

**N3. Прямой LLVM — только по замерам.** ⏸
- AC: сравнение с N2 на сценарии G7; без выигрыша — не делаем.

## 4. Порядок и зависимости

```
G0 ✅ ─ G1 ✅ ─ G2 ✅ ─ G3 ✅ ─ G4 ✅
               └─ G5 ─ G6 ─ G7 ─┬─ G8 ─ G9 ─ G10a ─ G10b ─ G10c ─ G10d
                                │   G11 (после G2, ширится с G10)
                                │   G12, G13 (после G7)
                                └─ N0? ─ N1 ─ N2 ─ N3
```

Вехи:
1. «Чистый Flick в Go» — G0–G4.
2. «Срез runtime» — G5–G7, решение go/no-go.
3. «Паритет ядра» — G8, G9, G10a, G11.
4. «CLI и серверы» — G10b–G10d, G12, G13.

## 5. Риски

1. **Полный паритет дорог** (§6.4–6.6): `Cause`, `Layer` и `Schema` — большие поверхности Effect, их поведение придётся портировать и сверять. Митигация: харнесс паритета G2 с первого дня; G8/G9 только после G7.
2. **Семантика JS-чисел и UTF-16 строк в Go** медленнее нативной. Митигация: замеры G7; переход к нативной семантике — отдельное решение спеки, не тихая подмена.
3. **Boxing и словари** дают оверхед против V8. Митигация: специализация из `opt/specialize` только по замерам G7.
4. **Отмена в Go кооперативная**: блокирующий I/O без поддержки отмены зависнет. Митигация: G6 — точки прерывания и тест на I/O с таймаутом.
5. **Гонки в runtime** при настоящем параллелизме. Митигация: атомарный `Ref` (§6.3), `go test -race` в CI.
6. **Матрица платформ** (§6.7) увеличивает стоимость CI и поддержки windows. Митигация: собираем всё, прогоняем четыре основных платформы.
7. **Расхождение двух backend со временем.** Митигация: новый тест в `tests/run` по умолчанию переносимый; исключение — с причиной.

## 6. Решения (приняты 2026-10-28)

1. **Отдельный роадмап**, [alternative-backend.md](alternative-backend.md) остаётся исследованием-основанием.
2. **Target — Go;** свой runtime + C/LLVM — условный трек N после G7.
3. **Один язык и репозиторий:** Flick определяет свою семантику исполнения, Effect — один из backend; спека пересматривается в G0.
4. **`Ref` атомарный**, без новых проверок в типах.
5. **Полный `Cause`** как в Effect, на обоих target.
6. **`Schema` и `Layer` с паритетом** Effect в первом полезном Go-target.
7. **Платформы — всё, что даёт Go.**
8. **stdlib первого target:** ядро, `Cli`, файлы/процессы, HTTP, `Sql` (PostgreSQL); остальное — JS-only с диагностикой.

Решения 5–6 строже рекомендации исследования (там — небольшой контракт ошибок и упрощённый `Layer`); оценки масштаба из его §9 их не учитывают.
