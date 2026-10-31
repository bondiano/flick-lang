# Дыры TypeScript и что с ними во Flick

Сверено с кодом на `0d6016d5`. Каталог перечисляет известные дыры TypeScript: места, где TS принимает программу, а значение в рантайме не совпадает с типом. Для каждой записано, чем она закрыта во Flick.

Каждый пример на TS проверен на TypeScript 5.9.3 с `--strict`: он компилируется и в Node даёт указанный результат.

Ссылки:

- `core.md`, `regions.md` — модель;
- `spec/08` — `docs/src/content/docs/spec/08-typescript-and-ffi.md`;
- `reference/ffi` — `docs/src/content/docs/reference/ffi.md`.

Импортёр устроен в два шага:

1. `tools/dts-importer/src/` переводит `.d.ts` в формы (`Shape.fl`);
2. `flick-core/src/ffi/` переводит формы в привязки Flick (`convert.rs` `ty`, `import.rs` `generate`).

## 0. Как закрыта дыра

| Класс | Что значит | Чем подтверждается |
| - | - | - |
| ядро | эквивалент на Flick не выводится по правилам модели | правило `core.md`/`regions.md` и отрицательная фикстура `tests/check` |
| граница | форма TS не доходит до Flick как есть: импортёр её отвергает или даёт непрозрачный тип, кодоген копирует данные | функция импортёра или кодогена, тест импортёра или run-тест |
| гипотеза | остаётся на доверенной привязке | часть гипотезы Г1 (`core.md` §4) |

Одна дыра бывает закрыта в двух классах. Тогда её эквивалент на Flick отвергает ядро, а форма из `.d.ts` останавливается на границе.

Фикстуры:

- `tests/check/fail/types/ts_holes.fl` — по форме на определение; ниже на неё ссылаются по имени определения.
- `tests/check/projects/ts_holes_errors` — ошибки с одинаковым именем в двух модулях.
- `tests/ffi/ts-holes` — пакет `.d.ts` с формами для импортёра. Тесты к нему: `tools/dts-importer/test/import.test.mjs` и `flick-core/src/ffi/tests.rs`.

## 1. Правило границы: внешний объект непрозрачен

Объект, интерфейс или экземпляр класса из `.d.ts` импортируется как непрозрачный тип, а не как запись Flick:

- Именованная форма становится `type Name` без конструкторов и полей: «A JS `Name`, opaque to Flick» (`convert.rs` `ty`, `import.rs` `generate`).
- Анонимная форма в результате становится `JsValue`. В параметре импортёр отвечает «needs a checked adapter».
- Экспорт класса отвергается: конструктор и методы привязываются руками.

Члены объекта импортёр не читает: `Shape.fl` `object_shape` сводит класс и объект к имени. Доступ к свойству — только явная привязка `foreign` с `this`, то есть вызов со своей стадией, а не чтение поля записи. Данные из внешнего объекта Flick получает в два шага:

1. `Js.snapshot : JsValue ->{; SnapshotError} Unknown` копирует граф и отвергает аксессоры, функции, чужие прототипы и циклы (spec/08 «`Unknown` и `JsValue`»);
2. `Schema.validate` проверяет полученное значение.

spec/08 записывает это так: «A simple TS object shape stays by default a foreign shape with runtime access until a checked snapshot/adapter; it does not become an immutable source record because of `readonly`».

У непрозрачного типа нет полей, поэтому Flick не видит spread, аксессоров, `readonly` у членов и индексных сигнатур. Это закрывает на границе дыры §2.9–2.11 и §2.13.

Данные самого Flick (`List`, кортеж, запись, ADT) непрозрачными не становятся. Как они пересекают границу, записано в Р26 (§3).

## 2. Каталог

### 2.1 `any`

```ts
const n: any = "42";
const m: number = n;
m.toFixed(1); // TypeError: m.toFixed is not a function
```

- Во Flick нет ни `any`, ни верхнего типа: у ≤ есть только ≤-Refl, ≤-Fun и ≤-Eff (`core.md` §2.2).
- Внешнее значение неизвестного типа — непрозрачный `JsValue`. Его можно только передать во внешний код или в `Js.snapshot`.
- `any` и `unknown` из `.d.ts` становятся `JsValue` (`convert.rs` `ty`). При `strict = true` импортёр их отвергает.
- Закрыта: ядро и граница.
- Тесты:
  - `ts_holes.fl` `any_as_string` — `FLICK-T003` «expected `String`, found `JsValue`»;
  - `ffi/tests.rs` `any_pure_and_cancellation_are_never_implied` и `strict_mode_refuses_unknown`.

### 2.2 `as T`

```ts
type Cat = { name: string; meow(): string };
const c = { name: "x" } as Cat;
c.meow(); // TypeError: c.meow is not a function
```

- Приведения во Flick нет.
- `(e : σ)` — это `let x : σ = e in x` (`core.md` §1.4). Его типизируют (Let-Sig) и (Sub). Аннотация может расширить стадию стрелки или строки `Eff`, но не может сменить тип.
- Закрыта: ядро (§2.2–2.3).
- Тест: `ts_holes.fl` `cast` — `(1 : String)` даёт `FLICK-T003`.

### 2.3 `as unknown as T`

```ts
const n = "42" as unknown as number;
n.toFixed(1); // TypeError: n.toFixed is not a function
```

- `Unknown` во Flick не верхний тип: по (Sub) в него не переходит ни одно значение.
- Значение `Unknown` создаёт только `Js.snapshot`. Выйти из него можно только через `Schema.validate : Schema a -> Unknown -> Result SchemaError a`, то есть с проверкой в рантайме.
- Закрыта: ядро.
- Тест: `ts_holes.fl` `double_cast` — `FLICK-T003` на обеих аннотациях.

### 2.4 `!`

```ts
const m = new Map<string, number>();
const v: number = m.get("k")!;
v.toFixed(1); // TypeError: Cannot read properties of undefined
```

- `null` и `undefined` во Flick нет: отсутствие значения — `Option`.
- Снять `Option` можно только `match`, а он обязан быть исчерпывающим (Exh, `core.md` §2.4).
- Опровержимый `let Some n = v` — `FLICK-T009`.
- `let assert Some n = v` при несовпадении — явный дефект. Это исход `die`, который допускает T1, а не значение чужого типа.
- Закрыта: ядро.
- Тест: `tests/check/fail/types/matching.fl` `refutable` (`FLICK-T009`).

### 2.5 Лгущий предикат `x is T`

```ts
function isNumber(x: unknown): x is number { return true; }
const s: unknown = "a";
if (isNumber(s)) s.toFixed(1); // TypeError: s.toFixed is not a function
```

- `if` и `match` по значению `Bool` не меняют тип переменной.
- Тип меняет только паттерн конструктора. Тип связанной переменной берётся из объявления конструктора (`core.md` §2.4), а не из обещания функции.
- Предикат из `.d.ts` импортёр не читает. Тип возврата `x is T` для TS — `boolean`, и привязка возвращает `Bool`. `asserts x is T` даёт `Unit`.
- Закрыта: ядро и граница.
- Тесты:
  - `ts_holes.fl` `guard` — после `if is_point value` поле `value.name` даёт `FLICK-T003`;
  - тест импортёра «reduces a type predicate to its boolean».

### 2.6 Бивариантность параметров методов

```ts
class Ev { constructor(readonly type: string) {} }
class MouseEv extends Ev { constructor(readonly clientX: number) { super("mouse"); } }
interface Handler { handle(e: Ev): string }
const h: Handler = { handle(e: MouseEv) { return e.clientX.toFixed(1); } };
h.handle(new Ev("key")); // TypeError: Cannot read properties of undefined
```

- Подтипы во Flick есть только у стрелок и `Eff`. Параметр стрелки контравариантен (≤-Fun, `core.md` §2.2; `effects.rs` `subsume`).
- Методов, которые сравнивались бы иначе, у записей нет. Метод инстанса сверяется с сигнатурой класса как написан (`FLICK-C010`).
- Аналог во Flick: функцию, которая полагается на чистый колбэк, ставят туда, где ей передадут эффектный.
- Закрыта: ядро.
- Тест: `ts_holes.fl` `pure_callback` — `(Unit -> Unit) ->{} Unit` за `(Unit ->{} Unit) ->{} Unit` даёт `FLICK-T003`.

### 2.7 Ковариантные изменяемые массивы

```ts
class Animal { constructor(readonly name: string) {} }
class Dog extends Animal { bark() { return "woof"; } }
class Kitten extends Animal { meow() { return "meow"; } }
const cats: Kitten[] = [];
const animals: Animal[] = cats;
animals.push(new Dog("rex"));
cats[0].meow(); // TypeError: cats[0].meow is not a function
```

- `List` неизменяем. Изменяемая ячейка — только `Ref`, и `Ref` инвариантен: внутри `Ref`, `List`, записей и ADT типы сравниваются по ≡ (`core.md` §2.2, Р8).
- На границе `T[]` и `readonly T[]` из `.d.ts` становятся `List T`. Массив, который пересекает границу, копируется (Р26).
- Закрыта: ядро и граница.
- Тесты:
  - `ts_holes.fl` `covariant_ref` — `Ref (Unit -> Unit)` за `Ref (Unit ->{} Unit)` даёт `FLICK-T003`;
  - тест импортёра «gives a readonly array the shape of a mutable one»;
  - run-тест Р26.

### 2.8 Подтипы по ширине

```ts
type Point2 = { x: number; y: number };
const p3 = { x: 1, y: 2, z: "far" };
const p2: Point2 = p3; // лишние свойства проверяются только у литерала
const total: number = Object.values(p2).reduce((s, v) => s + v, 0);
total; // "3far"
```

- Записи Flick точные. (Rec) даёт запись ровно с данными полями, а вложения строк полей нет (`core.md` §1.2: «подтипов по ширине нет»).
- Лишнее поле — `FLICK-T003`, и в литерале, и через переменную.
- Открытый хвост `{ name : String | r }` — полиморфизм, а не подтип: функция получает запись целиком, в своём типе.
- Закрыта: ядро.
- Тесты:
  - `ts_holes.fl` `width_via_variable` — через переменную;
  - `tests/check/fail/types/records.fl` `extra` — в литерале.

### 2.9 Spread

```ts
class Temperature { #c = 20; get celsius() { return this.#c; } }
interface Temp { readonly celsius: number } // так `.d.ts` описывает геттер
const t: Temp = new Temperature();
const copy = { ...t };          // тип { celsius: number }
copy.celsius.toFixed(1);        // TypeError: Cannot read properties of undefined
```

Тип `{...a}` одинаков для перечисляемого собственного свойства, неперечисляемого и аксессора на прототипе. Значение получает только первое, а прототип теряется.

- Spread во Flick нет. `{ r with ℓ = e }` есть только у записей Flick, а у них только собственные поля данных (spec/08 «Value representation»: «a plain JS object with the exact set of own data fields»).
- Внешний объект — непрозрачный тип (§1). `with` и чтение поля к нему не применимы.
- Закрыта: граница.
- Тесты:
  - `ts_holes.fl` `spread_handle` — `FLICK-T003` «`Handle` has no field `name`; `with` only replaces existing fields»;
  - тест импортёра «reads no members of a class».

### 2.10 Свойство только с сеттером

```ts
class Box { v = 0; set value(x: number) { this.v = x; } }
const x: number = new Box().value;
x.toFixed(1); // TypeError: Cannot read properties of undefined
```

- Доступа к свойствам внешнего объекта во Flick нет (§1). Привязка с `this` — это вызов функции, которую написал человек, а не чтение аксессора.
- `Js.snapshot` отвергает аксессоры.
- Закрыта: граница.
- Тесты:
  - `ts_holes.fl` `field_of_handle` — `FLICK-T003` «`Handle` has no field `value`»;
  - тест импортёра «reads no members of a class»: класс с геттером, сеттером и индексной сигнатурой даёт только `handle`.

### 2.11 `readonly` и другая ссылка

```ts
const frozen: { readonly n: number } = Object.freeze({ n: 1 });
const loose: { n: number } = frozen; // readonly свойства не участвует в совместимости
loose.n = 2; // TypeError: Cannot assign to read only property 'n'
```

- Данные Flick неизменяемы. Изменяемо только `Ref`, и это видно по типу.
- Вида «только для чтения» над `Ref` нет. В ядре нет формы, которую надо было бы отвергать, поэтому отрицательной фикстуры здесь нет.
- Внешний код получает данные Flick копией (Р26), поэтому другая ссылка из JS их тоже не меняет.
- Закрыта: ядро и граница.
- Тест: run-тест Р26.

### 2.12 Необязательное свойство против `| undefined`

```ts
type Opts = { retries?: number };
const o: Opts = { retries: undefined }; // без exactOptionalPropertyTypes
"retries" in o ? o.retries!.toFixed(0) : "none"; // TypeError
```

- `undefined` во Flick нет. Поле типа `Option T` обязательно: пропуск — `FLICK-T003`. Различия «нет ключа» и «нет значения» не возникает.
- На границе параметр `x?: T` и параметр `x: T | undefined` оба становятся `Option T` с пометкой `optional i`. `None` передаёт явный `undefined` при той же арности (`runtime/src/core/data.ts` `optionalArg`). Функция, которая различает пропущенный аргумент и `undefined` (`arguments.length`, `in`), видит одно и то же. Это гипотеза Г1.
- Результат `T | undefined` импортируется только по политике `undefined = "none"`.
- Закрыта: ядро; на границе — гипотеза.
- Тест: `tests/check/fail/ui/children.fl` `panel_no_header` — пропущенное поле типа `Option`.

### 2.13 Индексная сигнатура возвращает `T`

```ts
const scores: Record<string, number> = {};
const s: number = scores["ann"]; // без noUncheckedIndexedAccess
s.toFixed(1); // TypeError: Cannot read properties of undefined
```

- Поиск по ключу и по индексу возвращает `Option`: `HashMap.get`, `List.get`, `Record.get_key`, `String.at`. Это типы констант Γ₀.
- Индексные сигнатуры импортёр не читает. `Record<K, V>` вне поддерживаемого подмножества.
- Закрыта: ядро и граница.
- Тесты:
  - `ts_holes.fl` `index_lookup` — `FLICK-T003` «expected `Int`, found `Option Int`»;
  - тест импортёра «leaves an index signature outside the supported subset».

### 2.14 Структурно равные ошибки с одинаковым `_tag`

```ts
class StoreMissing { readonly _tag = "NotFound"; constructor(readonly key: string) {} }
class CacheMissing { readonly _tag = "NotFound"; constructor(readonly key: string) {} }
function fromStore(): StoreMissing { return new CacheMissing("k"); }
fromStore() instanceof StoreMissing; // false
```

Различить их в TS может только ручной бренд: поле `declare protected readonly brand: "StoreMissing"`, уникальное для класса.

- Элемент строки ошибок определяется именем `Module.Name` (`core.md` §1.2).
- Конструкторы двух модулей — разные значения. Их `_tag` — имя и 48 бит FNV-1a от id пакета, модуля, типа и конструктора (spec/08 «Value representation»).
- Ветвь `A.NotFound _` снимает из строки только `App.A.NotFound`.
- Закрыта: ядро.
- Тесты:
  - `tests/check/projects/ts_holes_errors` — `FLICK-E002`: `App.B.NotFound` остаётся в строке;
  - `flick-core/src/codegen/tests.rs` `constructor_ids_name_package_module_and_type` — теги различаются между модулями и пакетами.

## 3. Находки

Нумерация продолжает `regions.md`.

Каждая находка подтверждена пробой на `0d6016d5`: программа принята, а значение в рантайме расходится с типом.

**Р26. Данные Flick пересекают внешнюю границу по ссылке.**

- spec/08 «Value representation» обещает: «Immutable data is copied by default at an untrusted input and output boundary».
- Кодоген передаёт `List`, кортежи, записи и ADT как есть, в обе стороны (`codegen/js/foreign.rs` `foreign_call`).
- Проба — `tests/run/ffi/copy.fl` на Node:

  ```text
  argument [1, 2, 3], result [1, 2, 3]     JS отсортировал аргумент на месте
  held result [1, 2, 99]                   JS изменил массив, который вернул раньше
  record mallory ["a", "b"]                JS изменил запись
  callback result [1, 2, 9], JS saw 3      JS изменил то, что вернул колбэк
  ```

  Неизменяемое значение Flick меняется. JS может так же положить в `List Float` строку, и это нарушает T1.
- Решение — кодоген, как обещает spec/08.
  - Недоверенная граница — каждый `foreign` вне `@flick-lang/runtime`, включая `global`.
  - Копируются аргументы, результат (у `async` — разрешённое значение), аргументы и результат колбэка.
  - Копия глубокая по типу: `List`, кортеж, запись, ADT.
  - Как есть передаются примитивы, `JsValue`, непрозрачные типы и функции.
  - Доверенная граница (рантайм) не копирует. Для неё это гипотеза Г1.
- Статус: решено, не внесено.

**Р27. `typeof C` импортируется как сам `C`.**

- `Shape.fl` `object_shape` даёт `handle` любому типу с символом класса. Статическая сторона класса, то есть конструктор, получает то же имя, что экземпляр.
- Проба: `register(ctor: typeof Point): void` становится `register : Point ->{} Unit`, и `register (make_point 1)` проходит проверку. JS получает экземпляр вместо класса.
- Решение — импортёр: тип с construct-сигнатурами и символом класса — `other`, то есть отказ.
- Статус: решено, не внесено.

**Р28. Два TS-типа с одним простым именем — один тип Flick.**

- Непрозрачные типы в сгенерированном модуле различаются только простым именем (`import.rs` `generate`, множество `handles`).
- Проба: `retry(options: a.Options)` и `logOptions(): b.Options` дают один `type Options`, и `retry (log_options ())` проходит проверку.
- Решение — импортёр. Он находит простые имена, за которыми стоят разные объявления, и блокирует экспорты, которые их называют. Привязка для них пишется руками, с разными именами. Список таких имён — поле поверхности рядом с экспортами, так что отпечатки экспортов не меняются.
- Статус: решено, не внесено.

**Остаётся гипотезой Г1.** На это верится на слово, и тесты этого не проверяют:

- результат `never` (`Never`);
- параметричность дженериков: `.d.ts` её не доказывает (spec/08);
- примитивный результат: `number` не проверяется в рантайме;
- различие пропущенного аргумента и `undefined` (§2.12);
- отпечаток экспорта: у именованного типа это только имя, поэтому изменение его членов `ffi check` не видит.

**Вне каталога, не исправлено:**

- `T | void` в результате требует политики (`convert.rs` `result`), но `check.rs` `required` не требует `nullable` у проверенной привязки;
- имя из пакета вроде `Option` или `String` объявляется в сгенерированном модуле и затеняет имя прелюдии. Обычно это громкая ошибка типов, а не тихая;
- пустой кортеж `[]` становится `Unit`, а значит JS получает `undefined`.
