# Роадмап: реактивный UI, fullstack на Effect, CLI, observability, бандл

Статус: рабочий. Решения §6 приняты; исходная сверка статусов реализации — история `hetzner/flick-lang` до `d524e92`. Замечания ревью срезов `4a96533` и `98c4092` перенесены в трек Q (§7), без нового прогона кода при переносе.
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
| [core-option-equality.md](core-option-equality.md) | равенство и хеш `Option`/`Result` `core` и Effect, импорты сборок, размер `_core/`; решение H6 |
| [terminal-ui.md](terminal-ui.md) | атомы в терминале: адаптер Ink через `Ui.mount_with`, вес Ink, свой рендерер; решение L4 |

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
| `Setup s` | `setup` компонента | `Atom.local`, локальные action, финализаторы региона `s`, `on_mount`/`watch`, `Ui.element` | ⬜ C7, C8, U10 |
| `Dom` | action, задачи (клиент) | `focus`, `blur`, `scroll_into_view`, `measure` по handle | ⬜ U10 |
| `Server` | серверный entry, `server fn` | request/cookies/DB-слои | ⬜ F1 |

## 3. Треки

Нумерация: `S` — stdlib surface, `B` — бандл и выходной JS, `C` — компилятор/язык, `U` — UI-ядро, `R` — SSR, `F` — fullstack, `O` — observability, `L` — CLI, `H` — хосты и бэкенды эффектов, `D` — сахар прямого стиля, `Q` — замечания ревью (§7). Внутри трека порядок = приоритет, кроме Q: там сохранена нумерация ревью, порядок исправления — §7.3.

### Трек S — снять исключения Effect-entrypoint'ов

**S1. `effect/reactivity`.** ✅ `6445878`
- AC: `"effect/reactivity" = { modules = "reactivity" }`; каждый экспорт классифицирован `[wrapped]`/`[excluded]` с причиной; `*Unsafe`, guards, TS-only исключены; линтер покрытия зелёный; модули помечены unstable.

**S2. `effect/rpc`.** ✅ (модуль `Rpc`: процедуры с кодеками, группа как запись обработчиков, `listen` по HTTP + NDJSON, `call`/`call_stream`; 169 экспортов классифицированы; + клиент без Node-сервера, сериализации `ndjson`/`json`/`json_rpc`/`ndjson_rpc`/`binary`, WebSocket: `listen_websocket`, `Rpc.websocket`, `connect`/`call_on`; TCP: `listen_tcp`, `Rpc.tcp`; stdio: `serve_stdio`, `spawn`; `with_middleware`, `with_headers`; `uninterruptible`, `call_local`; обработчик для `Http.Router` — S2b ✅)
- AC: `Rpc`, `RpcGroup`, `RpcClient`, `RpcServer`, `RpcSerialization` классифицированы; run-тест в `tests/run/stdlib/rpc/`: сервер и клиент на одном `RpcGroup`, unary и stream вызов; линтер покрытия зелёный.

- Отложено: RPC через workers (нужны несколько точек входа компилятора и `@effect/platform-browser`).

**S2b. 🆕 RPC в маршрутизаторе Flick.** ✅ (`Rpc.handler`/`handler_websocket serialization group handlers` — HTTP-обработчик `^{s}`; `route`/`prefix_route`/`router` переносят захват обработчика `^c` на значение; e2e `tests/run/stdlib/rpc/router.fl`, G001 `tests/check/fail/stdlib/rpc_router.fl`)
- AC: `Rpc.handler`/`handler_websocket` монтируются в `Http.Router.route`; для этого `route`/`router` принимают обработчик с захватом региона сервера (`^s`), фикстура G001 на утечку; тест: группа на `Http.Server.serve` рядом с обычными маршрутами.

**S3. `effect/observability`.** ✅ (модули `Otlp` — `layer`/`traces`/`logs`/`metrics` с `{ url; service_name }`, HTTP-клиент и JSON внутри — и `Prometheus.format`; `Otlp.Settings` + `*_with`: версия, атрибуты, заголовки, gzip, `Otlp.protobuf`, интервалы, пачка, таймаут; `Otlp.from_config` по `OTEL_*`; `Prometheus.route`, `format_with`)
- AC: обёрнуты `Otlp`, `OtlpTracer`, `OtlpLogger`, `OtlpMetrics`, `PrometheusMetrics`; run-тест экспортирует span в локальный HTTP-приёмник и проверяет имя и атрибуты.

**S4. `effect/devtools`.** ✅ (`DevTools.layer url`; сервер для тестов — `@flick-lang/runtime/effect/devtools/DevToolsServer` на `@effect/platform-node`; `DevTools.layer_with` с заголовками рукопожатия)
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

**C1. Стадия `Sync`.** ✅ (замер `runtime/bench/render-stage.ts`: тело рендера с тремя чтениями 16.7 → 2.8 мкс, SSR 500 view 15.7 → 6.8 мс, −57 %)
- AC: `Param.runtime: bool` → `Stage { Pure, Sync, Runtime }`; render-тело с `{Atom.Read}` компилируется без `Effect.gen` и без `runSync`, `Atom.get` — прямой вызов.
- AC: check-фикстуры: `{Atom.Read}` передаётся в параметр `->{r; e}` с автоматическим lift.
- AC: бенчмарк рендера (до/после) в `runtime/test` или `tools/`; если выигрыш < 10 % и бандл не меньше — пункт закрывается записью в `implementation-decisions.md` без реализации.
- AC: spec 03/04 и `02-decisions.md` обновлены (en + ru), если реализовано.
- Сделано: стадия решается там, где записана стрелка (`Effects.sync`), поэтому переменная строки, решённая позже в `Atom.Read`, остаётся Runtime. Lift покрывает все пары стадий: Sync → Runtime (`$rt.sync`), Runtime → Sync (`$rt.runIn`), эффектный вызов в Sync — сразу (`runIn`). Foreign со Sync-стрелкой — `FLICK-F001`.

**C2. `Read`/`Write` и правило рендера.** ✅ `869dcf0`, `d37114c`
- AC: `Atom.get` требует `Atom.Read`, запись — `Atom.Write`; render и `Atom.derived` не могут писать, использовать другие сервисы или падать — `FLICK-U004` с подсказкой (action / `Atom.task` / `Ui.suspended`).

**C3. Атомы не генерализуются.** ✅ `3215332`
- AC: top-level не-функция с атомом полиморфного типа — `FLICK-U005` «укажите тип»; функция, создающая атом на вызов, остаётся полиморфной.

**C4. Стабильная идентичность.** ✅ `bb3ab0e`, `d524e92`, `35264ca`
- AC: `Atom.make`/makers/`Action.*`/`Ui.view` в render — `FLICK-U006`, включая пользовательские функции-makers и импортированные (через `.iface`); ключ `Atom.family` требует `Hash`; атомы family освобождаются (`Atom.forget`, weak).

**C5. Захваты атомов.** ✅ (`Setup.own : a ->{} a^{s}`: `Atom.local` — `(Writable a)^{s}`, `Ui.element` — `(Element t)^{s}`; API атомов и DOM заимствует `(Writable a)^d`, `(Atom a)^d`, `(Element t)^d`; render `Ui.stateful` возвращает `Ui^{s | c}`; `has_runtime_arrow`/`require` видят стадию внутри `Captured`; локальный атом не `keepAlive`, экземпляр держит его `registry.mount` и отпускает при unmount; G001 — `tests/check/fail/regions/instance.fl`; нерешённый регион после E001/E002 не даёт лишнего G001)
- AC: `Atom.local` в `Setup s` захватывает `^s` и освобождается с экземпляром; фикстура: атом, захватывающий `File s` из `scope`, не может стать top-level (G001); top-level сигнатуры требуют явных аннотаций захватов.

**C6. Метки от компилятора.** ✅ `e5ef204` (умолчание меняется в C6b)
- AC: каждый top-level атом, action и view — `$rt.label("Mod.name", …)`; ручное имя сохраняется; `[build] labels = "all" | "components" | "none"`; source map заполняет `names`.

**C6b. 🆕 Метки по умолчанию `all` (§6.4).** ✅ (замер: метки атомов стоят 0–7 B gzip; `withLabel` со стектрейсом ≈16 мкс на атом в Node — оставлен)
- AC: `Labels::default()` = `All` (`flick-core/src/project/config.rs`), тест конфигурации и `reference/project.md` (en + ru) обновлены; `components` и `none` остаются.
- AC: `just bundle-size` собирает playground и counter также с `labels = "components"` и печатает разницу; бюджеты пересчитаны под `all` и записаны с замером в коммите.
- AC: стоимость метки атома (stack trace при создании) в release измерена; если заметна в профиле — в `all` release метка без стека.

**C7. Компоненты: `component` (§6.1).** ✅ (C7a–C7e)
- **C7a. 🆕 Разбор ReScript.** ✅ [rescript-components.md](rescript-components.md): props — одна явная запись (как `@react.componentWithProps`), `key` — метаданные узла, метка на итоговом объекте React, адаптер = хост-примитивы + монтирование с проверяемым интерфейсом; не переносим props из аргументов, `make`/модуль на компонент, выбор рендерера в конфиге, `forwardRef`. AC: записка `docs/research/rescript-components.md` с цитатами: `@react.component` и генерация типа props, `@react.componentWithProps`, generic JSX transform (`"jsx": {"module": …}` — свой JSX-модуль вместо React), `React.memo`/`memoCustomCompareProps`, как это сказывается на выходном JS и ошибках; вывод — что переносим в Flick (кастомизация рендерера/адаптера, props-тип, memo) и что нет.
- **C7b. Синтаксис.** ✅ (контекстные `component`/`setup`/`render`: `LetDecl` с телом `ComponentExpr`, без `setup` — `Ui.view (fun props -> render)` с меткой C6; `setup` разбирается и форматируется, проверка — `FLICK-S002` до C7c, имена `setup` уже видны в `render`; порядок секций — `FLICK-S014`; tree-sitter: секция после `=` объявления или первым словом на строке, которая после него кончается (или `render ui`), — внешний сканер) AC: парсер/AST/`lower` для `component name (props : P) = setup … render …` (setup необязателен); `flick fmt` идемпотентен на фикстурах; tree-sitter-грамматика в `editors/` и семантические токены LSP знают ключевые слова; ошибки разбора по-человечески («`render` должен идти после `setup`»).
- **C7c. Типы.** ✅ (`service Setup (s : Region)` в Prelude, Sync-сервис с операцией `own`; `Ui.stateful : (forall s. p ->{Setup s;} (p ->{Atom.Read;} Ui^c)^{s | c})^c -> (View p)^c`; компонент с setup — `Ui.stateful (fun props -> setup; fun props -> render)`, setup видит первые props, render — текущие; `Atom.local` — `Setup.own (make v)`; render компонента — рендер для U006, setup — нет; `Setup` вне setup — U004/E002 с подсказкой; hover на `setup`/`render`; рантайм — `uiStateful`, setup в инициализаторе `useState`; захват `^s` у `Atom.local` — в C5, вместе с `(Writable a)^d` в API атомов) AC: `component` desugar'ится в `forall s. p ->{Setup s | r; e} (p ->{Atom.Read} Ui)^{s}` по образцу `Layer.from_build` (`Layer.fl:188`); setup не может быть условным по построению; `Atom.local` и `Action.*` разрешены в setup; hover LSP показывает тип экземпляра и регион.
- **C7d. Рантайм.** ✅ (setup — ленивый `useRef`, ровно один раз на экземпляр под StrictMode; action, созданные в setup, собирает `instanceActions`, при unmount `Bridge.release` прекращает их события и прерывает задачи, которые корень по-прежнему дожидается; `runtime/test/ui-components.test.ts`; плейграунд: фильтр «только ошибки» в `problems_pane`; `Ui.view` — это `component` без setup) AC: setup выполняется ровно один раз на смонтированный экземпляр, в т.ч. под StrictMode (тест в `runtime/test`), повторный mount не утекает fiber'ами; action экземпляра прерываются и ожидаются при unmount (перенесено из U1/U2); компонент получает метку C6.
- **C7e. 🆕 Без слов `setup`/`render`.** ✅ (решение 2026-10-26, §6.1: тело компонента — обычный блок; операторы перед последним выражением — setup, последнее выражение (`ui { … }`, `if`, `match` …) — render; тело из одного выражения — `Ui.view`; чтение атомов в setup — ошибка с подсказкой «читайте в последнем выражении, `ui { let x = … }`, или `Atom.derived`»; `FLICK-S014` и внешний сканер секций tree-sitter удалены; hover — на `component`) AC: парсер, типы, lowering, U006, LSP, tree-sitter/VS Code, фикстуры, плейграунд и `reference/ui.md` (en + ru) на новом синтаксисе.
- AC (всё C7): `Ui.view` либо выражается через `component` без setup, либо удаляется; плейграунд использует `component` хотя бы для одного виджета с локальным состоянием; `reference/ui.md` (en + ru).

**C8. 🆕 Эффекты setup — `useEffect` без массива зависимостей.** ✅ (`Ui.on_mount task` и `Ui.watch read run` — функции `Ui` поверх операции `Setup.start` с непрозрачным `Lifecycle` из Prelude: Prelude не видит `Atom`, а модуль `Setup` конфликтовал бы с операциями сервиса; задачи — `Eff {Atom.Write} none Unit`, как у action; `watch` передаёт `run` прочитанное значение; рантайм — `Bridge.fork` в layout effect, `watch` — `atomDerived` + `registry.subscribe`; сервер не запускает; `runtime/test/ui-components.test.ts`)
- AC: `Setup.on_mount task` в setup запускает задачу после commit (рантайм — `useLayoutEffect`) как fiber в регионе `s`; unmount прерывает и ожидает её; финализаторы задачи выполняются; под StrictMode (двойной mount) задача работает ровно один раз на живой экземпляр и не течёт fiber'ами (тест в `runtime/test`).
- AC: `Setup.watch (fun () -> …) task` перезапускает задачу при изменении атомов, прочитанных в первом аргументе (`->{Atom.Read}`); зависимости отслеживаются, списка нет (prior-art §8); предыдущий запуск прерывается до следующего; рантайм — производный атом + `AtomRegistry.subscribe` за границей U9.
- AC: строка задачи — `{Atom.Write | r; e}`, ошибки уходят супервизору корня, как у action; вне setup (render, top-level) — ошибка типов с подсказкой (`FLICK-U004`/`U006`).
- AC: `reference/ui.md` (en + ru): пример подписки на внешний источник (таймер/`resize`) и сравнение с React `useEffect`.

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

**U6b. 🆕 Хелпер «как Suspense», без смены умолчания.** ✅ (`Ui.awaiting failed fallback render : View (Atom (Async a e))` — тело на Flick через `Ui.suspended`, атом приходит props: замыкание над атомом с переменной `a` не проходит проверку захватов; `suspended` получил захват `^c`, как `Ui.view`; e2e `tests/run/ui/awaiting.fl`)
- AC: однострочная форма для простых экранов, например `Ui.awaiting task (fun value -> ui)` / `Ui.suspended_with default_failed fallback render`, сводится к `Ui.suspended` и не вводит новый механизм; пример в `reference/ui.md` сравнивает с ручным `match`.
- AC: тип по-прежнему требует обработчик ошибки `e` (или `e = Never`); тест рантайма: loading → loaded → failed.
- AC: никакой неявной приостановки: `Atom.get` на task-атоме по-прежнему даёт `Atom.Async`.

**U7. Миграция и удаление старого (§6.2).** ✅ `5daf143` + U7b
- ✅ AC: плейграунд и docs-header на атомах; `Ui.Dom.run`, `Ui.run_with`, `Ui.lazy_view(_with)` удалены; `ui.test.ts` разбит по темам; `reference/ui.md` и ru переписаны.
- **U7b. 🆕 `Ui r e` → `Ui`.** ✅ (строка обработчика фиксирована: `{Atom.Write}`, без ошибок; `mount` требует `Atom.Write`, супервизор получает `Failure none`; `FLICK-U001` проверяет и `Action.queue/concurrent`) AC: после U2/U3 параметры `r e` у `Ui` ничего не несут (обработчики — action со своей строкой) — тип схлопнут до `Ui` во всех builtins, тестах, docs и плейграунде; если какой-то параметр всё же нужен — записка с причиной в `implementation-decisions.md`, и он остаётся один. Deprecated-алиасов не оставляем.

**U8. Бюджет.** ✅ `5daf143`, `20edc6a`
- AC: counter на атомах ≤ 99 KB (B2 + 18), playground ≤ 110 KB gz; превышение валит `just bundle-size`. После O2/O3 (span'ы action и debug-лог записей, ≈4,4 KB) бюджеты — 104 / 112 KB.

**U9. 🆕 Граница с `effect/reactivity` (§6.6, план отхода).** ✅ (рантайм и запись сделаны; пункт про `Atom.Registry` для пользователей не выполнен: `type alias` сервиса компилятор не поддерживает — сервис в строке остаётся `Reactivity.Atom.Registry`, отдельный пункт U9b)
- AC: импорты `effect/reactivity/*` в `runtime/src` только в `atoms.ts` и `runtime/src/effect/reactivity/*`; `ui/index.ts`, `ui/server.ts`, `ui/react/*` ходят через них (сейчас импортируют `Hydration`, `Atom`, `AsyncResult` напрямую); проверка — тест/oxlint-правило `no-restricted-imports`.
- AC: пользовательский код и плейграунд используют `Atom.*`/`Ui.*` (`Atom.layer`), а не `Reactivity.*`; `Reactivity.*` в `reference` помечен как низкоуровневый unstable.
- AC: в `implementation-decisions.md` записано: остаёмся на `effect/reactivity`; условие пересмотра — бюджет U8 или ломающее изменение в Effect; замена реестра трогает только этот слой.

**U9b. 🆕 Пользовательские сигнатуры без `Reactivity.*`.** ✅ (псевдонимы сервисов и ошибок раскрываются в строках эффектов; `Atom.Registry`; реэкспорт — U9c)
- AC: пользователь пишет `{Atom.Write}` или `Atom.Registry` в строке эффектов вместо `Reactivity.Atom.Registry`: либо компилятор поддерживает псевдоним сервиса, либо `Ui.Dom.mount`/`Ui.Server.*` берут реестр из `Atom.Write` (тот же реестр в `Atom.layer`); фикстуры `tests/check/pass/ui/views.fl`, `tests/run/ui/serializable.fl` и `reference/ui.md` без `Reactivity.`.

**U9c. 🆕 Реэкспорт модулей.** ✅ (синтаксис `exposing (.., X)` уже был; исправлены квалифицированные пути к сервисам `M.S.op`, `M.S.Impl`, `M.S` как значение; `Atom.Registry` — реэкспорт; реэкспортированный `main`/`tests` не делает модуль входом — пересмотреть в L6)
- AC: модуль делает импортированную сущность (тип, ADT с конструкторами, сервис с операциями и `Impl`, значение) доступной по своему пути как ту же сущность; `Atom.Registry` — реэкспорт `Reactivity.Atom.Registry`, так что работают `Atom.Registry.get`, `Atom.Registry.Impl` и `Layer.succeed Atom.Registry`; `.iface`, кодген, LSP (goto/hover/completion) и `flick diff` знают реэкспорт; цикл реэкспорта — ошибка.

**U10. 🆕 Handle DOM-элемента — `useRef` только для DOM.** ✅ (`Ui.element : () ->{Setup s;} Element t`, теги `Input`/`Textarea`/`Button`/`Div`; привязка `input_at`/`textarea_at`/`button_at`/`div_at` — props точные записи, поле сломало бы все вызовы; сервис `Ui.Document` (не `Dom`: модуль `Ui.Dom` есть и зависит от `Ui`) с `focus`/`blur`/`scroll_into_view`/`measure` и ошибкой `Detached`; строки задач обработчиков и `on_mount`/`watch` — `{Atom.Write, Ui.Document}`, сервис даёт мост корня; `tests/run/ui/element.fl` в jsdom; плейграунд: имя файла фокусирует редактор; захват `^s` и G001-фикстура — в C5 вместе с `Atom.local`)
- AC: `Ui.element : () ->{Setup s} (Ui.Element t)^{s}` создаётся только в setup и захватывает `^s` — handle не может стать top-level или пережить экземпляр (G001-фикстура); `t` — тег (`Input`, `Div`, …), привязка к узлу через `Host`/props (`element = input`) с проверкой тега типом.
- AC: в render handle только передаётся, не читается; операции — задачи с сервисом `Dom` в строке: `Dom.focus`, `blur`, `scroll_into_view`, `measure` (`{ width; height; top; left }`); вызов из action или задачи C8; не смонтированный узел — типизированная ошибка (`Detached`), не `null`.
- AC: сервер (`Ui.Server.*`) не предоставляет `Dom` — серверный рендер с DOM-операцией не тайпчекается; при гидрации handle привязывается к существующему узлу.
- AC: плейграунд фокусирует поле через handle; `reference/ui.md` (en + ru) сравнивает с React `useRef` и Elm `Browser.Dom.focus` по id; тяжёлые императивные виджеты по-прежнему через `Ui.component` (это записано в reference).

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

**F1. Сервис `Server` и разделение бандлов.** ✅ (модуль `Server`: сервис без операций и слой `Server.node`; вход `target = "client"` в `flick.toml`; что достаёт до `Server` — тип со `Server` в строке или имя такого значения — считается до неподвижной точки, путь хранится в `.iface` (`servers`); `FLICK-E009` с путём `main → … → Server.node`; фикстура `tests/check/projects/server`, run-тест `tests/run/stdlib/server/node.fl`; `just bundle-size` — программа `fullstack`, клиентский `flick-web.js` без `App/Db.js` и `Server.js`; значение со `Server`, переданное как аргумент или через словарь класса, пути не даёт)
- AC: функция со `Server` в строке недостижима из клиентского entry (ошибка с путём вызова); клиентский бандл не содержит серверных модулей (проверка в `just bundle-size`).

**F2. `server fn`.** ✅ (контекстное слово: `server fn name x = body` — `LetDecl` с сигнатурой над ним, которую выполняет сервер, `P ->{r; e} S`; имя — клиентский вызов `P ->{; Rpc.ClientError | e} S` поверх `Rpc` (NDJSON, POST на `Server.path`, `/_flick/fn`, адрес — origin страницы или `FLICK_SERVER_URL`), годится для `Atom.task`/action; `server name` — реализация `Server.Function {Server, r | t}` для `Server.handler`, клиентский вход до неё не дотягивается (E009 с путём `server App.Api.place`); параметр, результат и ошибки — свидетели Schema, иначе `FLICK-E010` (как и обобщённая сигнатура, неперечисленные ошибки, `server x` не для server fn); `flick doc` помечает публичный endpoint; `reference/server-fn.md`; бандл `fullstack` — вызов без тела, `Db` и Node-сервера, 50 KB как у `rpc`. несколько параметров идут одним кортежем, стрелки до последней чистые; пока `Server.handler` смонтирован, вызов в том же процессе (SSR) идёт в памяти — с сервисами обработчика, заголовками запроса и кодеками, а задачи атомов получают контекст, в котором собран `Atom.layer` (трейс и заголовки запроса); shortcut: одна таблица на процесс; auth — `Server.authenticated` (Q12))
- AC: компилируется в `HttpApi`/`Rpc` endpoint и клиентскую заглушку; аргументы и результат требуют кодека; ошибки `e` восстанавливаются типизированно; на клиенте вызов — `Action`/task-атом; в документации каждый `server fn` помечен как публичный endpoint.

**F3. Запросы и мутации.** ✅ (`Atom.query keys task`, `Atom.query_stream keys stream` (без своей мутации), `Atom.mutation keys f` — после успеха `f` перезапускает прочитанные реестром запросы тех же ключей, неудача ничего не трогает; через механизм Effect: `Atom.withReactivity` и `Reactivity` реестра (тот же экземпляр, что у `AtomRpc`, через общую memo map), мутация инвалидирует ключи одной пачкой (`Atom.batch`), так что запрос нескольких ключей перезапускается один раз; сам `AtomRpc`/`AtomHttpApi` не взят: его `mutation` — атом с записью payload, а не функция с типизированной ошибкой, а `serializationKey` переносит и Failure (Q14); `serializable_task` над `query` получает ключ привязки — R3, гидратированный запрос не перезапускается, а мутация его ключа перезапускает (`tests/run/ui/stream.fl`); `tests/run/ui/query.fl`, `tests/check/pass/ui/query.fl`)
- AC: на механизме инвалидации Effect (`Reactivity`, общий с `AtomRpc`/`AtomHttpApi`); query инвалидируется мутацией с тем же `reactivityKeys` (тест); stream-запрос — отдельный тип без `mutation`; `serializationKey` участвует в R3.

**F4. Маршруты.** ✅ (`deriving Url` поверх свидетеля Schema: конструктор в kebab-case — первый сегмент, `Home`/`Index` — `/`, поля — сегменты `String`/`Number`/`Bool`/уточнённые, последнее поле-запись — query с `Option`-параметрами, иначе `FLICK-D009` с путём поля; компаньон `T.url : Url T`, модуль `Route` (`print`, `parse`), разбор проверяется схемой типа; `Atom.route url fallback` — `location`, `popstate`, запись — `pushState`; на сервере URL кладёт `Atom.serving`, разбор мемоизирован по URL, иначе стрим не сходится; `Atom.loader route load` — task-атом, перезапускаемый при смене маршрута, гидрированный результат — его первый запуск через `initialValueTarget`; `tests/run/schema/route.fl` (round-trip `Arbitrary.check` по 200 значениям; одиночный суррогат печатается как U+FFFD), `tests/run/ui/route.fl` (SSR `/user/42` без повторной загрузки, переход, «назад», `NotFound` с сервера), `tests/check/fail/schema/url.fl`; `reference/route.md`)
- AC: `Route` ADT ↔ URL с выведенным парсером/принтером (round-trip property-тест); `Atom.route` с server value из запроса; loader маршрута гидрируется.

**F5. Формы.** ✅ (`deriving Form` на записи с `Schema`: поля `String`/`Number`/`Bool`/уточнённые или `Option` их, иначе `FLICK-D010`; компаньоны `T.form : Form T` и `T.fields` — запись `Form.Field`, опечатка в поле — обычная `T003`; модуль `Form`: `draft` (атомы текста и ошибки на поле), `input` (подпись, `aria-label`, `aria-describedby` на ошибку-alert, `aria-invalid` — новое поле `Ui.Host`), `validate` (текст читается как тип поля, запись проверяется схемой со всеми ошибками, `errors: "all"` только здесь, ошибки ложатся по первому сегменту пути, фокус на первое неверное поле в порядке страницы), `report`; асинхронная проверка — `Ui.Event.latest`, новый ввод её прерывает; `tests/run/ui/form.fl`, `tests/check/fail/schema/form.fl`; `reference/form.md`)
- AC: `deriving Form` из record-типа; валидация — derived; ошибки кодека маппятся на пути полей; async-валидатор прерывается при новом вводе.

**F6. Шаблон проекта.** ✅ (`flick new/init --template fullstack`: входы `server` (`Runner.Node`, слушает `PORT`) и `web` (`Runner.Browser`, `target = "client"`), общий модуль `App.Api` из `server fn` — чтение всем, запись под `Server.authenticated` по cookie `session`, хранилище `App.Store` — сервис только сервера, страница: `Atom.query`/`mutation` и форма F5, Vite с `@flick-lang/astro` и прокси `/_flick`; `flick dev` без имени запускает все неклиентские входы и Vite для клиентских; тест — `flick/tests/cli_fullstack.rs` (check, fmt, `flick dev` отвечает на вызовы, запись без сессии — `Unauthorized`), а не `e2e.rs`, который гоняет только фикстуры; `@flick-lang/astro` пока `private` и не публикуется, как и рантайм — релиз npm-пакетов не собирает)
- AC: `flick new --template fullstack` — сервер + клиент + общий модуль API; `flick dev` поднимает оба; e2e в `flick/tests/e2e.rs`.

### Трек O — observability (после S3, C6)

**O1. Экспорт.** ✅ (`Otlp.tracer : Options ->{Scope s;} Tracer^{s}`, `with_tracer` принимает `Tracer^d`; пример `examples/observability` с e2e `flick/tests/examples.rs` на локальном приёмнике; run-тест `observability/tracer.fl`) AC: `Otlp.layer` из Flick, пример в `examples/` с локальным коллектором; `with_tracer` принимает OTLP-трейсер.
**O2. Именованные span'ы action.** ✅ (`Bridge.start` — `useSpan` с меткой action, без метки — `Ui.event`; атрибуты `flick.action.policy`, `capacity`/`limit`, `flick.event`, `flick.fiber`; события `rejected` (exhaust, вытесненный latest, полная очередь, предел concurrent) — на span'ах идущих задач, `interrupted`; `Action.with` сохраняет метку; `runtime/test/ui-spans.test.ts`) AC: каждый запуск `Action` — span с меткой C6 и атрибутами политики; отклонение/прерывание — события span'а.
**O3. Причинность записей.** ✅ (записи `Atom.Write` при уровне Debug пишут `Effect.logDebug` «atom App.count ← action App.go ← event click ← span <id> (fiber #n) at App:14»; место даёт компилятор: lowering переписывает `Atom.set/update/modify` в `*_at "Module:line"`, операции `Atom.Write` принимают место первым аргументом (стек не годится: задача собирается лениво, кадра `.fl` в нём нет); имена — метки C6b, так что и в release) AC: запись атома несёт место в исходнике и id span'а/fiber'а; dev-логгер печатает «атом ← action ← событие ← span»; с `labels = "all"` (C6b) работает и в release.
**O4. DevTools.** ✅ (`flick run/dev --devtools[=url]` запускает Node с `--import @flick-lang/runtime/runner/devtools`: модуль отдаёт `runMain` обёртку с `DevTools.layer` и уровнем Debug, так что записи атомов — события span'ов; собранные программы этого кода не несут, бюджеты B не меняются; `runtime/test/entry.test.ts`, e2e `flick/tests/cli_devtools.rs`) AC: `flick dev --devtools` подключает `DevTools.layer`; видны span'ы action и записи атомов с метками.
**O5. SSR-трейсинг.** ✅ (`Ui.Server.render`/`stream` — спан `flick.render` под спаном запроса HTTP-сервера Effect, который читает `traceparent`; вызов `server fn` идёт через HttpClient Effect и несёт `traceparent`, сервер выполняет его спаном `Module.name` в том же трейсе; `tests/run/stdlib/server/trace.fl` на локальном коллекторе: страница, вызов из обработчика и вызов клиента — по одному трейсу) AC: trace context запроса продолжается в рендере и server fn; клиентский вызов server fn пробрасывает `traceparent`.

### Трек L — CLI (основа: `Cli.fl`, блок `cli`, `Cli/Prompt.fl`)

**L1. Примеры.** ✅ (`examples/cli-tool` — заметки: подкоманды `init`/`add`/`list`/`done`, файл из `NOTES_FILE` через `Config`, мастер `init` на `Cli.Prompt.text` с проверкой, встроенные `--completions`; тесты мастера на поддельном `Tty`; `flick/tests/examples.rs` гоняет команды с `NOTES_FILE`. Попутно: ошибки `File.node` несли старый формат тега, `try … with File.ReadError` их не ловил — исправлено) AC: `examples/cli-tool` (подкоманды, конфиг/env, prompt-мастер, completions), запускается в e2e.
**L2. Шаблон.** ✅ (`flick new/init --template cli`: `main` — блок `cli` с флагом и аргументом по умолчанию, `Cli.run_version`; `flick/tests/cli_template.rs` — check/fmt/test и `run -- --help`; в `cli-dsl.md` дописаны `--wizard`, `--log-level`, PowerShell) AC: `flick new --template cli`; `flick run -- --help` выводит сгенерированную справку.
**L3. Observability в CLI.** ✅ (`Cli.TraceFlag` — встроенный по умолчанию `--trace`: глобальный флаг корня (Effect принимает в `CliConfig` только свои), обработчик идёт span'ом с именем команды под `Otlp.layer` на `OTEL_EXPORTER_OTLP_ENDPOINT`/`OTEL_SERVICE_NAME`, экспортёр грузится лениво; `--log-level` уже был, теперь проверен; `tests/run/stdlib/cli/trace.fl`) AC: флаг `--trace` включает OTLP (O1); `GlobalFlag.LogLevel` подключён.
**L4. Терминальный UI (исследование).** ✅ ([terminal-ui.md](terminal-ui.md): Ink в рантайм не берём — адаптер из 9 строк над `render()` Ink подключается через `Ui.mount_with` пакетом пользователя; Ink с зависимостями — 135 KB gzip, из них yoga 52 и react-reconciler 43, 8.3 MB установки, импорт 136 мс; свой рендерер всё равно требует reconciler; пересмотреть, когда у `Ui` появятся примитивы без DOM) AC: записка о рендерере атомов в терминал (Ink через `Ui.mount_with` или свой) с замером веса и решением.
**L6. 🆕 Несколько точек входа.** ✅ (`entries`, раннер `flick-<name>.js` на вход, `flick run/dev <name>`, `bin` в `pack`; реэкспортированный `main` разрешён — раннер импортирует его из модуля-источника)
- AC: `[project] entries = { server = "App.Server", cli = "App.Cli" }` (`entry` — сокращение для одного); сборка пишет раннер на каждый вход (`flick-server.js`, …) в один каталог, общие модули — один раз; `flick run <name>`, `flick dev <name>`; `flick pack` даёт `"bin"` с каждым входом; e2e-тест на два входа.
- AC: решение о реэкспортированном `main`: раннер импортирует `main` из модуля-источника, так что фасад (`entry = "App"`) может быть входом, — либо запрет остаётся; записано в `implementation-decisions.md`.
- AC: в `reference/project.md` описано, что веб-входы (Vite/rolldown) — это JS-модули, импортирующие `.fl`, а `entries` — раннеры приложения.

**L5. `flick dev` для фронта.** ✅ (решение и реализация, строка «Разработка страниц» в `implementation-decisions.md`: скрипт входа страницы принимает свою замену, `hotDispose` из `Runner.Browser` снимает значения помеченных состояний смонтированных корней и прерывает старую программу, её финализаторы размонтируют корни, так что наблюдатели не множатся; `hotCarry` — новый корень начинает с этих значений; тексты черновика формы помечены `Type.field`; хуки — в `hot.ts`, раннер браузера React не тянет; проверено вручную в headless Chromium: черновик пережил правку `Web.fl`, корень один; сервер — прежний перезапуск) AC: решение записано: браузер — HMR модулей Vite с сохранением состояния реестра (атомы с метками C6 переживают замену); сервер — текущий перезапуск.

### Трек H — хосты и бэкенды эффектов 🆕

Цель: программа на Flick запускается на любом хосте (Node, браузер, Cloudflare Workers, свой), а Effect — бэкенд, который можно выключить так, что его нет в бандле.

**H1. Тип `main` и чистый `main`.** ✅ `5f9281b`
- AC: `main` — `Unit -> Unit` (вызывается без рантайма) или `Unit ->{; AppError} Unit`; другой тип, сервис в строке или ошибка не `AppError` — `FLICK-P026` при проверке, а не дефект при запуске.

**H2. Runner'ы.** ✅ `bf7d437`, `8ea7afa`, `a1afb78`
- AC: `[project] runner = "<модуль>"`; runner — Flick-модуль с `type alias Main` (тип `main`, проверяет checker, иначе `P026`) и `run`; раннер входа — `export default run(main)`.
- AC: встроены `Runner.Node` (сигналы, код выхода), `Runner.Browser` (консоль, вход со страницы; виджеты сайта без кастов `runMain`), `Runner.Cloudflare` (`{ fetch }`, `main : Fetch ->{; AppError} ServerResponse` с запросом, привязками `var`/`binding` и `wait_until`); сторонний runner — пакет с таким модулем.
- AC: вход `entries` может быть таблицей `{ module, runner }` со своим runner'ом; `[project] runner` — по умолчанию для остальных.

**H3. `[build] effect = "none"`.** ✅ `c2cfa9b`, `64efe93`
- AC: вызов времени выполнения — `E001`, `task`, уточнённый тип, схема и встроенный модуль без версии на `core` — `E008`; `main` — `Unit -> Unit`.
- AC: вывод импортирует только `@flick-lang/runtime/core`, где нет ни одного импорта `effect` (тест); Option/Result — простые данные с полями Effect, привязки Prelude, List, Option, Result, String, Number, Bool сверены с Effect parity-тестами; программа `none` в `just bundle-size` — 0.4 KB gzip без `effect/dist/*`.
- AC: библиотека на Effect пишет в `_core/` модули, которым Effect не нужен, и отмечает это в `.iface` (`abi.core`); приложение `none` импортирует их, модуль пакета без такой сборки — `E008`; библиотека, собранная только с `none`, в приложении на Effect — `P027` (её Option не Effect'овский, `==` их не сравнит).

**H4. `[build] ui`.** ✅ `727e396`
- AC: модуль вместо `@flick-lang/runtime/ui` и `ui/react` для привязок `Ui` и `Action`; относительный путь — от корня проекта.

**H5. Сервисы runner'а.** ✅ (`Runner.Cloudflare.Env` — сервис с `var`/`binding`, поле `env` из `Fetch` убрано, `Main = Fetch ->{Env; AppError} ServerResponse`; runner предоставляет его каждому запросу через необязательный `provide` у `start` в `entry.ts`; вычитать ничего не пришлось: `subsume` ковариантен, так что `main` может требовать любое подмножество сервисов `Main`, а чужой сервис — `P026`; `project::tests::main_may_need_the_services_its_runner_provides`, e2e `flick/tests/build_runner.rs`)
- AC: runner объявляет сервисы, которые предоставляет сам (например, `Cloudflare.Env` вместо поля `env`), и `main` может держать их в строке; проверка `P026` вычитает их из строки.

**H6. Пакеты на `core` без второй сборки.** ✅ (исследование, [core-option-equality.md](core-option-equality.md): двойная сборка остаётся — `Equal.equals(Option.some(1), {_tag:"Some",value:1})` ложно, хеши разные, `Option.isOption` простого объекта ложно; к тому же Effect-сборка модуля импортирует `effect`, так что и при совместимых данных приложение `none` её не возьмёт; `_core/` у `@flick-lang/dts-importer` — 5.8 KB против 44 KB модулей, 13%; в `examples/` пакетов нет; строка «Библиотеки без Effect» в `implementation-decisions.md`)
- AC: решение записано: либо Option/Result Effect и `core` совместимы по `==` и хешу (тогда `_core/` не нужен), либо остаётся двойная сборка; замер, сколько весит `_core/` у пакетов из `examples/`.

### Трек D — сахар прямого стиля 🆕

Цель: с первоклассным прямым стилем код над `Option`/`Result`/списками и пользовательскими монадами пишется без цепочек `flat_map`/`match`; генераторы — нативный JS, без лишних аллокаций. Решение §6.7.

**D1. Спека.** ✅
- AC: `spec/01,02,05,06` (en + ru) описывают `do` над `Monad` с чистыми продолжениями, `for … yield` и `[for … yield]` вместо «без обобщённого `do`».

**D2. Классы прелюдии.** ✅
- AC: `Semigroup`, `Monoid` (инстансы `String`, `List a`; у `Number` нет — обёртки D13), `Alternative` (`nothing`, `or_else`; `Option`, `List`), `Applicative.map2` во всех инстансах (foreign: каррированное тело на Flick не проходит проверку захватов — замыкание держит `f a`).

**D3. `do`: синтаксис, типы, IR.** ✅ (вместе с D4)
- AC: `do { … }`; утверждения `p <- e`, `let`, `let assert`, `if c` без `then`, выражение; `<-` — токен лексера (`a<-1` — `FLICK-S013` с подсказкой `a < -1`; отформатированный код всегда пишет `a < -1`); `do` без скобок — `FLICK-S010`.
- AC: `flick fmt` идемпотентен, tree-sitter (`<-`), биндеры для unused/IDE, hover и сворачивание `do`; `reference/keywords.md`, `grammar.md` (en + ru).
- `for`/`yield` и их синтаксис — в D6/D9.

**D4. Типизация `do`.** ✅ (в D3)
- AC: `do` раскрывается в методы `Monad`/`Alternative` прелюдии по абсолютному пути (пользовательский `flat_map` не перехватывает); опровержимый образец после `<-` — ошибка с подсказкой `match`; эффектный вызов в продолжении — отдельный диагноз с исправлением «связать до `do`»; ошибки указывают на утверждение.
- AC: фикстуры `tests/check` и `tests/run`: `Option`, `Result`, `List`, `Eff`, обобщённый `Monad m =>`.

**D5. `Seq` и генераторы.** ✅
- AC: `Seq a` в прелюдии — повторно итерируемый объект (`[Symbol.iterator]` запускает генератор заново), захваты `(Seq a)^c`; модуль `Seq`: `from`, `iterate`, `repeat`, `cycle`, `unfold`, `range`, `range_by`, `append`, `map`, `filter`, `filter_map`, `flat_map`, `take*`, `drop*`, `zip*`, `enumerate`, `scan`, `chunks`, `windows`, `to_list`, `fold`, `any`, `all`, `find`, `first`, `each_effect`, `fold_effect`, `of_list`; `Stream.from_seq`; TS-проекция `Iterable<A>`; работает и с `effect = "none"`.
- AC: `Functor`/`Foldable`/`Applicative`/`Monad`/`Alternative` для `Seq`; тест рантайма: повторная итерация, `take` не тянет лишнего.
- Класс `Iterable` для источников `for` — в D6.

**D6. `for` в IR.** ✅
- AC: `for { … } yield e` и `[for … yield e]` (клаузы в скобках или через `;`/`if`); `for`/`yield` — ключевые слова; класс `Iterable` (`List`, `Seq`, без затрат: JS обходит их как есть).
- AC: узлы IR `Generate`/`Each`/`Yield` проходят `opt/`; ленивый — один `$rt.seq(function* …)` с вложенными `for…of`, строгий — цикл с `push` без генератора; опровержимый образец пропускает элемент; клаузы чистые (`E001`), ленивый `Seq` держит захваты (`G001` при утечке из `scope`).

**D7. Sync-тело генератора.** ✅ (строгий `[for]` тоже делает Sync-вызовы — сразу, в своей стадии)
- AC: Sync-вызовы в теле `for … yield` выполняются при `next()` в контексте, захваченном при создании; строка ошибок `none`; Async/Scope — ошибка; повтор эффектов при повторной итерации задокументирован.

**D8. Линты и исправления.** ✅
- ✅ `FLICK-T029` `do` над Eff → `task` с `await` (`x <- e` → `let x = await e`, утверждение → `await`/`ignore await`, `pure v` → `v`); `T030` `x <- e; pure x` → `e` (safe); `T031` `Seq.to_list` бесконечной последовательности (`from`/`repeat`/`iterate`/`cycle` через `map`/`filter`/… без `take*`); `T032` `Seq.to_list (for …)` → `[for …]` (safe); `T033` `xs |> List.filter (fun x -> p) |> List.map (fun x -> e)` → `[for x <- xs if p yield e]` при одном имени параметра.
- ✅ `T035` цепочки `flat_map`/`Option.bind`/`Result.bind` с `fun x -> …` и `match … None -> None`/`Err e -> Err e` → `do` (safe, от двух шагов, в одну строку); у `E001` в чистом остатке `do` — исправление «связать вызов `let` до `do`» (unsafe: вызов выполнится и при `None`; не предлагается, если вызов использует переменные блока).

**D9. Эффектный `for`.** ✅
- AC: `for { … } do body` / `for x <- xs do body` (тело — блок с отступом, в скобках или в строку) типа `Unit`; клаузы и тело могут делать эффектные вызовы, строки объединяются со строкой функции; в стадии времени выполнения — `for…of` с `yield*` внутри текущего `Effect.gen`, без генератора; прерывание посреди цикла (тест с `timeout_option`).
- `do` больше не начинает аргумент (как `if`/`match`): `f (do { … })`.


**D10. Applicative `do`.** ✅ (`and` — ключевое слово только между связками `do`; `Validation` — в D13)
- AC: `x <- a and y <- b` → `map2`, нужен только `Applicative`; ссылка между связками `and` — ошибка; `Validation` накапливает ошибки (D13).

**D11. Постфиксный `?`.** ✅ (исправление — `FLICK-T034`)
- AC: `r?` от `Result (Errors e) a` в эффектной функции — `a` с `e` в строке; `o?` от `Option` в функции, возвращающей `Option`, — ранний `None`; иначе ошибка с подсказкой; исправление `match … Err e -> …` → `?`.

**D12. Диапазоны.** ✅ (`a..b` — `Seq.range_to`, `a..<b` — `Seq.range`, `a..` — `Seq.from`; приоритет между сравнением и `++`)
- AC: `a..b`, `a..<b`, `a..` → `Seq`; `[a..b]` — `List`; `exposing (..)`, `...rest`, `1.5`, `x.field` разбираются как раньше (фикстуры).

**D13. Map/Set и моноиды.** ✅ (модули `Monoids`, `Validation`; `fold_map` берёт функцию без захватов — шаг `fold` их не держит)
- AC: `HashMap.from_seq`/`to_seq`, `HashSet.from_seq`; `Sum`, `Product`, `First`, `Last`, `Min`, `Max`; `Foldable.fold_map`; `Validation e a`.

**D14. Fusion.** ✅ (`tools/fusion-bench`: `for` + `filter` + `map` в список — 177 → 10 мкс на 1000 элементов, ×17; `for` с Sync-телом не сливается)
- AC: проход `opt/fuse.rs` (release) сливает `Seq.map/filter/flat_map/take/to_list` и `List.map/filter` над `For` в тот же цикл, `For` над `For` — во вложенные клаузы; комбинаторы распознаются по foreign-биндингу; сливаются только чистые функции; debug и release дают одинаковый вывод; бенчмарк в `tools/`.

**D15. MTL и законы.** Выполнено (`examples/mtl`: `Identity`, `MaybeT`, `StateT`, `ReaderT`, `WriterT`, `ExceptT`; единый `MonadTrans` и 20 путей `MonadState`/`MonadReader`/`MonadWriter`/`MonadError`. FD-покрытие учитывает контекст и суперклассы, выбор остаётся только по головам с проверкой согласованности и лимитами. `App.Capabilities` исполняет один полиморфный workflow над `Identity` и холодным `Eff`; планировщик сохраняет прежний CLI. Законы, порядок слоёв, class defaults и захваты проверяются на реальных интерфейсах. Перечисленные AC выполнены, а не заявлена совместимость со всей экосистемой Haskell MTL.)
- AC: `examples/mtl/`: `Identity`, `StateT`, `ReaderT`, `WriterT`, `ExceptT`, `MonadState`/`MonadReader`/`MonadWriter`/`MonadError` с FD, `MonadTrans`; программа на `do` поверх `Identity` и `Eff`; невыразимое записано в `implementation-decisions.md`.
- AC: property-тесты (`Test.property`) законов `Functor`/`Applicative`/`Monad`/`Semigroup`/`Monoid` для инстансов прелюдии, `Seq`, обёрток и трансформеров; эквивалентность сахара и ручных цепочек, fused и не-fused.

## 4. Порядок и зависимости

```
B0…B9 ✅ ─ B7b ✅                       (замер, независимо)
S1 ✅ ─ U1…U6 ✅ ─ U7 ✅ ─ U7b ✅ ─ U6b ✅
        └─ R1…R4 ✅ ─ R5 ✅ ─ R6 ✅
C2/C3/C4/C6 ✅ ─ C6b ✅
C7a ✅ ─ C7b ✅ ─ C7c ✅ ─ C7d ✅ ─ C5 ✅            C1 ✅
U9 ✅ ─ U9b ✅
S2 ✅ ─ F1 ✅ ─ F2 ✅ ─ F3 ✅ ─ F4 ✅ ─ F5 ✅ ─ F6 ✅
S3 ✅ ─ S4 ✅ ─ O1 ✅ ─ O2 ✅ ─ O3 ✅ ─ O4 ✅ ─ O5 ✅
L1 ✅ ─ L2 ✅ ─ L3 ✅ ─ L4 ✅ ─ L5 ✅
H1 ✅ ─ H2 ✅ ─ H3 ✅ ─ H4 ✅ ─ H5 ✅ ─ H6 ✅     (H2 по входам — после L6 ✅)
D1 ✅ ─ D2 ✅ ─ D3 ✅ ─ D4 ✅ ─ D5 ✅ ─ D6 ✅ ─ D9 ✅ ─ D11 ✅ ─ D10 ✅ ─ D13 ✅ ─ D12 ✅ ─ D14 ✅ ─ D8 ✅   (D15 ✅ — с D4/D5; D7 ✅ после C1)
```

Вехи:
1. «Видно в DevTools и меньше бандл» — ✅ (B0–B9, S1, S5, U1, C6).
2. «Типы гарантируют» — ✅: C1–C8, U2–U6, U6b, U7b.
3. «SSR» — ✅: R1–R6, U7, U9.
4. «Fullstack» — ✅: S2, O1–O5, F1–F6, Q12–Q14.

Ближайший блок по архитектурному ревью: **Q29–Q32 (P1)**, затем **Q33–Q36 (P2)** (§7). Прежние открытые P1 сохраняют приоритет; индивидуальные отметки выполнения Q1–Q28 не сбрасываются. Отметка реализации вехи не закрывает связанные замечания ревью.

## 5. Риски

1. **Нестабильность `effect/reactivity`** (unstable, 4.0.6 уже меняет `Hooks.ts`, `RegistryContext.ts`). Митигация: U9 — один слой контакта; версия Effect закреплена; тесты на поведение.
2. **C1 трогает ядро** (`typing.rs`, `effects.rs`, IR, кодген, спеки). Митигация: сначала бенчмарк, C1 только при значимой разнице.
3. **Вес `effect/reactivity`** (~25 KB gz, держит `Stream`). Решение §6.6: остаёмся; при превышении бюджета U8 — свой реестр за границей U9.
4. **Глобальный реестр по умолчанию в Effect.** Закрыто R1: реестр — обязательный сервис на сервере.
5. **Инференс захватов** (Scala CC до сих пор experimental). Митигация: C5 требует явных аннотаций в сигнатурах top-level.
6. 🆕 **Стоимость синтаксиса компонентов** (C7b: парсер, fmt, tree-sitter, LSP). Митигация: C7a до кода; семантика проверяется тестами C7c/C7d, синтаксис — тонкий desugar.
7. 🆕 **Метки `all` в публичном бандле** раскрывают имена модулей. Митигация: документировать; `components`/`none` для тех, кому это важно.
8. 🆕 **Два стиля последовательности** (`do` и прямые вызовы). Митигация: линты D8 с исправлениями; для Eff `Monad (Eff r e)` фиксирует строки — прямой стиль остаётся основным.
9. 🆕 **`<-`, `?`, `..` в лексере/парсере** ломают существующий код (`a<-1`, `1.5`, `exposing (..)`). Митигация: контекстный разбор, регрессия по `tests/syntax`.
10. 🆕 **Узел `For` через все проходы `opt/`** (затенение биндеров клауз) и fusion, меняющий порядок эффектов. Митигация: исчерпывающие `match`, fusion только для чистых функций, тест debug = release.
11. 🆕 **Ленивость `Seq`**: бесконечные последовательности без `take` зависают, захваты должны держать регионы (G001). Митигация: линт D8, фикстуры захватов.

## 6. Решения (приняты 2026-10-25)

1. **Синтаксис компонентов — `component name (props) = body`** (пересмотрено 2026-10-26, C7e: слова `setup`/`render` убраны, последнее выражение тела — render, операторы перед ним — setup). Исходно — ключевые слова `component` / `setup` / `render`. Перед реализацией разобрать ReScript (`@react.component`, generic JSX transform) и перенести опыт кастомизации → C7a–C7d.
2. **Старый UI удаляется целиком, без deprecated-релиза** — ничего не выпущено. Ref-API удалён (`5daf143`); остаток — `Ui r e` → `Ui` (U7b).
3. **Suspense только opt-in** (`Atom.Async` + `Ui.suspended`, U6). Добавить хелпер для коротких экранов, не меняя умолчание (U6b). Стриминговый SSR откладывает только внутри явной границы (R5).
4. **Метки: все три настройки остаются, умолчание — `all`** (C6b), с замером разницы в `just bundle-size`.
5. **Хэши тегов:** сначала замер; при выигрыше < 1 KB gz — оставить хэш и закрыть записью; иначе голое имя при уникальности, хэш для кодеков/Schema и библиотек (B7b).
6. **Остаёмся на `effect/reactivity`**; свой реестр — только план отхода за границей U9.
7. **Обобщённый `do` над `Monad`, генераторы `for` и списковые выражения** (трек D) отменяют решение «без обобщённого `do`»: продолжения `do` чистые, `do` над Eff разрешён с линтом и исправлением в прямой стиль; `for … yield` — один нативный `function*`, тело чистое, Sync — после C1; `[for … yield]` — строгий цикл; fusion обязателен.

## 7. Корректирующий трек Q — ревью языка

Исторический срез `4a96533`: 10 коммитов после `3f1c7c3`, язык 0.7, Effect 4.0.5. Архитектурный срез `98c4092` — §7.2b. `P1` — нарушенный критичный контракт/блокер; `P2` — остальные дефекты и ограничения; `[INFERENCE]` — не исполнено.

Q1–Q28 сохраняют индивидуальные отметки выполнения; Qn соответствует пункту n прежнего ревью. Новые Q29–Q36 открыты и соответствуют пунктам 1–8 архитектурного ревью `98c4092`. Ниже сохранены наблюдения, причины и требования к исправлению. Статусы реализации в §3 не означают выполнения AC, нарушенных открытыми пунктами.

**Исторический итог `4a96533`:** C1/D7/D10–D14 реализованы, но тогда найдены 12 дефектов **Q17–Q28**, включая расхождения release/debug; на том срезе прежние Q1–Q16 ещё не были закрыты. Последующие исправления отмечены у отдельных пунктов. Таблица ниже — контекст того ревью, не новый аудит этих границ.

| Сценарий | Новое | Открытая граница |
|---|---|---|
| Node / Effect | Result ?, Validation, Monoids | DI, SQL ownership/отмена, TestClock |
| SPA | Sync-render, чтение атомов в for | Lifetime, controls, корректность Sync |
| Fullstack | Чистая накопительная валидация | Auth, server/client isolation, F5 |
| TS / Effect host | Явный Sync ABI | Native projections, service keys, v3 bridge |

`and` над Eff последовательный, не parallel. `->{}` не pure; none допускает defects/interruption. Task холодный, чтение Eff не должно его запускать. Exact records/сигнатуры требуют hints; FFI — доверенная граница.

### 7.1. Прежние пункты Q1–Q16

Пункты Q1–Q16 — доказательства `3f1c7c3`, не новый прогон. По ревью, причины в diff до `4a96533` не устранены; Sync-граница Q7 проверена на `4a96533`.

**Q1. P1 — Serving теряет DI-требования** ✅ (строка сервисов в типах: `HttpApi.Group n r hs`, `HttpApi.Api r gs`, `Rpc.Group r hs`; `add` унифицирует `r` обработчика эндпоинта/процедуры с группой, `listen`/`listen_websocket`/`listen_tcp`/`handler`/`serve_stdio`/`call_local` требуют её; `tests/check/fail/stdlib/serving_services.fl`)
[listen](../../flick-core/src/types/builtins/HttpApi.fl#L276-L284) не связывает r с handlers. Users без Layer компилируется; RPC — Service not found, HTTP — 500. Учитывать r handlers/middleware на serving boundary; без Layer — compile error.

**Q2. P1 — HttpOnly login даёт ложную ошибку** ✅ (значение cookie `security_set_cookie` — `Option (Redacted String)`: сервер ставит его при `Some`, `call` при скрытом `Set-Cookie` успешен с `None`, браузер шлёт cookie сам; `runtime/test/httpapi.test.ts`, `tests/run/stdlib/http/api_bodies.fl`)
Chromium сохранил cookie после 200, но [HttpApi.call требует скрытый Set-Cookie](../../runtime/src/httpapi/client.ts#L166-L178). Отделить server extras от client success: login и session работают без раскрытия секрета JS.

**Q3. P1 — Controlled fields не принимают Host** ✅ (`with_host` над `field`/`field_area` кладёт атрибуты на сам input/textarea: узел поля несёт `hosted`, повторный `with_host` дополняет; caret, сброс состояния и SSR сохраняют их; `runtime/test/ui-fields.test.ts`, `tests/run/ui/host_field.fl`)
`with_host (field …)` компилируется, импорт JS падает: component вместо element. [FieldProps](../../flick-core/src/types/builtins/Ui.fl#L262-L282) не имеет Host. Нужны id/ARIA на input/textarea, сохранение caret/IME/reset/SSR.

**Q4. P1 — Публичный реэкспорт теряется** ✅ (модуль, экспортирующий импортированное, пишет `export { … } from` модуля-владельца в JS/TS и `.d.ts`, типы без значения — `export type`; identity та же, что у владельца; `flick/tests/dts_command.rs`)
Barrel с pair собирается, но JS export отсутствует, TS2305. [Dts](../../flick-core/src/codegen/ts/dts.rs#L20-L25) отбрасывает чужой defining module. Value/type re-exports должны сохранять identity во Flick/JS/TS.

**Q5. P2 — Поздний generic теряет inference** ✅ (`codegen/ts/stages.rs`: параметр типа стоит на первой стадии, чей параметр его упоминает, упомянутые только в результате (в том числе R/E) — на последней; JS не меняется; функции со словарями классов по-прежнему получают все параметры вместе со словарями; `pair: <A>(arg: A) => <B>(arg: B) => readonly [A, B]` (снапшот), tsc в `flick/tests/dts_command.rs`: `pair(1)('x')[1]` — `string`)
`pair(1)("x")` имеет `[number,unknown]`: [оба generic на первой стадии](../../flick-core/src/codegen/ts.rs#L80-L135). Вводить B на своей стадии, включая поздние R/E, без смены ABI.

**Q6. P1 — Reference противоречит контрактам** ✅ (пример `save_button` EN/RU — обработчик с `Atom.Write`, дословно код фикстуры `tests/check/pass/ui/save_button.fl`, сверку держит `ui_reference_example_is_a_checked_fixture`; MVU-план заменён atom-first: цикла `Msg → update` нет и не планируется; `Ui.text` в spec/10 больше не «полиморфна по строкам»)
[UI-пример](../src/content/docs/reference/ui.md#L12-L31) с Users/SaveError даёт E002; save_button fixture проходит. [MVU-план](../src/content/docs/reference/ui.md#L525-L539) противоречит atom-first; DSL назван any вопреки T003. Нужны проверяемые EN/RU-примеры.

**Q7. P1 — Неполная Effect/TS-граница** ✅ (`Stream.Core.Stream` проецируется в `Stream.Stream<A, E, R>` Effect, без бренда — TS2739 нет, `Stream.runCollect` принимает поток Flick; `.d.ts` объявляет для каждого экспортируемого сервиса без параметров ключ `$s_S: Context.Service<S, { операции }>`, по которому TypeScript делает `Effect.provideService`; экспорт с последней Sync-стадией в TS — `Effect<A, never, R>`, а в JS обёрнут `$rt.syncExport`: внутри Sync-стадии Flick (задан окружающий читатель) вызов прежний, снаружи — Effect через `sync`, ABI между модулями Flick не изменился; `flick/tests/dts_command.rs::streams_services_and_sync_exports_cross_into_effect_typescript` — tsc и запуск в Node: `[1,2,3] hi ada true`; регионы по-прежнему отвергаются)
Eff сохраняет fiber/service/error identity. Stream работает в runtime, но [TS brand](../../flick-core/src/codegen/ts.rs#L259-L297) даёт TS2739; нет typed key для пользовательского service. Нужны native projections/adapters без any и потери regions. v3 требует bridge с другим owner/fiber.

Новое: [Sync export](../../flick-core/src/codegen/ts.rs#L154-L163) — `(undefined)=>number` с ambient reader; прямой вызов падает. `syncReading`, `$rt.sync`, native provider проверены. Row-polymorphic аналог остаётся Effect: стадия фиксируется при записи стрелки, позднее уточнение строки ABI не меняет.

**Q8. P1 — Error row не гарантирует wire-codecs** ✅ (строка ошибок допускает переменную типа как элемент: `{t | e}` — метка `~t` следует за элементом и становится `Module.Name`, когда `t` решена; одноимённые элементы с разными аргументами унифицируются, а не отбрасываются. `HttpApi`/`Rpc`: `ErrorCases e`, `errors : ErrorCases none`, `error_case … : ErrorCases e -> ErrorCases {t | e}`, `with_errors` добавляет `e2` к строке обработчика; эндпоинт и процедура начинаются без ошибок, так что ошибка без codec — E002/C001 до запуска, а не 500. `with_multipart_stream_payload` добавляет `MultipartError`, сервер отвечает её статусом; `tests/check/{pass,fail}/effects/variable_entries.fl`, `tests/check/fail/stdlib/error_codecs.fl`)
NotFound без with_errors принимается и становится 500 — [заявленный fallback](../../flick-core/src/types/builtins/HttpApi.fl#L177-L188). Проверять полноту codecs по error row: новый Conflict без codec отвергается до запуска. AC для S2/F2.

Элемент-переменная, решённая не-error типом (`error_case 404 (Codec.json Schema.number)`), — T002 «`Number` is not an error» на вызове: инстанцирование записывает элементы `~t`, а `report_regions` проверяет их формы; E002 о той же строке не дублируется.

**Q9. P1 — Нет SQL-транзакционной композиции** ✅ (`Database.transaction body`: тело на одном соединении между `BEGIN` и `COMMIT`, сервис `Database` внутри привязан к этому соединению; неудача или прерывание — `ROLLBACK` (при прерывании соединение закрывается, сервер откатывает сам), вложенная транзакция — `SAVEPOINT`/`RELEASE`/`ROLLBACK TO`; другие вызывающие берут другие соединения; `runtime/test/sql.test.ts` на поддельном пуле и `tests/postgres/tx.fl` на настоящем PostgreSQL: CHECK второй записи откатывает первую, при pool=1 откат одного не отменяет коммит другого, точка сохранения)
Первая запись пережила CHECK-ошибку второй; при pool=1 чужой ROLLBACK отменил успешную запись B. [Запросы отдельно берут connection](../../runtime/src/sql/pg.ts#L138-L184). Нужен scoped transaction с commit/rollback и изоляцией callers; raw BEGIN/Tx.atomically не замена. До F3/F6.

**Q10. P2 — Непоследовательные captures builders** ✅ (47 построителей `Stream` в `Transform`/`Combine`/`Errors`/`Pipe` (и поля-колбэки записей `map_both`, `tap_both`, `throttle`) — форма `Run.fl`: колбэк и поток делят захват `c`, частичное применение тоже его держит, `(a -> b)^c -> ((Stream r e a)^c -> (Stream r e b)^c)^c`; checker не менялся; локальное потребление потока с колбэком над подпиской проходит (`tests/check/pass/stdlib/stream_captures.fl`), а возврат такого потока из `scope` — по-прежнему G001 (`leaked`, `escaped`, `broadcast` в `tests/check/fail/stdlib/stream.fl`); полный e2e зелёный)
[Stream callbacks](../../flick-core/src/types/builtins/Stream/Transform.fl#L1-L4) отвергают scoped subscription даже при локальном потреблении (G001). Объединять captures callback/source без ослабления запрета утечки; D7 этого не решает.

D15 снимает прежнее ограничение пользовательских Applicative/Monad: [планировщик](../../examples/mtl/src/App/Plan.fl) использует возможности поверх `MaybeT`/`StateT`/`ReaderT`, а [Capabilities](../../examples/mtl/src/App/Capabilities.fl) — `ExceptT`/`WriterT`/`StateT`/`ReaderT` над `Identity` и `Eff`. Границы целых обобщённых значений и проекции сохраняют реальные регионы; callback-контракты учитывают позднее состояние/окружение, а stored-квалификатор модификатора `pass` не стирается. Закрыты перечисленные AC D15; Q10 для Stream builders этим не решён.

**Q11. P1 — Нет instance lifetime** ✅ (экземпляр — C7d: setup один раз, action экземпляра прерываются при unmount; `Action.with` — своё состояние политики на элемент, сохраняется при перестановке с ключами; задачи обработчика элемента по-прежнему принадлежат корню, чтобы сохранение пережило закрытый диалог; watch — C8)
[Action.with](../../runtime/src/ui/react/policies.ts#L56-L65) делит policy между аргументами; unmount не закрывает root-owned задачи. Нужны instance actions/watch и interrupt-and-wait: изоляция редакторов, drafts при reorder, StrictMode без утечек. C7/C5/C8 → U10.

**Q12. P1 — Раздельные HTTP-контуры и auth** ✅ (обёртка следующего обработчика — в `prepare`, общем для Rpc, `Server.handler` и вызовов в памяти: после хуков проверка делает сервис, который получает только этот вызов (`provideService` поверх сервисов `listen`), неудача проверки отвечает типизированной ошибкой, а обработчик не запускается; `server fn` с `Server.Unauthorized` в ошибках — `Server.Guarded`, его обслуживает только `Server.authenticated Session check`, вызывающий ловит `Server.Unauthorized`; Rpc — `with_auth` на процедуре и `authenticated` на группе; SSR-вызов в памяти несёт заголовки запроса страницы (cookie); один origin — `HttpApi.route` для `Http.Router` рядом с `Rpc.handler` и `Server.handler`; тесты `tests/run/stdlib/server/{auth,origin}.fl`, `tests/run/stdlib/rpc/auth.fl`, `tests/check/fail/server/auth.fl`, изоляция одновременных вызовов — `runtime/test/rpc-auth.test.ts`; у WebSocket заголовки общие для соединения)
HttpApi/Rpc имеют свои listeners; [Unit-hook middleware](../../runtime/src/rpc/handlers.ts#L23-L40) не передаёт CurrentUser через provide. Нужна обёртка next handler: один origin, изоляция concurrent tenants, неверные credentials не запускают handler. S2b открыт.

**Q13. P1 — F1/F6 не замкнуты** ✅ (общий контракт и CRUD с auth — шаблон F6; секреты: `Config.redacted` требует `Server.Server`, клиентский вход, читающий секрет, — `E009` (`tests/check/projects/server`, вход `admin`); одна команда — `flick dev`; черновик переживает замену модуля — L5; попутно `Server.handler` больше не тянет в браузер `@effect/platform-node`: обработчик RPC вынесен в `rpc/handler.ts` без Node, иначе Vite dev, не вытряхивающий импорты, падал на общей странице)
L6/H2 и Vite есть; server/client isolation и общий dev workflow — нет. Нужны authenticated CRUD/shared contract, server/client isolation без утечки секретов, один dev command. L5 сохраняет draft без размножения watchers; F2 после prerequisites.

**Q14. P2 — Loader переносит только success** ✅ (`Atom.serializable_result wire error_wire atom` передаёт и неудачу с одной типизированной ошибкой, закодированной её кодеком; дефект, прерывание и несколько ошибок не передаются — клиент запускает задачу сам; запрос гидрируется и неудачей, `tests/run/ui/route.fl`; поля форм — в F5)
[serializable_task](../../flick-core/src/types/builtins/Atom.fl#L142-L151) передаёт Loaded, не Failed. `[INFERENCE]` SSR 401/404 могут повторить запрос. F3/F4 должны переносить публичный Result, не Failure со стеком/секретами. Validation не закрывает F5: нужны field paths, accessible controls, async cancellation/lifecycle.

**Q15. P2 — Нет виртуального времени тестов** ✅ (`Test.clock_test` — `effect_test` с собственным `TestClock` Effect на каждый запуск; `Test.adjust`/`Test.set_time` требуют сервис `Test.VirtualTime`, так что двигать часы можно только в таком тесте; обычный `effect_test` по-прежнему на живых часах, иначе существующие тесты со сном зависли бы; `flick/tests/test_command.rs::clock_tests_run_on_virtual_time`: сон в час завершается после `adjust` без реального ожидания, у каждого теста часы с нуля)
[effect_test](../../flick-core/src/types/builtins/Test.fl#L44-L59) даёт scope, не Clock; effect/testing исключён. Нужен scoped TestClock: advance, изоляция и cleanup без реального backoff. Gen.function не заменяет это.

**Q16. P1 — Отмена не ограничивает PostgreSQL** ✅ (прерванный запрос отменяется на сервере CancelRequest протокола по отдельному сокету (`processID`/`secretKey`, без места в пуле), прерывание ждёт до 5 с его остановки и закрывает соединение; `PgConfig` получил `statement_timeout_ms` (сервер завершает оператор, `57014`) и `acquire_timeout_ms` (ожидание соединения пула), `Database.config url` — значения по умолчанию; коммит неотменяем, потерянный ответ — `DbError "commit_unknown"`, откат не обещан; `tests/postgres/tx.fl`: 20 прерванных `pg_sleep` при pool=2 не оставляют ни одного спящего backend (без отмены — 2), дедлайн — `57014`; shortcut: CancelRequest только по TCP без TLS)
[Finalizer](../../runtime/src/sql/pg.ts#L149-L162) ждёт socket end, не backend. Pool=8 не предотвратил 39 backend и 53300; восстановление — после sleepers. Нужны server-side cancellation/deadlines, admission control и явная commit ambiguity. Rollback не обещан.

### 7.2. Новые воспроизведённые дефекты Q17–Q28

Ниже — срез `4a96533`, JS/debug и JS/release; пункт Q24 — checker.

**Q17. P1 — Sync-композиция генерирует невалидный JS** ✅ (операнды Sync-вызова в генераторе, где есть `yield*`, становятся константами до `$rt.sync`; `tests/run/ui/sync.fl`)
Runtime-вызов `read_more (Atom.current count)`, где `read_more : Number ->{Atom.Read;} Number`, собирается, но импорт падает с SyntaxError. [Wrapper](../../flick-core/src/codegen/js/expr.rs#L178-L200) помещает `yield*` в обычную стрелку `() => read_more(yield* …)`.

Вычислять callee/arguments до wrapper. Контроли с отдельными let работают; ошибка блокирует весь модуль.

**Q18. P1 — Atom.Read запускает Eff-значение** ✅ (ABI задаёт провайдер: JS-реализация Sync-сервиса помечена `syncAbi` и отдаёт значение как есть, у `S.Impl` из Flick эффект последней стадии исполняется; `tests/run/ui/sync_data.fl`, `runtime/test/sync.test.ts`)
`Atom.make (task { …; 41 })` через derived/Atom.value выполняет задачу при чтении; await полученного Number даёт TypeError. Atom.current сохраняет Eff холодным.

[syncOperation](../../runtime/src/sync.ts#L36-L46) путает ABI и данные через `Effect.isEffect(result)`. Нормализовать ABI провайдера, не значение. Чтение не должно выполнять скрытые сервисы/ошибки.

**Q19. P1 — Lazy Seq выпускает закрытый reader** ✅ (ленивый `for` с чтением атомов держит регион reader функции, которая его строит: выход результатом, замыканием или в атом — G001/G002 с подсказкой `[for …]`; потребители `Seq` (`to_list`, `fold`, `any`, `all`, `find`, `first`, `each_effect`, `fold_effect`, `from_seq`) принимают `(Seq a)^d`; хелпер, возвращающий читающий `Seq`, теперь ошибка; `tests/check/{fail/regions,pass/effects}/reader.fl`)
Derived atom возвращает `for n <- [1] yield Atom.value count + n`. Seq, полученный внутри provide Atom.layer, разрешено обойти снаружи; результат — `registry is disposed`. Внутренний обход и строгий List дали `[8]`.

[Captures](../../flick-core/src/types/comprehension.rs#L100-L115) не включают lifetime неявного [reader context](../../runtime/src/sync.ts#L59-L75). Утечку нужно отвергать, не продлевать жизнь registry.

**Q20. P1 — await/scope обходят Sync-only for** ✅ (`runtime_allowed` — общая проверка стадии для runtime-вызова, `await`, `scope` и `provide`; `tests/check/fail/effects/sync_for.fl`)
В `{Atom.Read;}` принимаются `for n <- [1] yield await task { Effect.never () }` и `for n <- [1] yield scope { n }`. Оба падают с AsyncFiberError; await перед for работает.

[call_effects](../../flick-core/src/types/effects.rs#L184-L199) проверяет стадию, но await/scope вызывают perform напрямую. Применять ограничение ко всем исполняющим Runtime конструкциям, как требует D7.

**Q21. P1 — Option ? нарушает выведенный тип** ✅ (несовпадение результата, навязанного `?`, с телом — T003; `tests/check/fail/types/question.fl`)
`value : Number; let value = (fun () -> (None : Option Number)? + 1) ()` принимается, экспортируя None вместо Number. [function_with](../../flick-core/src/types/infer.rs#L357-L378) игнорирует неудачную unify результата, навязанного `?`, с телом. Нужна ошибка типов; корректное тело с Some возвращает None.

**Q22. P1 — and принимает скаляр как контейнер** ✅ (первый операнд проверяется через `expect`, как остальные, после второго, который задаёт контейнер; `tests/check/fail/types/applicative_do.fl`)
`do { x <- 1 and y <- Some 2; pure (x + y) } : Option Number` даёт None. Обычный `map2 (fun x y -> x + y) 1 (Some 2)` отвергается с T003; корректная группа — Some 3.

[Первая unify](../../flick-core/src/types/applicative_do.rs#L71-L81) игнорирует ошибку. Проверять первый operand как остальные: sugar не обходит типобезопасность.

**Q23. P1 — and удаляет локальный pure** ✅ (`pure`, связанный паттернами группы, не сахар; `tests/run/codegen/applicative_do.fl`)
`do { pure <- Some (fun n -> Some (n + 10)) and x <- Some 1; pure x }` даёт Some 1; последовательные `<-` — Some 11.

[prelude_pure вызывается до binding patterns](../../flick-core/src/types/applicative_do.rs#L62-L66), принимая локальную функцию за sugar. Разрешать callee в правильном lexical scope.

**Q24. P2 — Квалификация pure требует лишний Monad** ✅ (`Prelude.pure` разрешается через квалификатор модуля, скобки вокруг `pure e` прозрачны; `tests/run/codegen/applicative_do.fl`)
Для `both : Applicative f => f Number -> f Number -> f Number` терминальный `pure (x+y)` после and принимается; `Prelude.pure (x+y)` или `(pure (x+y))` дают C001: нужен Monad f. Validation намеренно не Monad.

[Сравнивается текст](../../flick-core/src/types/applicative_do.rs#L214-L227). Распознавать resolved method и прозрачные скобки, сохраняя shadowing.

**Q25. P2 — Конечный range перестаёт продвигаться** ✅ (`seqRangeTo`/`seqRangeBy`/`seqFrom`/`listRange` кончаются на достигнутом конце или на шаге, который не меняет число; `runtime/test/seq.test.ts`)
`Seq.take 3 (9007199254740992..9007199254740992)` дал три копии вместо одной; `[7..7]` корректен. Полный сбор большого диапазона не запускался.

В [seqRangeTo](../../runtime/src/core/seq.ts#L26-L29) `i++` не меняет этот Number. Завершать достигнутый endpoint и обрабатывать непродвигающийся шаг.

**Q26. P1 — Fusion выносит return None из callback** ✅ (лямбда с собственным `o?` не встраивается в цикл, а связывается и вызывается — та же проверка `exits`, что у beta-редукции; `tests/run/codegen/fusion.fl`)
`Seq.map (fun x -> Some ((if x == 2 then None else Some x)? + 1)) (for x <- [1,2,3] yield x)` даёт в debug `[Some 2,None,Some 4]`, в release `[Some 2]`. С Seq.to_list внутри функции release возвращает None вместо List.

[Function::of](../../flick-core/src/opt/fuse.rs#L39-L68) переносит Unwrap через границу функции. Оставить callback-вызов, переиспользовав exits-защиту beta-reducer.

**Q27. P2 — Fractional take меняется в release** ✅ (контракт — усечение, как у `List.take`: `seqTake`/`seqDrop` и предел fusion берут `Math.trunc(n)`, NaN — ноль; `tests/run/codegen/fusion.fl`)
`Seq.take 1.5 (for x <- [1,2,3,4] yield x)` отдаёт четыре элемента в debug, два в release; 0.5 — четыре против одного. Целые, NaN/Infinity, порядок take/filter совпали.

[Runtime](../../runtime/src/core/seq.ts#L87-L95) ждёт `--left === 0`, [fusion](../../flick-core/src/codegen/js/generator.rs#L90-L98) — `++count >= n`. Определить единый контракт count; публичный Number не требует целого.

**Q28. P1 — Fusion меняет eager evaluation и defects** ✅ (строгий `[for …]` сливается с потребителем, только если его тело или callback не может бросить — `droppable` по источникам, условиям и элементам; ленивые цепочки как были; `opt/tests.rs`)
`Seq.first (for x <- [for y <- [1n,0n] yield 1n/y] yield x)` меняется с Division by zero на Some 1n. `List.map (fun x -> 2n ** (0n-x)) [for y <- [1n,0n] yield 1n/y]` меняет ошибку деления на `undefined must be positive`.

[nested](../../flick-core/src/opt/fuse.rs#L204-L228) делает строгий source ленивым; [List fusion](../../flick-core/src/opt/fuse.rs#L138-L158) смешивает producer/callback. Pure не total: сохранить eager-порядок либо доказать безопасность перестановки.

### 7.2a. Регрессии checker'а регионов после `3353662`

Срез `3353662` (переписан checker захватов) оставил красными 11 check- и 9 e2e-фикстур; исправлено в коде: заблокированная capture-строка растёт, когда другой нет (снова G001 по записанной границе); значение, чей тип не подошёл, сообщается один раз — типом; T003/T007/T008 печатают типы без выведенных квалификаторов; инстанцированная `^c` — хвост, который может остаться пустым (абстрактные типы и функции в `Ref` снова проходят); типы унифицируются раньше квалификаторов; аргументы kind `ErrorRow` не считаются захватами; foreign-колбэк разворачивается только по чистым стадиям (`Ui.stateful`); аннотированный параметр rank-2 колбэка сохраняет захват позиции; подсказка G001 для переменной типа не просит невозможный квалификатор. `ref_capacity`/`qualifiers` — новое поведение верно, фикстуры обновлены; `rpc/local` ждёт старта обработчика вместо таймера.

Запись с замыканием над ячейкой неизвестного ещё типа, переданная туда, где захватов нет, до решения типа (`logger.fl`, `Logger.batched` с `o` из третьего аргумента): внутри `subsume` квалификаторы, отличающиеся лишь захватами нерешённого типа, сравниваются в конце объявления (`regions.deferred`); `tests/check/{pass,fail}/regions/late_captures.fl`, фикстура `logger.fl` без аннотаций.

### 7.2b. Архитектура и границы решения: Q29–Q36

Срез `98c4092`: восемь дефектов воспроизведены реальными CLI/LSP/Node-пробами; проверки и ограничения — §7.6. Все пункты ниже открыты, перенос в родмап не исправляет код.

**Архитектурное решение:** сохранить общий Rust checker/IR/emitter для CLI, LSP и WASM, версионированные `.iface` и адаптеры к Effect. Исправлять контракты в существующих слоях: владение fibers, входы/выходы сборки, канал ошибок RPC, семантические идентичности IDE/API и владение diagnostics. Второй runtime, новый backend или переписывание компилятора из этих результатов не следуют.

**Q29. P1 — FiberSet прерывает children после освобождения ресурсов** ✅ (`owned` регистрирует `clear` коллекции в первой фазе закрытия владельца (`owner.closing`), так что дети FiberSet/FiberMap/FiberHandle прерываются и дожидаются до любого ресурса; закрытый владелец новых задач не запускает — `inOwner` прерывает их сразу; `runtime/test/fiber-collections.test.ts` с барьером старта для всех трёх: `["interrupt: released=false","release"]`, завершённый ребёнок не прерывается, после закрытия задача не стартует; без исправления три теста падают)
- Наблюдение: создать FiberSet, затем ресурс, дождаться старта child и закрыть owner → `["release","interrupt: released=true"]` вместо `["interrupt: released=false","release"]`.
- Причина/изменение: [inOwner](../../runtime/src/stdlib/fibers.ts#L11-L24) предоставляет scope, но не регистрирует native child в первой фазе [Owner.close](../../runtime/src/owner.ts#L62-L67). Включить children коллекций в interrupt-and-wait до закрытия ресурсов, удалять завершённые, не принимать новые после close; сохранить native supervision.
- AC: детерминированная проверка с барьером старта, без sleeps; ресурс жив до завершения child. Проверить общий путь FiberSet/FiberHandle/FiberMap, в том числе позднее приобретение ресурса, завершение child и закрытый owner. В ревью исполнен только FiberSet.

**Q30. P1 — build --out-dir . уничтожает package.json проекта** ✅ (до поиска в кэше и любой записи `build` отвергает каталог вывода, равный корню проекта, каталогу исходников или тестов или лежащий над ними, — `FLICK-P028`; пути сравниваются после разрешения символических ссылок; `flick/tests/build_command.rs`: `.`, `..`, `src` отвергнуты, `package.json` побайтно цел, сборка в `out` и `clean` работают)
- Наблюдение: `flick build --out-dir .` заменяет исходный manifest на `{ "type": "module", "sideEffects": false }`; последующий `flick clean` удаляет его. Обе команды завершаются успешно.
- Причина/изменение: [emitter](../../flick-core/src/build/mod.rs#L434-L450) всегда генерирует package.json, а [CLI](../../flick/src/commands/build.rs#L186-L198) пишет его в выбранный каталог. До любой записи проверять пересечение выходов с входами/метаданными проекта, включая канонические алиасы; отвергать корень проекта как out-dir, не объявлять исходный manifest собственным артефактом.
- AC: исходный manifest побайтно сохранён, отказ с диагностикой до частичной записи и регистрации outputs; обычная сборка в отдельный каталог и clean её артефактов работают.

**Q31. P1 — Сохранение файла во время сборки отравляет cache key** ✅ (перед записью в кэш `build` сверяет каждый прочитанный компилятором вход с байтами на диске (`cache::read_unchanged` по `SourceMap`); если что-то изменилось, сборка не кэшируется и следующая собирает заново; тест `cache::tests::a_source_saved_after_it_was_read_keeps_the_build_out_of_the_cache` — компиляция, затем правка Helper, затем проверка; барьер — сам порядок шагов, без хуков в бинаре)
- Наблюдение: Helper изменён с `1` на `2` во время сборки; результат — `1`, следующая сборка получает cache hit и снова `1`; после очистки cache — `2`.
- Причина/изменение: [build](../../flick/src/commands/build.rs#L101-L144) после компиляции заново хеширует живую FS для записи cache. Ключ должен соответствовать байтам, фактически прочитанным компилятором: использовать тот же снимок либо не публиковать cache, если соответствие не подтверждено. Просто сохранить предварительный key недостаточно.
- AC: детерминированный барьер чтения входов, затем изменение зависимости; следующая сборка не считает старый артефакт актуальным, cache/cold дают одинаковый результат. Одного редактирующего разработчика достаточно для воспроизведения.

**Q32. P1 — Ошибка запуска Rpc.spawn аварийно завершает Node** ✅ (приобретение ждёт первого `spawn` или `error` ребёнка: ошибка запуска — `ClientError` «cannot start … ENOENT», пайпы закрыты, слушатели сняты; прерывание во время запуска убивает ребёнка; поздняя ошибка запущенного ребёнка закрывает соединение, а не процесс; `tests/run/stdlib/rpc/stdio.fl`: отсутствующий helper — `ClientError ENOENT`, программа продолжает работу, обмен с живым helper прежний)
- Наблюдение: отсутствующий helper → exit 1, `Unhandled 'error' event` / `ENOENT`, а не типизированный `ClientError`.
- Причина/изменение: [rpcSpawn](../../runtime/src/rpc/server.ts#L163-L181) оборачивает синхронный spawn, но ошибка ChildProcess приходит асинхронно. Acquisition должен дождаться spawn/error, отдать ошибку через ClientError и корректно снять listeners/закрыть pipes; пустой error-listener не исправляет контракт.
- AC: запуск отсутствующего executable возвращает ClientError, родительский процесс остаётся жив, ресурсы очищены; успешный запуск и обмен с helper сохраняются.

**Q33. P2 — Cache не учитывает импорты рядом с внешним корневым файлом** ✅ (`cache::input_files` добавляет файлы корня-соседа каждого выбранного файла вне корней проекта; корень считает общая `inputs::sibling_root` по имени модуля из заголовка, как `--inputs` и загрузчик; `flick/tests/build_inputs.rs`: изменение, удаление и новое создание Helper не дают попадания в кэш)
- Наблюдение: `scratch/External.fl` импортирует соседний Helper вне настроенного src; после изменения только Helper с `1` на `2` повторный `flick build scratch/External.fl` отдаёт cache hit и `1`, cold build — `2`. Конкурентное сохранение не требуется: это не Q31.
- Причина/изменение: [cache inputs](../../flick/src/cache.rs#L68-L85) не включают sibling import root, поддержанный loader. Переиспользовать [поиск внешних корней](../../flick/src/commands/inputs.rs#L46-L70), чтобы loader, `--inputs` и cache согласованно учитывали входы.
- AC: изменение, создание и удаление sibling dependency инвалидируют cache; результат и диагностики cached/cold сборок совпадают.

**Q34. P2 — Rename метода класса ломает корректную программу** ✅ (`renamable` — общий для prepareRename и rename — отказывает для метода класса (`Decl::Method`) с сообщением «a method of a class: renaming it is not supported yet»; вызов метода по-прежнему ведёт к выбранному instance; `ide::tests::rename_changes_every_use_and_refuses_what_has_no_exact_mapping` — отказ и на сигнатуре, и на вызове)
- Наблюдение: stdio LSP принимает rename `Pretty.render` → `display`, но возвращает только правку сигнатуры класса; instance и вызов остаются `render`. Применение edit даёт C011 и T001.
- Причина/изменение: [rename](../../flick-core/src/ide/rename.rs#L18-L35) собирает ссылки по navigation identity, а выбранная реализация — InstanceMethod, не entity класса. Минимальное безопасное решение — отказ для Decl::Method в eligibility; не менять переход к выбранному instance ради rename.
- AC: prepareRename и rename отказывают для неподдерживаемого метода; обычный rename значения обновляет объявления/вызовы, go-to-definition метода по-прежнему открывает выбранную реализацию. Полная поддержка требует отдельной идентичности ссылок на метод класса.

**Q35. P2 — API diff считает удаление публичного instance PATCH-изменением** ✅ (`api_diff` сравнивает собственные instance модуля по голове и множеству ограничений контекста, не по месту в исходнике: удаление и усиление контекста — MAJOR, новый instance и ослабление контекста — MINOR, перестановка — ничего; `pack` предупреждает через тот же `diff::breaking`; `api_diff::tests::a_removed_instance_or_a_stronger_context_is_major`: удаление `Show Thing` в 1.2.3 — `2.0.0`)
- Наблюдение: удаление `Show Thing` при неизменных exports → `flick diff HEAD`: `none`, PATCH `1.2.3 -> 1.2.4`; прежний consumer с `show x` после этого не проходит check (C001).
- Причина/изменение: [api_diff](../../flick-core/src/api_diff.rs#L49-L62) сравнивает exports, но не instances. Сравнивать видимые instance heads и contexts по смыслу, не source positions; удаление/усиление контекста считать breaking.
- AC: diff требует MAJOR для этого примера 1.x; ужесточение context тоже breaking, перенос без семантических изменений — нет. [pack](../../flick/src/commands/pack.rs#L84-L91) использует ту же классификацию для предупреждения. В ревью исполнены diff и consumer check, путь предупреждения pack только прочитан.

**Q36. P2 — Закрытие standalone-документа стирает ошибки другого открытого** ✅ (закрытие и refresh контекста опустошают файл, только если ни один другой живой контекст его не публикует; иначе анализ того контекста (по его открытому документу) публикует файл заново; `flick/tests/lsp_command/isolation.rs::closing_a_standalone_document_keeps_what_another_open_one_reports`: refresh A, закрытие несвязанного C и закрытие A сохраняют T003 открытого B, закрытие B его очищает; без исправления тест падает)
- Наблюдение без flick.toml: A импортирует B с ошибкой типов; didOpen(A), didOpen(B) публикуют T003 для B, didClose(A) публикует `[]`, хотя B остаётся открытым и ошибочным.
- Причина/изменение: [закрытие/refresh контекста](../../flick/src/lsp/mod.rs#L404-L476) очищает URI без учёта других publishers. Не очищать URI, пока им владеет живой контекст; при необходимости обновлять его. Один порядок владения для didClose и очистки устаревших URI при refresh.
- AC: перекрывающиеся import closures сохраняют диагностики открытого B после закрытия A; закрытие последнего владельца очищает их. Проверить также refresh и непересекающиеся standalone-контексты.

### 7.3. Приоритеты и проверка закрытия

По новому ревью сначала **Q29–Q32 (P1)**, затем **Q33–Q36 (P2)**. Прежние открытые P1 сохраняют приоритет. Q17–Q28 уже имеют отметки выполнения: не переоткрывать их по историческим наблюдениям; при изменении соответствующих границ сохранять AC type safety/стадии/debug=release.

C1 уже реализован; сохранить B-бюджеты и измерять compiled consumers: C1/D14 microbenchmarks — ручные JS-модели. Сверить устаревшие вехи §4 с R5/U9/L6 и D-треком; добавить SQL/cancellation AC. Не строить второй Effect runtime.

### 7.4. PostgreSQL и нагрузка: сохранённый baseline

**Исторические измерения `3f1c7c3`, не срез `4a96533` и не новый прогон при переносе.** Локально: M4 Max/16 CPU/64 GiB, Node 24.18.0, Effect 4.0.5; JS/release (`flick build … --emit js --profile release`). PostgreSQL 17.11, `postgres:17-alpine`, Docker 4 CPU/2 GiB, loopback, fsync/synchronous_commit=on, max_connections=40, pool=8. Production не использовался.

- Точный `tests/postgres/app.fl` + schema/stdout: 15 строк совпали; CRUD, joins/NULL, BigDecimal, constraint 23514, DecodeError при drift. Injection-looking параметр остался данными, таблица цела.
- Отдельно: 512 конкурентных atomic increments одного ключа (c=32) дали 512 разных результатов и итог 512. Частичный commit/чужой rollback — **Q9**, отмена/overload — **Q16**.
- HTTP Schema echo, HTTP SQL `UPDATE value=value+1 RETURNING value` (ключ на worker), unary RPC через Flick `Rpc.call`. HTTP-генератор — fetch; разные клиенты, не сравнение транспортов.
- Closed-loop, без retries, deadline 5 с: прогрев каждого пути 10 с при c=16; затем по 3×10 с для каждой пары путь/c; SQL soak 60 с. Клиент/сервер на одном Mac. Таблица: медианы трёх прогонов, не pooled percentiles; soak — один.

| Путь | c | req/s | p50, мс | p95, мс | p99, мс |
|---|---:|---:|---:|---:|---:|
| HTTP echo | 1 | 657 | 1.48 | 1.93 | 2.23 |
| HTTP echo | 16 | 8258 | 2.00 | 2.44 | 4.46 |
| HTTP echo | 64 | 20271 | 2.73 | 6.16 | 7.08 |
| HTTP SQL | 1 | 217 | 4.51 | 5.96 | 7.57 |
| HTTP SQL | 16 | 2521 | 5.90 | 8.50 | 12.75 |
| HTTP SQL | 64 | 5041 | 11.63 | 22.28 | 27.12 |
| RPC echo | 1 | 580 | 1.69 | 2.20 | 2.88 |
| RPC echo | 16 | 5009 | 2.32 | 7.57 | 9.29 |
| RPC echo | 64 | 4727 | 10.48 | 18.58 | 87.75 |
| HTTP SQL, 60 с | 64 | 4619 | 11.76 | 26.89 | 37.36 |

Штатный профиль, включая прогрев/soak: **1 842 275** ответов без status/schema/transport ошибок и таймаутов; проверены id. Все **532 648** успешных инкрементов совпали с изменением БД; в soak — 277 270, максимум 8 наблюдавшихся DB connections. Разброс SQL — 2.1×; локальный ориентир, не production capacity/SLO.

| Память Node-сервера, MiB | До soak | Пик (1 Гц) | Через ~38 с idle |
|---|---:|---:|---:|
| RSS | 252.4 | 331.5 | 207.0 |
| heapUsed | 118.8 | 199.5 | 94.5 |

Soak: server CPU ≈0.36 ядра; отсутствие утечек не доказано. SIGTERM во время `pg_sleep(3)`: ответ 200, процесс завершился за 2.97 с (exit 143), listener закрыт, DB sessions=0. Аварийный abort-профиль **Q16** не включён в «ошибок нет».

Abort baseline: 6×64 запросов, pool=8/DB max=40; 263 отмены, 121 HTTP 500/53300 и два 500 обычного bump. После sleepers сервис восстановился.

### 7.5. Проверка среза ревью

Проверки ниже выполнены в сессии ревью на `4a96533`, не при переносе в ROADMAP.

`cargo build -p flick`, `pnpm -C runtime build` прошли. Fixtures `codegen/{question,applicative_do,ranges,fusion}`, `stdlib/monoids`, `ui/{sync,sync_for}` из tests/run: JS/debug, JS/release, TS/debug — **21 исполнение**, stdout совпал. `examples/mtl` run дал три ожидаемых результата; property suite не запускался.

Одноразовые consumers подтвердили **Q17–Q23,Q25–Q28** в обоих JS-профилях; **Q24** — C001. Chromium/JS-release: три клика → count 3, lift 12, Seq `[4,5,6]`; browser errors нет, interruption удалил root.

Не проверены полный suite, TS/release, v3 host, comparative benchmarks и повторная PostgreSQL-нагрузка. Старые browser/HTTP/TS/SQL-доказательства — baseline. Временные пробы удалены в сессии ревью; тогда изменён только отчёт.

### 7.6. Проверка архитектурного среза `98c4092`

Выполнено в сессии архитектурного ревью, не при переносе в родмап:
- `cargo build -p flick` и `target/debug/flick --project examples/first-slice run` прошли; пример вывел загруженных пользователей, `Ada Lovelace and Grace Hopper` и `not found: "u3"`.
- Реальные CLI build/cache/clean и Node imports подтвердили Q30/Q31/Q33; CLI diff и consumer check — Q35. Две stdio LSP-сессии подтвердили Q34/Q36; правки rename применены и проверены компилятором.
- Node-пробы на текущем `runtime/src` и установленном Effect подтвердили Q29/Q32. Проверка требуемого порядка FiberSet намеренно упала; это доказательство дефекта, не успешный regression test.

Среда: macOS arm64, Cargo 1.94.1, Node 24.18.0, pnpm 12.14.0. Разрушительные сценарии выполнялись на одноразовых файлах вне репозитория; временные пробы удалены.

Не проверены полный Rust/TS suite, lint, полная корректность checker/optimizer, browser/WASM и UI, SSR под нагрузкой, реальные PostgreSQL/RPC deployments, Windows/Linux, длительный рост heap LSP, конкурентные publishers и публикация пакетов. Ограничения прежнего ревью (§7.5) сохраняют свой исторический контекст. Код найденных дефектов этим обновлением не изменён.
