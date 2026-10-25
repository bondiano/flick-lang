# Роадмап: реактивный UI, fullstack на Effect, CLI, observability, бандл

Статус: рабочий, обновлён 2026-10-25 (решения §6 приняты, статусы сверены с историей `hetzner/flick-lang` до `d524e92`).
Язык не в релизе, поэтому языковые изменения допустимы и deprecated-релизов не делаем.

Обозначения: ✅ сделано (коммит), 🔶 частично / в работе, ⬜ не начато, 🆕 добавлено при обновлении 2026-10-25.

Основания — исследования в этой папке (все утверждения там с цитатами на первоисточники):

| Файл | О чём |
|---|---|
| [reatom-v1001.md](reatom-v1001.md) | Reatom v1001 (commit `aee4f572`): атомы, фреймы, `wrap`, abort, расширения, React, формы, логгер, подводные камни; §8 — 17 идей для Flick |
| [effect-v4-reactivity.md](effect-v4-reactivity.md) | `effect/reactivity` (Atom, AtomRegistry, Hydration, AsyncResult, AtomRpc, AtomHttpApi), `@effect/atom-react`, rpc/http-api, observability, cli; §9 — 16 инвариантов для компилятора |
| [typed-ui-prior-art.md](typed-ui-prior-art.md) | PureScript react-basic-hooks / halogen-hooks, Ur/Web, Koka, Scala CC, Effekt, Bonsai, Leptos, Lustre, ReScript, Feliz; §4 — синтез гарантий |
| [flick-internals-for-ui.md](flick-internals-for-ui.md) | как Flick компилирует эффекты, E001, регионы, имена, Schema, сервер, CLI, тесты, ограничения |
| [rescript-components.md](rescript-components.md) | ReScript JSX v4: `@react.component`, `componentWithProps`, свой JSX-модуль, memo, типы DOM и событий, ошибки props; §7–8 — что переносим в `component`/`setup`/`render` |
| [bundle-and-output.md](bundle-and-output.md) | замеры бандла (rolldown), разбор выходного JS против ReScript; §4 — 8 улучшений с AC |

Первоисточники верхнего уровня: <https://github.com/reatom/reatom/tree/v1001>, <https://v1001.reatom.dev/reference/react>,
<https://effect.website/docs/v4/api/atom-react>, `effect@4.0.5` (`dist/reactivity/*.d.ts`), <https://github.com/megamaddu/purescript-react-basic-hooks>.

## 1. Цели и принципы

1. **Модель — атомы, не TEA.** Компонент подписан на атомы, прочитанные при рендере (Reatom `reatomComponent`, Effect `useAtomValue`); ререндерится только он. TEA при желании — библиотека поверх (один атом-модель + action `update`).
2. **Реализация — Effect, а не своё ядро.** `effect/reactivity` даёт реестр, трекинг, family, TTL, гидрацию, AtomRpc/AtomHttpApi ([effect §1–5](effect-v4-reactivity.md)). Всё `@stability unstable` в 4.0.5 — оборачиваем как unstable. Контакт с Effect держим в одном слое (U9), чтобы замена реестра не трогала язык.
3. **Что Reatom/Effect проверяют в рантайме или оставляют пользователю, Flick проверяет типами.** Чтение/запись атомов — сервисы `Atom.Read`/`Atom.Write` в строке; время жизни — регионы и `^c`; сериализуемость — кодеки/`Schema`; клиент/сервер — сервис в строке эффектов ([prior-art §4](typed-ui-prior-art.md), [effect §9](effect-v4-reactivity.md)).
4. **Имена даёт компилятор.** `Mod.binding` → `displayName` компонента, метка атома, имя span'а action. Строки руками не пишутся.
5. **Бандл — продуктовая метрика.** Каждое изменение рантайма/кодгена проверяется бюджетом в CI (`just bundle-size`, трек B).
6. **Настройки — явные.** Компромиссы вес/отладка выносятся в `[build]` с разумным умолчанием, а не зашиваются.

## 2. Целевое API

Сейчас (✅, `c0c5fe3`…`d37114c`):

```flick
count : Writable Number                    // метка "App.Counter.count"
let count = Atom.make 0

increment : Action.Action ClickSnapshot {Atom.Write} {}
let increment = Action.exhaust (fun _ -> task { Atom.update count (fun n -> n + 1) })

counter_view = Ui.view (fun () ->          // компонент "App.Counter.counter_view" в DevTools
  Ui.button { on_click = increment } [Ui.text (show (Atom.value count))])
```

Цель C7 — компонент с локальным состоянием, ключевые слова (решение §6.1):

```flick
component search (props : SearchProps) =
  setup                                    // один раз на экземпляр, ->{Setup s}
    let query = Atom.local ""              // не покинет экземпляр (G001)
    let found = Atom.task (Api.find (Atom.get query))
    let edit = Action.latest (fun e -> task { Atom.set query e.value })
  render                                   // ->{Atom.Read}; unmount закрывает регион s
    ui { … }
```

Эффекты/сервисы:

| Эффект | Где разрешён | Даёт | Статус |
|---|---|---|---|
| `Atom.Read` | render, `Atom.derived` | `Atom.get`/`value` с трекингом | ✅ |
| `Atom.Write` | `Action`, задачи, setup | `set`, `update`, `modify`, `refresh` | ✅ `d37114c` |
| `Setup s` | `setup` компонента | `Atom.local`, локальные action, финализаторы региона `s` | ⬜ C7 |
| `Server` | серверный entry, `server fn` | request/cookies/DB-слои | ⬜ F1 |

## 3. Треки

Нумерация: `S` — stdlib surface, `B` — бандл и выходной JS, `C` — компилятор/язык, `U` — UI-ядро, `R` — SSR, `F` — fullstack, `O` — observability, `L` — CLI. Внутри трека порядок = приоритет.

### Трек S — снять исключения Effect-entrypoint'ов

**S1. `effect/reactivity`.** ✅ `6445878`
- AC: `"effect/reactivity" = { modules = "reactivity" }`; каждый экспорт классифицирован `[wrapped]`/`[excluded]` с причиной; `*Unsafe`, guards, TS-only исключены; линтер покрытия зелёный; модули помечены unstable.

**S2. `effect/rpc`.** ✅ (модуль `Rpc`: процедуры с кодеками, группа как запись обработчиков, `listen` по HTTP + NDJSON, `call`/`call_stream`; 169 экспортов классифицированы; + клиент без Node-сервера, сериализации `ndjson`/`json`/`json_rpc`/`ndjson_rpc`/`binary`, WebSocket: `listen_websocket`, `Rpc.websocket`, `connect`/`call_on`; обработчик для `Http.Router` ждёт захватов в `Http.Router.route` — S2b)
- AC: `Rpc`, `RpcGroup`, `RpcClient`, `RpcServer`, `RpcSerialization` классифицированы; run-тест в `tests/run/stdlib/rpc/`: сервер и клиент на одном `RpcGroup`, unary и stream вызов; линтер покрытия зелёный.

**S2b. 🆕 RPC в маршрутизаторе Flick.** ⬜
- AC: `Rpc.handler`/`handler_websocket` монтируются в `Http.Router.route`; для этого `route`/`router` принимают обработчик с захватом региона сервера (`^s`), фикстура G001 на утечку; тест: группа на `Http.Server.serve` рядом с обычными маршрутами.

**S3. `effect/observability`.** ✅ (модули `Otlp` — `layer`/`traces`/`logs`/`metrics` с `{ url; service_name }`, HTTP-клиент и JSON внутри — и `Prometheus.format`)
- AC: обёрнуты `Otlp`, `OtlpTracer`, `OtlpLogger`, `OtlpMetrics`, `PrometheusMetrics`; run-тест экспортирует span в локальный HTTP-приёмник и проверяет имя и атрибуты.

**S4. `effect/devtools`.** ✅ (`DevTools.layer url`; сервер для тестов — `@flick-lang/runtime/effect/devtools/DevToolsServer` на `@effect/platform-node`)
- AC: обёрнут `DevTools.layer` (по умолчанию `ws://localhost:34437`); смоук-тест подключения к фейковому ws-серверу.

**S5. Пересмотр решения о UI.** ✅ `8cae598`
- AC: строка UI в `spec/02-decisions.md` описывает атомы, action с политикой и реестр на запрос; запись с причиной в `implementation-decisions.md`; ru синхронизирована.

### Трек B — бандл и читаемость выходного JS

Базовая линия: pure 239.3 / counter 309.5 / playground 315.9 KB gz. После B0–B9: pure 10.8, counter 80.7, playground 90.7. Сейчас (с атомами и гидрацией): counter 97.4 (бюджет 99), playground 105.3 (бюджет 110), email 30.4 (бюджет 32).

**B0. Бюджет в CI.** ✅ `56fea79`, `d931dfe`
- AC: `just bundle-size` собирает pure/counter/playground/email в release, падает при превышении бюджета и если release больше debug; печатает таблицу.

**B1. Убрать `import * as effect` из runner'а.** ✅ `26e8137`
- AC: counter ≤ 97 KB, pure ≤ 26 KB gz; нет лишних модулей Effect; проверка версии сохранена.

**B2. Рантайм без побочных эффектов.** ✅ `758fbf7`, `0facec9`
- AC: `"sideEffects": false`, `/*#__PURE__*/` на top-level; бюджеты counter 81, pure 12, playground 91 KB (до атомов); импорт только `Effect` не тянет `Schema`.

**B3. Чистый выход компилятора.** ✅ `b0b19a0`
- AC: сгенерированный `package.json` с `"sideEffects"`; `/*#__PURE__*/` на вызовах/инстансах/константах; неиспользуемый `deriving Show, Eq` = 0 байт (node-тест).

**B4. Нет `Effect.gen` вокруг одиночного хвостового эффекта.** ✅ `1dde2ae`
- AC: 0 таких генераторов в release playground; e2e корпус зелёный в обоих профилях.

**B5. Вызовы с учётом арности между модулями.** ✅ `d931dfe`, `e649fac`
- AC: `)(`-цепочек в release playground ≤ 124 (стало 6); release ≤ debug; Vite-плагин собирает `build` в release; все `tests/run` проходят в release.

**B6. Тип-направленный `==` и известные инстансы.** ✅ `9458220`
- AC: нет `$rt.equals` для `String`/`Bool`/`BigInt`/ADT без полей; полные применения `foreign pure` вызывают импорт напрямую.

**B7. Читаемые имена инстансов.** ✅ `a7408ea`
- AC: `$i_Show_Number` без хэша при однозначной голове; иначе 8-hex суффикс; решение записано в `implementation-decisions.md`.

**B7b. 🆕 Хэши тегов конструкторов — замер и решение (§6.5).** ✅ (−138 B gz на playground < 1 KB: хэш остаётся; замер в `bundle-and-output.md` §4a)
- AC: `just bundle-size` печатает (или разовый скрипт фиксирует в `bundle-and-output.md`) разницу gzip playground/counter между тегами с хэшем и без.
- AC: если выигрыш < 1 KB gz на playground — тег остаётся `Name#hash` везде, в `implementation-decisions.md` (en + ru) дописан замер и «вопрос закрыт».
- AC: иначе — голое имя, когда оно уникально во всей сборке, хэш при коллизии; типы с `deriving Schema`/кодеком и экспортируемые из библиотеки (`kind = "lib"`) всегда с хэшем; тест: два модуля с `NotFound` получают разные теги; гидрация/сериализация между debug и release совпадает (e2e).

**B8. Match и конструкторы.** ✅ `a975363`
- AC: последняя ветка не проверяется; ≥3 веток → `switch (x._tag)`; полный конструктор — литерал объекта.

**B9. Мелочи.** ✅ `d605edd`
- AC: в `*.js.map` нет `file://`; pure-программа пишет ≤ 5 файлов `_flick` (сейчас 3 встроенных модуля).

### Трек C — язык и компилятор

**C1. Стадия `Sync`.** ⬜ (рендер сейчас идёт через `runSync` в `runtime/src/atoms.ts`)
- AC: `Param.runtime: bool` → `Stage { Pure, Sync, Runtime }`; render-тело с `{Atom.Read}` компилируется без `Effect.gen` и без `runSync`, `Atom.get` — прямой вызов.
- AC: check-фикстуры: `{Atom.Read}` передаётся в параметр `->{r; e}` с автоматическим lift.
- AC: бенчмарк рендера (до/после) в `runtime/test` или `tools/`; если выигрыш < 10 % и бандл не меньше — пункт закрывается записью в `implementation-decisions.md` без реализации.
- AC: spec 03/04 и `02-decisions.md` обновлены (en + ru), если реализовано.

**C2. `Read`/`Write` и правило рендера.** ✅ `869dcf0`, `d37114c`
- AC: `Atom.get` требует `Atom.Read`, запись — `Atom.Write`; render и `Atom.derived` не могут писать, использовать другие сервисы или падать — `FLICK-U004` с подсказкой (action / `Atom.task` / `Ui.suspended`).

**C3. Атомы не генерализуются.** ✅ `3215332`
- AC: top-level не-функция с атомом полиморфного типа — `FLICK-U005` «укажите тип»; функция, создающая атом на вызов, остаётся полиморфной.

**C4. Стабильная идентичность.** ✅ `bb3ab0e`, `d524e92`, `35264ca`
- AC: `Atom.make`/makers/`Action.*`/`Ui.view` в render — `FLICK-U006`, включая пользовательские функции-makers и импортированные (через `.iface`); ключ `Atom.family` требует `Hash`; атомы family освобождаются (`Atom.forget`, weak).

**C5. Захваты атомов.** ⬜ (после C7)
- AC: `Atom.local` в `Setup s` захватывает `^s` и освобождается с экземпляром; фикстура: атом, захватывающий `File s` из `scope`, не может стать top-level (G001); top-level сигнатуры требуют явных аннотаций захватов.

**C6. Метки от компилятора.** ✅ `e5ef204` (умолчание меняется в C6b)
- AC: каждый top-level атом, action и view — `$rt.label("Mod.name", …)`; ручное имя сохраняется; `[build] labels = "all" | "components" | "none"`; source map заполняет `names`.

**C6b. 🆕 Метки по умолчанию `all` (§6.4).** ✅ (замер: метки атомов стоят 0–7 B gzip; `withLabel` со стектрейсом ≈16 мкс на атом в Node — оставлен)
- AC: `Labels::default()` = `All` (`flick-core/src/project/config.rs`), тест конфигурации и `reference/project.md` (en + ru) обновлены; `components` и `none` остаются.
- AC: `just bundle-size` собирает playground и counter также с `labels = "components"` и печатает разницу; бюджеты пересчитаны под `all` и записаны с замером в коммите.
- AC: стоимость метки атома (stack trace при создании) в release измерена; если заметна в профиле — в `all` release метка без стека.

**C7. Компоненты: ключевые слова `component` / `setup` / `render` (§6.1).** ⬜
- **C7a. 🆕 Разбор ReScript.** ✅ [rescript-components.md](rescript-components.md): props — одна явная запись (как `@react.componentWithProps`), `key` — метаданные узла, метка на итоговом объекте React, адаптер = хост-примитивы + монтирование с проверяемым интерфейсом; не переносим props из аргументов, `make`/модуль на компонент, выбор рендерера в конфиге, `forwardRef`. AC: записка `docs/research/rescript-components.md` с цитатами: `@react.component` и генерация типа props, `@react.componentWithProps`, generic JSX transform (`"jsx": {"module": …}` — свой JSX-модуль вместо React), `React.memo`/`memoCustomCompareProps`, как это сказывается на выходном JS и ошибках; вывод — что переносим в Flick (кастомизация рендерера/адаптера, props-тип, memo) и что нет.
- **C7b. Синтаксис.** AC: парсер/AST/`lower` для `component name (props : P) = setup … render …` (setup необязателен); `flick fmt` идемпотентен на фикстурах; tree-sitter-грамматика в `editors/` и семантические токены LSP знают ключевые слова; ошибки разбора по-человечески («`render` должен идти после `setup`»).
- **C7c. Типы.** AC: `component` desugar'ится в `forall s. p ->{Setup s | r; e} (p ->{Atom.Read} Ui)^{s}` по образцу `Layer.from_build` (`Layer.fl:188`); setup не может быть условным по построению; `Atom.local` и `Action.*` разрешены в setup; hover LSP показывает тип экземпляра и регион.
- **C7d. Рантайм.** AC: setup выполняется ровно один раз на смонтированный экземпляр, в т.ч. под StrictMode (тест в `runtime/test`), повторный mount не утекает fiber'ами; action экземпляра прерываются и ожидаются при unmount (перенесено из U1/U2); компонент получает метку C6.
- AC (всё C7): `Ui.view` либо выражается через `component` без setup, либо удаляется; плейграунд использует `component` хотя бы для одного виджета с локальным состоянием; `reference/ui.md` (en + ru).

### Трек U — реактивное UI-ядро

**U1. Обёртки и React-биндинг.** ✅ `c0c5fe3`, `48d453e`, `e5ef204`
- AC: компонент перерисовывается только при изменении прочитанных атомов (тест считает рендеры соседей); имена `App.Counter.counter_view` видны в DevTools; программа без views не тянет их код. StrictMode/setup-once — в C7d.

**U2. Actions и политики.** ✅ для корня `c76a67e`; экземпляры — C7d
- AC: `Action.exhaust/latest/queue/concurrent` делят состояние политики на корень; корень прерывает и ждёт задачи при unmount; тесты политик сохранены.

**U3. Обработчики без замыканий в render.** ✅ `e727df8`
- AC: `on_click = increment` и `Action.with remove item` дают общее состояние политики; лямбда-обработчик в render — предупреждение `FLICK-U002`.

**U4. Контролируемые поля.** ✅ `0ac9988`
- AC: `Ui.field`/`field_area` пишут состояние синхронно в событии; тесты caret, IME composition, внешнего сброса.

**U5. Типизированные host-атрибуты.** ✅ `0326a1c`, `dd0ab4c`
- AC: `Ui.Host` с типизированными `id`, `role`, `aria_*`, `data`, `tab_index`; `Ui.heading_at H1…H6`; `Dom.attributes` плейграунда удалён.

**U6. Async в render — opt-in Suspense (§6.3).** ✅ `e8324e1`
- AC: `Atom.task` даёт `Atom.Async a e` (`Loading`, `Loaded a`, `Failed (Failure e)`), match исчерпывающий; `Ui.suspended failed fallback render` + `Atom.ready` — opt-in, каждая ошибка обязана иметь место (`failed`); `Atom.ready` вне `Ui.suspended` — дефект с подсказкой.

**U6b. 🆕 Хелпер «как Suspense», без смены умолчания.** ⬜
- AC: однострочная форма для простых экранов, например `Ui.awaiting task (fun value -> ui)` / `Ui.suspended_with default_failed fallback render`, сводится к `Ui.suspended` и не вводит новый механизм; пример в `reference/ui.md` сравнивает с ручным `match`.
- AC: тип по-прежнему требует обработчик ошибки `e` (или `e = Never`); тест рантайма: loading → loaded → failed.
- AC: никакой неявной приостановки: `Atom.get` на task-атоме по-прежнему даёт `Atom.Async`.

**U7. Миграция и удаление старого (§6.2).** ✅ `5daf143` + U7b
- ✅ AC: плейграунд и docs-header на атомах; `Ui.Dom.run`, `Ui.run_with`, `Ui.lazy_view(_with)` удалены; `ui.test.ts` разбит по темам; `reference/ui.md` и ru переписаны.
- **U7b. 🆕 `Ui r e` → `Ui`.** ✅ (строка обработчика фиксирована: `{Atom.Write}`, без ошибок; `mount` требует `Atom.Write`, супервизор получает `Failure none`; `FLICK-U001` проверяет и `Action.queue/concurrent`) AC: после U2/U3 параметры `r e` у `Ui` ничего не несут (обработчики — action со своей строкой) — тип схлопнут до `Ui` во всех builtins, тестах, docs и плейграунде; если какой-то параметр всё же нужен — записка с причиной в `implementation-decisions.md`, и он остаётся один. Deprecated-алиасов не оставляем.

**U8. Бюджет.** ✅ `5daf143`, `20edc6a`
- AC: counter на атомах ≤ 99 KB (B2 + 18), playground ≤ 110 KB gz; превышение валит `just bundle-size`.

**U9. 🆕 Граница с `effect/reactivity` (§6.6, план отхода).** ✅ (рантайм и запись сделаны; пункт про `Atom.Registry` для пользователей не выполнен: `type alias` сервиса компилятор не поддерживает — сервис в строке остаётся `Reactivity.Atom.Registry`, отдельный пункт U9b)
- AC: импорты `effect/reactivity/*` в `runtime/src` только в `atoms.ts` и `runtime/src/effect/reactivity/*`; `ui/index.ts`, `ui/server.ts`, `ui/react/*` ходят через них (сейчас импортируют `Hydration`, `Atom`, `AsyncResult` напрямую); проверка — тест/oxlint-правило `no-restricted-imports`.
- AC: пользовательский код и плейграунд используют `Atom.*`/`Ui.*` (`Atom.layer`), а не `Reactivity.*`; `Reactivity.*` в `reference` помечен как низкоуровневый unstable.
- AC: в `implementation-decisions.md` записано: остаёмся на `effect/reactivity`; условие пересмотра — бюджет U8 или ломающее изменение в Effect; замена реестра трогает только этот слой.

**U9b. 🆕 Пользовательские сигнатуры без `Reactivity.*`.** ✅ (псевдонимы сервисов и ошибок раскрываются в строках эффектов; `Atom.Registry`; реэкспорт — U9c)
- AC: пользователь пишет `{Atom.Write}` или `Atom.Registry` в строке эффектов вместо `Reactivity.Atom.Registry`: либо компилятор поддерживает псевдоним сервиса, либо `Ui.Dom.mount`/`Ui.Server.*` берут реестр из `Atom.Write` (тот же реестр в `Atom.layer`); фикстуры `tests/check/pass/ui/views.fl`, `tests/run/ui/serializable.fl` и `reference/ui.md` без `Reactivity.`.

**U9c. 🆕 Реэкспорт модулей.** ✅ (синтаксис `exposing (.., X)` уже был; исправлены квалифицированные пути к сервисам `M.S.op`, `M.S.Impl`, `M.S` как значение; `Atom.Registry` — реэкспорт; реэкспортированный `main`/`tests` не делает модуль входом — пересмотреть в L6)
- AC: модуль делает импортированную сущность (тип, ADT с конструкторами, сервис с операциями и `Impl`, значение) доступной по своему пути как ту же сущность; `Atom.Registry` — реэкспорт `Reactivity.Atom.Registry`, так что работают `Atom.Registry.get`, `Atom.Registry.Impl` и `Layer.succeed Atom.Registry`; `.iface`, кодген, LSP (goto/hover/completion) и `flick diff` знают реэкспорт; цикл реэкспорта — ошибка.

### Трек R — SSR и шаблоны

**R1. Реестр на запрос.** ✅ `aa0c2b5`, `4100738`
- AC: `Ui.Server.render` читает атомы из провайдера реестра; параллельные запросы изолированы; рендер после `provide` — ошибка типов; `Ui.Server` — отдельный entry, клиент не тянет `react-dom/server`.

**R2. Сериализуемые атомы.** ✅ `a3c8830`
- AC: `Atom.serializable wire state` с синхронным `PureCodec`; ключ `Module.name` (+ `/key` для family) даёт компилятор; неоднозначное использование — `FLICK-U003`.

**R3. Dehydrate/hydrate.** ✅ `c29e835`, `5205e61`
- AC: `Ui.Server.state` пишет JSON-скрипт на контейнер (`<` экранирован); `Ui.Dom.mount` гидрирует до первого рендера; незакодированный атом — дефект с ключом в обоих профилях; несколько корней на странице.

**R4. Паритет сервер/клиент.** ✅ `c047a70`
- AC: view читает серверное значение до гидрации; `Atom.Browser a` требует `Atom.on_server` или `Atom.client_only`; e2e и docs-тест без hydration mismatch.

**R5. Стриминг.** ✅ `56f3a4a`, `d9d43ba` + этот пункт (`Ui.Server.stream`, `Atom.serializable_task`, e2e `tests/run/ui/stream.fl`; поток сам рендерит контейнер — React 19.3 не отдаёт оболочку без host-элемента в корне)
- AC: `Ui.Server.stream` в `Ui/Server.fl` поверх `renderToReadableStream`; значения task-атомов дописываются чанками-скриптами перед своей разметкой; клиент гидрирует их до гидрации разметки (тест `runtime/test/ui-stream.test.ts`).
- 🆕 AC (§6.3, правило C): стриминг откладывает только поддеревья внутри `Ui.suspended`; task-атом, прочитанный вне границы, рендерится как `Loading` в первом чанке без ожидания; тест на это.
- AC: размер клиентского бандла не меняется (`just bundle-size`); e2e: страница стримится, гидрируется без mismatch, task не запускается на клиенте повторно.

**R6. Шаблоны без React.** ✅ `57a7b63`
- AC: `Ui.Server.to_html` чистый и детерминированный, экранирует текст и атрибуты; snapshot письма; бандл `email` без `react-dom/server`.

### Трек F — fullstack поверх Effect (после S2, R1)

**F1. Сервис `Server` и разделение бандлов.** ⬜
- AC: функция со `Server` в строке недостижима из клиентского entry (ошибка с путём вызова); клиентский бандл не содержит серверных модулей (проверка в `just bundle-size`).

**F2. `server fn`.** ⬜
- AC: компилируется в `HttpApi`/`Rpc` endpoint и клиентскую заглушку; аргументы и результат требуют кодека; ошибки `e` восстанавливаются типизированно; на клиенте вызов — `Action`/task-атом; в документации каждый `server fn` помечен как публичный endpoint.

**F3. Запросы и мутации.** ⬜
- AC: поверх `AtomRpc`/`AtomHttpApi`; query инвалидируется мутацией с тем же `reactivityKeys` (тест); stream-запрос — отдельный тип без `mutation`; `serializationKey` участвует в R3.

**F4. Маршруты.** ⬜
- AC: `Route` ADT ↔ URL с выведенным парсером/принтером (round-trip property-тест); `Atom.route` с server value из запроса; loader маршрута гидрируется.

**F5. Формы.** ⬜
- AC: `deriving Form` из record-типа; валидация — derived; ошибки кодека маппятся на пути полей; async-валидатор прерывается при новом вводе.

**F6. Шаблон проекта.** ⬜ (сейчас `flick new --template app|lib`)
- AC: `flick new --template fullstack` — сервер + клиент + общий модуль API; `flick dev` поднимает оба; e2e в `flick/tests/e2e.rs`.

### Трек O — observability (после S3, C6)

**O1. Экспорт.** ⬜ AC: `Otlp.layer` из Flick, пример в `examples/` с локальным коллектором; `with_tracer` принимает OTLP-трейсер.
**O2. Именованные span'ы action.** ⬜ AC: каждый запуск `Action` — span с меткой C6 и атрибутами политики; отклонение/прерывание — события span'а.
**O3. Причинность записей.** ⬜ AC: запись атома несёт место в исходнике и id span'а/fiber'а; dev-логгер печатает «атом ← action ← событие ← span»; с `labels = "all"` (C6b) работает и в release.
**O4. DevTools.** ⬜ AC: `flick dev --devtools` подключает `DevTools.layer`; видны span'ы action и записи атомов с метками.
**O5. SSR-трейсинг.** ⬜ AC: trace context запроса продолжается в рендере и server fn; клиентский вызов server fn пробрасывает `traceparent`.

### Трек L — CLI (основа: `Cli.fl`, блок `cli`, `Cli/Prompt.fl`)

**L1. Примеры.** ⬜ AC: `examples/cli-tool` (подкоманды, конфиг/env, prompt-мастер, completions), запускается в e2e.
**L2. Шаблон.** ⬜ AC: `flick new --template cli`; `flick run -- --help` выводит сгенерированную справку.
**L3. Observability в CLI.** ⬜ AC: флаг `--trace` включает OTLP (O1); `GlobalFlag.LogLevel` подключён.
**L4. Терминальный UI (исследование).** ⬜ AC: записка о рендерере атомов в терминал (Ink через `Ui.mount_with` или свой) с замером веса и решением.
**L6. 🆕 Несколько точек входа.** ⬜ (после S2–S4)
- AC: `[project] entries = { server = "App.Server", cli = "App.Cli" }` (`entry` — сокращение для одного); сборка пишет раннер на каждый вход (`flick-server.js`, …) в один каталог, общие модули — один раз; `flick run <name>`, `flick dev <name>`; `flick pack` даёт `"bin"` с каждым входом; e2e-тест на два входа.
- AC: решение о реэкспортированном `main`: раннер импортирует `main` из модуля-источника, так что фасад (`entry = "App"`) может быть входом, — либо запрет остаётся; записано в `implementation-decisions.md`.
- AC: в `reference/project.md` описано, что веб-входы (Vite/rolldown) — это JS-модули, импортирующие `.fl`, а `entries` — раннеры приложения.

**L5. `flick dev` для фронта.** ⬜ AC: решение записано: браузер — HMR модулей Vite с сохранением состояния реестра (атомы с метками C6 переживают замену); сервер — текущий перезапуск.

## 4. Порядок и зависимости

```
B0…B9 ✅ ─ B7b ✅                       (замер, независимо)
S1 ✅ ─ U1…U6 ✅ ─ U7 ✅ ─ U7b ✅ ─ U6b
        └─ R1…R4 ✅ ─ R5 ✅ ─ R6 ✅
C2/C3/C4/C6 ✅ ─ C6b ✅
C7a ✅ ─ C7b ─ C7c ─ C7d ─ C5            C1 (бенчмарк, может закрыться без реализации)
U9 ✅ ─ U9b
S2 ─ F1 ─ F2 ─ F3 ─ F4 ─ F5 ─ F6
S3 ─ S4 ─ O1 ─ O2 ─ O3 ─ O4 ─ O5      (O2/O3 после C6b)
L1 ─ L2 ─ L3 (после O1) ─ L4 ─ L5
```

Вехи:
1. «Видно в DevTools и меньше бандл» — ✅ (B0–B9, S1, S5, U1, C6).
2. «Типы гарантируют» — 🔶: C2–C4, U2–U6 ✅; осталось C7a–d, C5, U6b, U7b, C1.
3. «SSR» — 🔶: R1–R4, R6, U7 ✅; осталось R5, U9.
4. «Fullstack» — ⬜: S2, F1–F3, O1–O3.

Ближайшее: закоммитить R5 (с правилом §6.3), C6b, U7b, U9, B7b; затем C7a → C7b.

## 5. Риски

1. **Нестабильность `effect/reactivity`** (unstable, 4.0.6 уже меняет `Hooks.ts`, `RegistryContext.ts`). Митигация: U9 — один слой контакта; версия Effect закреплена; тесты на поведение.
2. **C1 трогает ядро** (`typing.rs`, `effects.rs`, IR, кодген, спеки). Митигация: сначала бенчмарк, C1 только при значимой разнице.
3. **Вес `effect/reactivity`** (~25 KB gz, держит `Stream`). Решение §6.6: остаёмся; при превышении бюджета U8 — свой реестр за границей U9.
4. **Глобальный реестр по умолчанию в Effect.** Закрыто R1: реестр — обязательный сервис на сервере.
5. **Инференс захватов** (Scala CC до сих пор experimental). Митигация: C5 требует явных аннотаций в сигнатурах top-level.
6. 🆕 **Стоимость синтаксиса компонентов** (C7b: парсер, fmt, tree-sitter, LSP). Митигация: C7a до кода; семантика проверяется тестами C7c/C7d, синтаксис — тонкий desugar.
7. 🆕 **Метки `all` в публичном бандле** раскрывают имена модулей. Митигация: документировать; `components`/`none` для тех, кому это важно.

## 6. Решения (приняты 2026-10-25)

1. **Синтаксис компонентов — ключевые слова** `component` / `setup` / `render`. Перед реализацией разобрать ReScript (`@react.component`, generic JSX transform) и перенести опыт кастомизации → C7a–C7d.
2. **Старый UI удаляется целиком, без deprecated-релиза** — ничего не выпущено. Ref-API удалён (`5daf143`); остаток — `Ui r e` → `Ui` (U7b).
3. **Suspense только opt-in** (`Atom.Async` + `Ui.suspended`, U6). Добавить хелпер для коротких экранов, не меняя умолчание (U6b). Стриминговый SSR откладывает только внутри явной границы (R5).
4. **Метки: все три настройки остаются, умолчание — `all`** (C6b), с замером разницы в `just bundle-size`.
5. **Хэши тегов:** сначала замер; при выигрыше < 1 KB gz — оставить хэш и закрыть записью; иначе голое имя при уникальности, хэш для кодеков/Schema и библиотек (B7b).
6. **Остаёмся на `effect/reactivity`**; свой реестр — только план отхода за границей U9.
