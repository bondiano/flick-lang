# ReScript: компоненты React и их кастомизация (C7a)

Записка к пункту C7a [ROADMAP](ROADMAP.md): как ReScript объявляет компоненты, что из этого генерирует
компилятор и как пользователи подменяют рендерер. Хуки, `useState`/`useEffect1` и цитата про
eslint уже разобраны в [typed-ui-prior-art §2](typed-ui-prior-art.md); здесь не повторяются.

Источники, только первичные, ничего не запускалось (компилятора ReScript в окружении нет):

- Компилятор `rescript-lang/rescript`, `master` на коммите `5689a30531da7ab9700bd4464ec735f637312dae`
  (2026-10-25, `package.json` = `13.0.0-alpha.7`). Читался shallow-клоном.
- Биндинги `rescript-lang/rescript-react` на коммите `bb34644ca803e021e7218a8a63b572705df73701`
  (`0.16.0-beta.1`, тот же коммит, что в prior-art).
- Документация rescript-lang.org: исходники `rescript-lang/rescript-lang.org` на коммите
  `e8579252fae7993b7c8ab02328871e1fe6ed81e0`, живые страницы проверены (HTTP 200) 2026-10-25.

Ключи цитат:

- `rs:X` — `https://github.com/rescript-lang/rescript/blob/5689a30531da7ab9700bd4464ec735f637312dae/X`.
  `ppx:` = `rs:compiler/syntax/src/`, `rt:` = `rs:packages/@rescript/runtime/`,
  `ppxt:` = `rs:tests/syntax_tests/data/ppx/react/` (вход `F.res`, ожидаемый результат PPX — `expected/F.res.txt`),
  `se:` = `rs:tests/build_tests/super_errors/` (`fixtures/F.res` и `expected/F.res.expected`).
- `rr:X` — `https://github.com/rescript-lang/rescript-react/blob/bb34644ca803e021e7218a8a63b572705df73701/X`.
- `d:/X` — `https://rescript-lang.org/X`; исходник страницы — `apps/docs/markdown-pages/X.mdx` в
  `rescript-lang.org@e857925`. `sl:X` — `apps/docs/markdown-pages/syntax-lookup/X.mdx` там же.
- **НЕ ПРОВЕРЕНО** — вывод, который источник не подтверждает напрямую.

Оговорка о версиях: `master` компилятора — v13 alpha, а сайт описывает v12. Где они расходятся,
это сказано явно (§3.3).

## 1. `@react.component`: что генерирует JSX v4 PPX

### 1.1 Интерфейс компонента: модуль с `props` и `make`

- «A ReScript React component needs to be a (sub-)module with a `make` function and `props` type to be
  usable in JSX» `d:/docs/react/beyond-jsx`. Декоратор даёт «`props`: A generated record type that has
  fields according to the labeled arguments of the `make` function» и «`make`: … `(props) => React.element`» (там же).
- Тип props — запись, **параметризованная по каждому полю**: `type props<'x, 'y> = {x: 'x, y: 'y}`, а
  аннотации аргументов уходят в ограничение `make` (`props<string, string>`), не в тип
  `ppxt:expected/v4.res.txt` L1–L10. Поэтому props полиморфны там, где аргумент не аннотирован
  (`props<_>`, там же L18).
- Тип помечается `@res.jsxComponentProps` (там же L1); по этому атрибуту тайпчекер узнаёт props
  компонента и печатает JSX-ошибки (§6) `rs:compiler/ml/error_message_utils.ml` L885, L900–L911.
- Результат PPX (вход — `ppxt:v4.res` L1–L3):

  ```rescript
  @res.jsxComponentProps
  type props<'x, 'y> = {x: 'x, y: 'y}
  let make = ({x, y, _}: props<string, string>): React.element => React.string(x ++ y)
  let make = React.component({
    let \"V4" = (props: props<_>) => make(props)
    \"V4"
  })
  ```

  `ppxt:expected/v4.res.txt` L1–L10.
- Обёртка — именованный `let` с именем `Файл$Модуль` (или `Файл$Модуль$имя`, если функция не `make`):
  `make_module_name` склеивает файл, вложенные модули и имя через `$` `ppx:jsx_v4.ml` L82–L93,
  комментарий «React component name should start with uppercase letter» L636. Пример имени
  `V4$AnotherName$anotherName` `ppxt:expected/v4.res.txt` L35. Документация: «will be named `File$Nested`
  in dev tools»; для своего имени — `React.setDisplayName(make, "…")` `d:/docs/react/components-and-props`
  (раздел о `@react` decorators и именовании).
- `React.component` — это `external component: componentLike<'props, element> => component<'props> =
  "%component_identity"` `rr:src/React.res` L16, `rt:Jsx.res` L26–L27: приведение без кода. Порядок
  «сначала функция, потом приведение» выбран ради инференса полиморфных props: приведение
  `%component_identity` не-экспансивно, и тип генерализуется `ppx:jsx_v4.ml` L669–L684.
- `Jsx.component<-'props>` абстрактный и **контравариантный** по props: «If one component can accept
  broader props, it can safely stand in for a component that only needs narrower props» `rt:Jsx.res`
  L20–L24. Абстрактным его сделали в v13 (`rs:CHANGELOG.md` L219, PR #8390, раздел 13.0.0-alpha.5).

### 1.2 Выходной JS: обёртка исчезает

Вход (`rs:tests/tests/src/jsx_optional_props_test.res` L3–L12) и вывод компилятора
(`…/jsx_optional_props_test.mjs` L5–L17):

```rescript
module ComponentWithOptionalProps = {
  @react.component
  let make = (~i as _: option<int>=?, ~s as _: option<string>=?, ~element as _: option<React.element>=?) => React.null
}
let _element = <ComponentWithOptionalProps i=1 s="test" element={<div />} />
```

```js
function Jsx_optional_props_test$ComponentWithOptionalProps(props) {
  return null;
}
let ComponentWithOptionalProps = {
  make: Jsx_optional_props_test$ComponentWithOptionalProps
};
let _element = JsxRuntime.jsx(Jsx_optional_props_test$ComponentWithOptionalProps, {
  i: 1, s: "test", element: JsxRuntime.jsx("div", {})
});
```

- Два `make` сливаются в одну JS-функцию с именем `Файл$Модуль`; props — обычный JS-объект, поля
  читаются как `props.x`; элемент — прямой вызов `react/jsx-runtime` (`jsx` для 0–1 ребёнка, `jsxs` для
  нескольких) `d:/docs/react/beyond-jsx`.
- То же в документации: `function Example$Friend(props) { return JsxRuntime.jsxs("div", {children:
  [props.name, props.children]}) }` `d:/docs/react/beyond-jsx`.

### 1.3 Необязательные props и значения по умолчанию

- `~a=2` даёт необязательное поле `a?: 'a`, а в теле `make` — `switch __a { Some(a) => a | None => 2 }`;
  умолчание может ссылаться на предыдущий prop (`~b=a*2`) `ppxt:defaultValueProp.res` L1–L4,
  `ppxt:expected/defaultValueProp.res.txt` L1–L24.
- В JS это проверка на `undefined` без аллокаций: `let __a_value = __a !== undefined ? __a : 2;`
  `rs:tests/tests/src/alias_default_value_test.mjs` L4–L10.
- `~name: option<string>=?` — необязательный prop без умолчания. На месте вызова `?name` передаёт
  `option` как есть, а `name=v` передаёт значение `d:/docs/react/components-and-props` (Optional Props).
  `Some("Andrea")` в JS — просто `"Andrea"` (там же, JS Output).
- `@react.component` неявно добавляет последний `()` `d:/docs/react/components-and-props`.

### 1.4 `key`, `ref`, `children`

- `key` не попадает в тип props: он отфильтрован при построении параметров типа `ppx:jsx_v4.ml` L100
  и на месте вызова (`(* key should be filtered out *)` L1149–L1153). JSX с `key` вызывает отдельную
  функцию `React.jsxKeyed(V4CA.make, {}, ~key="k", ())` `ppxt:expected/noPropsWithKey.res.txt` L32–L33.
- Аргумент `~key` в компоненте — ошибка: «Key cannot be accessed inside of a component. Don't worry -
  you can always key a component from its parent!» `ppx:jsx_v4.ml` L252–L255.
- `ref` в React 19 — обычный prop (`~ref`); `React.forwardRef` оставлен для старого кода
  `d:/docs/react/forwarding-refs` (врезка «React 19»), `d:/docs/react/beyond-jsx` (Ref Props).
- `children` — обычный prop со своим типом: `React.element`, `string`, `option<React.element>=?`
  или отсутствует вовсе. Документация советует моделировать структуру props'ами, а не детьми:
  «all components end up being a `React.element`» `d:/docs/react/components-and-props` (Children Props).

### 1.5 Ограничения формы

- Один компонент на модуль: «Only one component definition is allowed for each module. Move to a
  submodule or other file if necessary.» `ppx:jsx_common.ml` L79–L82.
- Props — только именованные аргументы: «React: props need to be labelled arguments.» `ppx:jsx_v4.ml`
  L450–L453.
- Деструктуризация привязки запрещена («JSX component calls cannot be destructured.» L58–L65);
  декоратор работает только на функции или на обёртке `forwardRef`/`memo` (L400–L408).
- `async`-компоненты (Server Components) оборачиваются в `Jsx.promise` `ppx:jsx_common.ml` L84–L90.

## 2. `@react.componentWithProps`

- Для функции, которая сама принимает запись props: «annotate functions that are React components which
  take in a record type as prop. This decorator will ensure your component gets an uppercased name.»
  `sl:decorator_react_component_with_props`. Пример: `type props = {name: string}` и
  `@react.componentWithProps let make = (props: props) => …` компилируется в
  `function Example$Hey(props) {…}` (там же).
- Что меняется в PPX: тип props **не генерируется** (возвращается `None` вместо `props_record_type`),
  остаётся только именованная обёртка `Файл$Модуль` и `React.component` `ppx:jsx_v4.ml` L818–L917.
  Значит, нет умолчаний, нет `option`-развёртки и нет автоматического полиморфизма по полям: всё это
  пользователь пишет сам.
- Зачем: полный контроль над типом props (`d:/docs/react/components-and-props`, «full control over the
  props type»), общий тип props у нескольких компонентов, обход `memoCustomCompareProps` (§4.1).
  Вариант с общим типом есть и у `@react.component(: props)`: `ppx:jsx_common.ml` L53–L57,
  тесты `ppxt:sharedProps.res`, `ppxt:sharedPropsWithProps.res`.
- Запреты: с `React.forwardRef` — «Components using React.forwardRef cannot use
  @react.componentWithProps. Use @react.component instead.» `ppx:jsx_v4.ml` L849–L851,
  `se:react_component_with_props`; для `external` — «Components cannot be defined as externals when
  using @react.componentWithProps … use the type React.component<props> instead.» L963–L970.
- Появился в 12.0.0-alpha.6 (`rs:CHANGELOG.md` L1188, PR #7203).

## 3. Generic JSX transform: свой JSX-модуль

### 3.1 Конфигурация

- `rescript.json`: `"jsx": {"version": 4, "module": "Preact"}`; также `"preserve": true` оставляет JSX в
  выводе. Схема: `version` ∈ {4}, `module` — «Either "react" for React, or (since v11.1) any valid module
  name», `additionalProperties: false` `rs:docs/docson/build-schema.json` L360–L378.
- Переопределение на файл: `@@jsxConfig({module_: "Preact"})`, удобно при смешении фреймворков
  `d:/docs/manual/jsx` (Generic JSX transform). `@jsx.component` — синоним `@react.component` (там же).
- Механизм — чисто синтаксический: PPX подставляет имя модуля в путь, `Module.value`
  (`module_access_name` `ppx:jsx_v4.ml` L7–L9). Для строчных тегов он берёт `ReactDOM` при `"react"` и
  `Module.Elements` иначе (L1214–L1221, L1272–L1273). «The `Preact` module can be defined by anyone -
  locally in your project, or by a package» `d:/docs/manual/jsx`.

### 3.2 Что модуль обязан дать

По документации (пример `Preact.res`), «everything needs to be there unless explicitly noted or the
transform will fail at compile time» `d:/docs/manual/jsx`:

| Имя | Сигнатура (Preact-пример) |
|---|---|
| `element`, `component<'props>`, `componentLike<'props,'return>` | алиасы `Jsx.*` |
| `jsx`, `jsxs` | `(component<'props>, 'props) => element` |
| `jsxKeyed`, `jsxsKeyed` | `(component<'props>, 'props, ~key: string=?, @ignore unit) => element` |
| `array` | `array<element> => element` (обязательно; остальные `null/float/int/string/promise` — опционально) |
| `fragmentProps`, `jsxFragment` | `{children?: element}`, `component<fragmentProps>` |
| `Elements.props` | тип props строчных тегов, обычно `JsxDOM.domProps`, «you can make this anything» |
| `Elements.jsx/jsxs/jsxKeyed/jsxsKeyed` | `(string, props, …) => Jsx.element` |
| `Elements.someElement` | `element => option<element>` |

- Функции не обязаны быть `external`: тест компилятора задаёт `MyJsx` обычными функциями поверх
  `react/jsx-runtime` (`let jsxKeyed = (c, p, ~key=?, ()) => …`)
  `rs:tests/build_tests/jsx_preserve_semantics/cases/CustomFn.res` L1–L30. Контракт — имена и типы, а не
  JS-модуль.
- Минимальный набор для конкретного кода меньше: тестовые фикстуры объявляют только `element`,
  `component`, `jsx` и `jsxFragment` `se:fixtures/jsx_custom_component_optional_prop.res` L5–L18.
  **НЕ ПРОВЕРЕНО**, что PPX требует имена лениво, по мере использования; так выглядит по коду, но
  полного прохода я не делал.

### 3.3 Расхождение docs и v13

- В v13 PPX оборачивает каждый компонент в `Module.component(...)` (`jsx_component_expr`
  `ppx:jsx_v4.ml` L52–L55, L684), а значит, JSX-модуль должен дать и `component`. Пример `Preact.res` на
  сайте его не содержит (в `docs/manual/jsx.mdx` нет ни `component_identity`, ни `external component`).
  Тестовые модули v13 его объявляют (`CustomFn.res` L6–L7). Пример с сайта под v13 скорее всего не
  скомпилируется (**НЕ ПРОВЕРЕНО**, компилятор не запускался).

### 3.4 Другие рендереры

- Официально задокументирован только Preact-пример `d:/docs/manual/jsx`. Сторонние библиотеки поверх
  generic transform я по первичным источникам не проверял (см. «Не проверено»).

## 4. `memo`, `forwardRef`, хуки

### 4.1 `React.memo` и `React.memoCustomCompareProps`

- Оба — `external` к `react`'s `memo`: `memo: component<'props> => component<'props>` и
  `memoCustomCompareProps: (component<'props>, ('props, 'props) => bool) => component<'props>`
  `rr:src/React.res` L87–L94. Сравнение по умолчанию — то, что делает React (поверхностное по props).
- `@react.component let make = React.memo((~a: int, ~b: string) => …)` — PPX пробивается через
  применение обёртки `ppx:jsx_v4.ml` L463–L470. В выводе документации функция при этом **анонимная**:
  `let make = React.memo((props) => JsxRuntime.jsxs("div", …))` `d:/docs/react/memo`. То есть имя
  `Файл$Модуль` под `memo` теряется (по выводу на сайте; компилятор не запускался).
- «`React.memoCustomCompareProps` cannot be combined with `@react.component`». Обходы: переопределить
  `make` вторым `let` (`let make = React.memoCustomCompareProps(make, (p1, p2) => p1.disabled ==
  p2.disabled)`) или написать props-тип руками без декоратора `d:/docs/react/memo` (arePropsEqual).
  Функция сравнения получает типизированные записи props.
- Сам React: «memoization is a performance optimization, not a guarantee» `d:/docs/react/memo`.

### 4.2 `forwardRef` и `ref`

- `forwardRef: (('props, nullable<ref<'a>>) => element) => component<'props>` `rr:src/React.res` L84–L85.
  PPX передаёт `ref` вторым аргументом и делает `make({...props, ref: …})` `ppx:jsx_v4.ml` L636–L667.
- Рекомендация: «we strongly recommend **passing your ref as a prop**»; `forwardRef` помечен
  «[Discouraged]» `d:/docs/react/forwarding-refs`.

### 4.3 Правила хуков

- Не проверяются компилятором; предлагается eslint-plugin-react-hooks по скомпилированному JS
  `d:/docs/react/hooks-overview` (Rules of Hooks). Подробно — в [prior-art §2](typed-ui-prior-art.md).
  В PPX нет проверок на хуки: в `ppx:jsx_v4.ml` нет ни одного упоминания `use`-функций (проверено
  grep'ом по `useState|useEffect|hook` — пусто). **НЕ ПРОВЕРЕНО**, что такой проверки нет в других
  частях компилятора (анализатор, `reanalyze`).

## 5. Типы DOM-props и событий

- Props строчных тегов — одна большая запись `JsxDOM.domProps` (~600 строк) с необязательными полями:
  `key?: string`, `children?: Jsx.element`, `ref?: domRef`, `className?: string`, ARIA через
  `@as("aria-label") ariaLabel?: string`, варианты-перечисления вроде
  `ariaPressed?: [#"true" | #"false" | #mixed]` `rt:JsxDOM.res` L21–L66, L132, конец записи L622.
  `ReactDOM.domProps = JsxDOM.domProps`, и `ReactDOM.jsx: (string, JsxDOM.domProps) => Jsx.element`
  `rr:src/ReactDOM.res` L229, L245–L254.
- Следствие: тип не зависит от тега. `<input>` и `<div>` принимают один и тот же набор полей; проверка
  «какие атрибуты у какого тега» не делается (вывод по одной записи на все теги; **НЕ ПРОВЕРЕНО** на
  компиляторе).
- Обработчики типизированы классом события: `onClick?: JsxEvent.Mouse.t => unit`,
  `onChange?/onInput?: JsxEvent.Form.t => unit`, `onKeyDown?: JsxEvent.Keyboard.t => unit`
  `rt:JsxDOM.res` L270–L284. `Mouse.t = synthetic<tag>` с фантомным `tag`, геттеры — `@get external`
  (`altKey`, `button`, `clientX` …) `rt:JsxEvent.res` L134–L162.
- Цель события не типизирована: `target: synthetic<'a> => {..}` (открытый объект) `rt:JsxEvent.res`
  L33, L65. Документация: «we cannot anticipate the target type on the event. So, we need a leap of faith
  to grab the input value as string»: `let value: string = target["value"]` `d:/docs/react/events`.
- Имена-ключевые слова — с подчёркиванием (`type_`), дефисные — через `@as` в своём `domProps`
  `d:/docs/react/components-and-props`, `d:/docs/manual/jsx` (Hyphens in tag names).
- Через generic transform можно задать свой `Elements.props`, чтобы получить другие атрибуты, и
  редактор будет их подсказывать `d:/docs/manual/jsx` (комментарий в `Preact.res`).

## 6. Ошибки компиляции для props

Тексты — из `se:expected/*.expected` (ANSI-коды убраны). Тайпчекер отличает props компонента по
`@res.jsxComponentProps` (§1.1); тексты собраны в `rs:compiler/ml/error_message_utils.ml` L913–L937 и
`rs:compiler/ml/typecore.ml` L5309.

| Ситуация | Сообщение | Фикстура |
|---|---|---|
| Лишний prop | `The prop test does not belong to the JSX component` | `component_invalid_prop` |
| Нет обязательного | `The component is missing these required props:` + список (`name`) | `component_missing_prop` |
| Дети компоненту без `children` | `This JSX component does not accept child elements. It has no children prop <Wrapper />` | `missing_required_prop` |
| Prop дважды | `The prop name has already been passed to the component` / `You can't pass the same prop more than once.` | `component_prop_passed_multiple_times` |
| Не тот тип | `This has type: array<int>` / `But the component prop someProp is expected to have type: float` | `wrong_type_prop_punning` |
| `option` в необязательный prop | `someOpt is an optional component prop, and you're passing an optional value to it.` … `prepend the value with ? … like: someOpt=?o` | `jsx_custom_component_optional_prop` |
| Строка вместо элемента | `But children of JSX fragments must be of type: React.element` … `You can convert string to a JSX element with React.string.` | `jsx_type_mismatch_string` |
| Тело не элемент | `But it's expected to have type: Jsx.element` … `with React.int.` | `react_component_missing_jsx_element` |
| Опечатка в поле без JSX | `The field nonExistant does not belong to type Component.props` + полный тип `Component.props<string, …>` | `wrong_name_component_prop` |

- Три фикстуры (`component_invalid_prop`, `component_missing_prop`, `…_multiple_times`) не включают PPX:
  «Since the React transform isn't active in the tests, mimic what the transform outputs». Они
  проверяют сообщение тайпчекера на уже развёрнутой записи `props`.
- Последняя строка таблицы показывает цену параметризованного props: без JSX-контекста ошибка
  печатает `props<string, string, …>` на девять параметров.
- Ошибки PPX (§1.4, §1.5, §2) — синтаксические, до типов: `key`, позиционные props, два компонента в
  модуле, `componentWithProps` + `forwardRef`.

## 7. Что переносим во Flick

Сопоставление с C7b–C7d и текущим API (`Ui.view`, `Ui.render`, `Ui.named`, `Ui.Renderer`,
`Ui.mount_with` в `flick-core/src/types/builtins/Ui.fl`; `uiView`/`uiNamed` и `memo(render)` с
`displayName = view.label` в `runtime/src/ui/react/views.ts` L39, L57, L119–L120).

1. **Синтаксис — ключевое слово, desugar тонкий (C7b).** ReScript показывает, что «компонент» — это
   только соглашение: функция `props => element` плюс имя и тип props; всё остальное — обычный код
   (§1.1–1.2). `component name (props : P) = setup … render …` должен разворачиваться в одно
   выражение, без собственного рантайм-слоя на экземпляр, кроме того, что требует `Setup s` (C7d).
2. **Props — одна запись `P`, объявленная пользователем (C7b/C7c).** Это модель
   `@react.componentWithProps`, а не `@react.component`: тип props пишется явно, ошибки в нём называют
   поля, а не `props<'a, 'b, …>` (§6, последняя строка). Необязательные поля и умолчания — через
   обычные `Option` в записи и разбор в `setup`/`render`; специального синтаксиса `~a=2` не нужно.
   Сообщения об ошибках взять по образцу §6: «prop X не принадлежит компоненту `App.search`», «не
   хватает props: …», «компонент не принимает детей», с именем компонента из метки.
3. **`key` — не prop.** Как в ReScript (§1.4), ключ остаётся метаданными узла: во Flick это уже
   `Ui.keyed`. В `P` поле `key` запретить или просто не трактовать особо; в теле компонента ключ
   недоступен по построению.
4. **Имена даёт компилятор (C6 → C7d).** ReScript называет саму JS-функцию `Файл$Модуль` во время
   компиляции и не тратит на это рантайм (§1.1–1.2). Flick уже даёт `$rt.label("Mod.name", …)` и
   ставит `displayName`; для `component` метка `Module.name` берётся из имени после ключевого слова.
   Урок ReScript: имя теряется под обёрткой (`React.memo` даёт анонимную функцию, §4.1), значит, метку
   надо ставить на итоговый объект, который видит React, что Flick и делает (`made.displayName`).
5. **Адаптер рендерера = generic JSX module (`Ui.Renderer`, `Ui.mount_with`).** ReScript разделяет
   «компоненты» (`jsx`, `jsxKeyed`, `component`, `jsxFragment`) и «хост-элементы» (`Elements` со своим
   типом props) (§3.2). Для Flick это довод разделить адаптер на две части: (а) монтирование и
   компоненты — общие для всех хостов; (б) набор хост-примитивов с собственными типами props
   (`Ui.Dom.*`, нативный набор). Сейчас `mount_with` прямо пишет, что DOM-хелперы `Ui` строят DOM-узлы, а
   нативный адаптер «supplies its own components» — ReScript делает этот выбор типом `Elements.props`.
   Контракт адаптера стоит описать как интерфейс модуля Flick (сигнатура, проверяемая компилятором), а
   не как неявный набор имён, которые PPX подставляет в путь.
6. **`memo` — по умолчанию, сравнение — явное.** В ReScript memo — opt-in обёртка, а
   `memoCustomCompareProps` не сочетается с декоратором (§4.1). Во Flick каждый view уже обёрнут в
   `memo` (`views.ts` L119), а перерисовку по атомам даёт трекинг. Переносим только идею
   типизированного сравнения: если понадобится своё, это `Eq P` (или поле в `component`), а не
   произвольная функция на JS-объектах.
7. **Правила хуков — из типов, не из линтера (C7c).** ReScript их не проверяет (§4.3); Flick снимает
   вопрос структурно: `setup` выполняется один раз и не может быть условным (`Setup s` в строке),
   `render` — `{Atom.Read}` без записи. Это главный выигрыш перед ReScript, держать его в AC C7c.
8. **События — снимки, а не `{..}`.** ReScript оставляет `target` нетипизированным (§5). Flick уже
   передаёт инертные снимки (`ClickSnapshot`, `InputSnapshot`) в `Handler`; так и оставить для
   `component`.
9. **Нулевая цена в JS (трек B).** Ориентир из §1.2: компонент без `setup` должен компилироваться в
   одну функцию от props с прямым чтением полей; обёртки PPX ReScript полностью исчезают после
   инлайна. Хорошая фикстура для `just bundle-size`: `component` без `setup` против `Ui.view`.

## 8. Что НЕ переносим

- **Props из именованных аргументов и генерация `props<'a, 'b>`.** Даёт короткую запись, но тип с
  параметром на каждое поле, неочевидный полиморфизм и длинные сообщения (§6). Во Flick уже есть
  записи и `type alias`; второй способ объявить props не нужен.
- **«Один компонент на модуль» и имя `make`.** Это следствие того, что JSX ищет `Module.make`
  (§1.1, §1.5). У Flick ключевое слово с именем, и компонентов в модуле может быть сколько угодно.
- **Подстановка модуля по имени в синтаксисе (`"jsx": {"module": …}`, `@@jsxConfig`).** Контракт
  проверяется только тем, что сгенерированный код тайпчекается, а описание расходится с компилятором
  (§3.3). Во Flick адаптер — значение (`Ui.Renderer`) с типом; выбор рендерера — в точке
  `mount_with`, а не в конфиге сборки.
- **`forwardRef` и `ref` как prop.** Ref-API во Flick удалён (`5daf143`, ROADMAP §6.2); ReScript и сам
  его не рекомендует (§4.2).
- **Одна запись `domProps` на все теги.** Удобно для биндингов, но не ловит «атрибут не того тега»
  (§5). Flick уже даёт props на каждый примитив (`ButtonProps`, `InputProps`) плюс общий `Host`.
- **Нетипизированные события и `%identity`-конверсии (`React.string`, `React.int`).** Во Flick `Ui.text`
  и снимки событий; неявных приведений к элементу не нужно.
- **Правила хуков на совести пользователя.** См. §7.7: заменяются `Setup s`.

## Не проверено

- Ничего не компилировалось: компилятора ReScript нет в окружении. JS-вывод взят из тестов компилятора
  (`*.mjs`, `expected/*.txt`) и из вкладок «JS Output» на сайте.
- Что пример `Preact.res` с сайта не компилируется под v13 без `component` (§3.3) — вывод по коду PPX.
- Потеря имени `Файл$Модуль` под `React.memo` (§4.1) — только по выводу на странице `memo`.
- Что PPX требует имена JSX-модуля лениво, по мере использования (§3.2).
- Отсутствие проверок хуков вне `jsx_v4.ml` (анализатор, reanalyze).
- Сторонние рендереры поверх generic transform (Preact-биндинги, SSR-шаблонизаторы и т.п.): кроме
  примера на сайте и тестовых модулей компилятора ничего не читал.
- Что имена функций `Файл$Модуль` переживают минификацию в production-сборке: ReScript этого не
  обещает, а `setDisplayName` — ручной путь.
