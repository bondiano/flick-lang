---
title: "9. Schema и стандартные типы данных"
---

## Граница между ядром языка и библиотекой

Ядро задаёт pure/runtime, nominal types, HKT, classes, captures и полный deriving Schema. `Schema`, `Ref`, `Option`, `Result`, `Redacted`, `BigDecimal`, `Duration` и `Chunk` — библиотечные типы с нативным ABI Effect. В первой версии специальные literals есть только у обычного Number и BigInt; Schema/SQL/UI имеют выбранный DSL, а остальные модули используют обычные функции.

| Исходный тип | Представление в Effect и TS |
| - | - |
| `Eff r e a` | `Effect.Effect<A, E, R>` |
| `Layer ri e ro` | `Layer.Layer<RO, E, RI>` |
| `Option a` | `Option.Option<A>` |
| `Result e a` | `Result.Result<A, E>` |
| `Ref a` | `Ref.Ref<A>` |
| `Redacted a` | `Redacted.Redacted<A>` |
| `Codec rd re i a` | `Schema.Codec<A, I, RD, RE>` |
| `PureCodec i a` | Тот же Schema codec и проверенная исходная гарантия чистоты |
| `Schema a` | Чистое доменное свидетельство на основе `Schema.Codec<A, A, never, never>` |
| `BigDecimal`, `Duration` | Нативные значения одноимённых модулей Effect |

Порядок generic-параметров исходного API может отличаться от TS; это не требует переупаковки значения. Source-метаданные сохраняют kinds, rows и гарантии чистоты, которых нет в `.d.ts`.

| Возможность | Польза | Цена |
| - | - | - |
| Библиотечные combinators | Простое ядро и явные значения | Больше служебного кода |
| Типы и структурный deriving | Один источник структуры ADT и records | Генерация должна учитывать рекурсию, generics и неподдержанные поля |
| Refinement types с runtime-проверкой | Доменные ограничения видны в типах | Нужны безопасные конструкторы и правила доверенного FFI |
| DSL для внешнего формата | Переименования, defaults и transformations собраны рядом | Дополнительная грамматика и диагностика |
| Произвольные вычисления на уровне типов | Очень выразительные библиотеки | Существенное усложнение checker и времени компиляции |

Deriving охватывает весь поддерживаемый язык данных. Библиотечные Schema combinators и codec DSL остаются путями настройки и расширения. Для этого не требуется вводить произвольные type families.

## Тип значения и его внешний формат

Исходный тип описывает значения программы. Schema — runtime-значение, которое проверяет структуру и ограничения. Codec дополнительно описывает преобразование между внешним и внутренним представлением. У одного доменного типа могут быть разные codecs для HTTP, базы данных и конфигурации.

В [Effect v4](https://effect.website/docs/v4/schema/getting-started) общий тип codec имеет четыре параметра: `Schema.Codec<T, Encoded, DecodingServices, EncodingServices>`. `Encoded` означает внешний формат, а не канал ошибок. Раздельные требования decoding и encoding полезно сохранить во Flick.

Source-типы имеют decoded-результат последним. Это не обещает `Functor` для Codec: encoder нуждается в обратном преобразовании, поэтому используются `imap` или отдельные decoder/encoder. `rd` и `re` обозначают decoding/encoding services. Ожидаемая ошибка обоих направлений — `SchemaError`; доменная ошибка effectful transformation явно переводится в schema issue, а defect/interruption сохраняются.

```text
Schema a
PureCodec encoded decoded
Codec decoding_services encoding_services encoded decoded

Codec.decode :
  forall c ci rd re i a.
  (Codec rd re i a)^c ->
  (Unknown^ci ->{rd; SchemaError} a)^c

Codec.encode :
  forall c rd re i a.
  (Codec rd re i a)^c -> (a ->{re; SchemaError} i)^c

Codec.decode_result :
  forall c ci i a.
  (PureCodec i a)^c ->
  (Unknown^ci -> Result SchemaError a)^c

Codec.encode_result :
  forall c i a.
  (PureCodec i a)^c -> (a -> Result SchemaError i)^c
```

Captures codec сохраняются у частично применённой функции; provenance возвращаемого `a`/`i` проверяется отдельно и не стирается generic witness. SchemaError содержит безопасную диагностику, не живой исходный input, способный вынести scoped capability.

`PureCodec` — opaque сертификат чистого синхронного выполнения, а не alias по пустым service rows. `Codec none none i a` может выполнять async/runtime-действия. Сертификат создают проверенные pure combinators, deriving и явно trusted FFI. Внешняя Effect Schema без такого контракта получает общий Codec. Pure decoding принимает только source Unknown с инертным структурным представлением; произвольный JS сначала проходит runtime import из раздела 8.

`Schema a` в этом source API — тоже сертифицированное чистое доменное свидетельство; это более узкий контракт, чем произвольная внешняя Schema. `PureCodec` и `Schema` используют тот же runtime-объект Effect Schema, а дополнительная гарантия хранится в source-интерфейсе. Effectful Schema interpreters остаются доступны через runtime-стрелки. [Schema Effect v4](https://effect.website/docs/v4/schema/getting-started).

## Типы и Schema из одного объявления

Выбрана грамматика `refined type`, `deriving Schema`, `schema T` и `codec witness { ... }`. Refinement — nominal type с закрытым constructor, чистым predicate, безопасным `parse` и доступом к underlying value. Predicate возвращает Bool и не нормализует значение; false — validation failure, неожиданный defect остаётся defect. Trim/lowercase/parse находятся в codec transformation. `Checks.email` — конкретная библиотечная проверка, не обещание полной проверки всех email-адресов.

```flick
refined type Email = String where Checks.email
  deriving Eq, Hash, Schema

type alias Profile = {
  email : Email
  nickname : Option String
} deriving Eq, Hash, Schema

profile_schema : Schema Profile
let profile_schema =
  schema Profile

profile_json : PureCodec Json Profile
let profile_json =
  codec profile_schema {
    email = field "email_address" Email.json
    nickname = field "nickname" (option_as_null String.json)
  }

parse_profile : Unknown -> Result SchemaError Profile
let parse_profile input =
  Codec.decode_result profile_json input
```

В codec DSL каждое decoded-поле record задаётся ровно один раз. Конфликт wire-имён, неизвестное поле или пропущенное обязательное поле — ошибка компиляции. По умолчанию лишние входные ключи отвергаются; `ignore_extra_fields` явно строит проверенную проекцию без них. Для сохранения неизвестных полей требуется объявленное поле соответствующего типа. В примере `None` кодируется JSON null; missing, null и tagged Option не выбираются неявно по одному `Option a`. [Представления Effect Schema](https://effect.website/docs/v4/schema/effect-data-types).

### Политики Option и defaults

Политика поля задаётся codec DSL; её нельзя вывести только из `Option a`.

| Политика | Encode None | Decode |
| - | - | - |
| `field "x" codec` | По codec | Ключ обязателен |
| `field "x" (option_as_null codec)` | null | null означает None; missing — ошибка |
| `optional_field "x" codec` | Ключ отсутствует | missing означает None; присутствующее значение проверяет codec |
| `field "x" (option_as_nullish codec)` | Canonical null | missing/null сворачиваются в None; undefined только в явно выбранном JS-формате |
| Tagged Option codec | Явная ветка None | Проверяемый discriminant и payload |

`undefined` не входит в `Json`. Если нужно различать missing и null, используется отдельный ADT.

Если приложение различает missing и null, нужен отдельный ADT или иная явно более богатая модель, а не свёртка обоих в один `None`.

```flick
type alias Preferences = {
  nickname : Option String
  quota : Number
} deriving Schema

preferences_json : PureCodec Json Preferences
let preferences_json =
  codec (schema Preferences) {
    nickname = optional_field "nickname" String.json
    quota =
      field "quota" Number.json_finite
      |> on_missing (fun () -> 0)
  }
```

`optional_field` и `on_missing` — принятые имена DSL. Missing quota получает 0 ровно одним вызовом pure factory на соответствующий decode; null и неверное значение не заменяются default. Number.json_finite отвергает non-finite значение и при encode. Чтобы исключить его в домене, поле объявляют refinement-типом.

Constructor default, decode default и omission при encode — независимые политики. `on_missing` принимает pure factory; отдельный `on_missing_effect` добавляет её services в `rd`, а ожидаемые ошибки явно представляются SchemaError. Encode не опускает значение из-за равенства default без отдельной политики. Эффектный default не даёт PureCodec/Schema сертификат. [Effect constructor defaults](https://effect.website/docs/v4/schema/default-constructors).

### Эффектные преобразования codec

Для codec с поиском пользователя по ID тип может иметь вид `Codec {Directory} none UserId User`: decode требует `Directory`, encode получает ID из уже имеющегося User. Само построение значения codec чистое, выполнение decode — runtime. Возможность такого поиска не обещает round-trip, если состояние Directory меняется.

Effect поддерживает [effectful schema transformations](https://effect.website/docs/v4/schema/transformations). Язык сохраняет независимые требования decode/encode; пустые строки общего `Codec` не превращают его в `PureCodec`.

### Полноценный deriving Schema

`deriving Schema` — выбранная часть языка. Compiler строит runtime-свидетельство по типизированному определению, передаёт параметры явно и генерирует Effect Schema/combinators. Он не пытается восстановить стёртый тип из произвольного JS-значения.

| Форма данных | Что выводится |
| - | - |
| Примитивы, literals, `Unit`, `Never` | Проверка выбранного домена; `Never` не принимает значений |
| Records и точные tuples | Свидетельства полей/элементов, layout и согласованная политика лишних ключей |
| Номинальные ADT | Проверки discriminant и payload каждой ветки; согласование с ABI конструкторов |
| `Option`, `Result`, коллекции | Стандартные schema-конструкторы с явными свидетельствами параметров |
| Generic-типы | Функция от схем используемых параметров к схеме результата |
| Higher-kinded параметры | Явное полиморфное свидетельство построения схем для `f a` |
| Рекурсия и взаимная рекурсия | Ленивые ссылки внутри группы определений; без бесконечного разворачивания |
| Refinements | Чистый predicate и диагностика; нормализация отделена в codec |
| Opaque типы | Свидетельство, предоставленное владельцем типа; приватный constructor не раскрывается |
| Phantom-параметры | Нет фиктивной runtime-проверки того, чего нет в представлении |

```flick
type Tree a =
  | Leaf a
  | Branch (List (Tree a))
  deriving Schema

tree_schema : Schema a -> Schema (Tree a)
let tree_schema item_schema =
  schema Tree item_schema

type alias PairOf f = {
  count : f Number
  label : f String
} deriving Schema

pair_of_schema :
  (forall a. Schema a -> Schema (f a)) -> Schema (PairOf f)
let pair_of_schema make_schema =
  schema PairOf make_schema
```

`List` здесь — выбранный immutable source API над dense JS array. `make_schema` применяется при разных типах и использует higher-rank checking. `Schema a` — передаваемое значение-свидетельство, не скрытый class constraint.

Обычные рекурсивные данные конечны; shared поддеревья разрешены, цикл в активном пути decoding даёт SchemaError. Cyclic-graph codecs не входят в первую версию. Рекурсивные и взаимно рекурсивные schemas используют lazy references; нерегулярная рекурсия — lazy instantiation по фактическому параметру, без бесконечного eager разворачивания. Если такой контракт нельзя выразить конечным JSON Schema, strict exporter его отвергает.

Effect предоставляет [ленивые recursive и mutually recursive schemas](https://effect.website/docs/v4/schema/advanced-usage). Вывод по определениям Flick и проверка witness-параметров остаются работой compiler Flick.

### Полнота DSL и границы автоматизации

Полный DSL включает renaming, ADT discriminants, composition, pure/fallible/effectful transformations, refinements, defaults, whole-record checks и explicit witnesses. Он elaborates в типизированные библиотечные constructors. Неподдерживаемое поле deriving не пропускается молча: требуется witness или source diagnostic. Brand check функции, Ref или opaque generic не доказывает parametricity/тип содержимого. Snapshot mutable/resource данных остаётся runtime, captures opaque leaves сохраняются.

Полнота deriving означает отсутствие искусственного ограничения только простыми records. Она не означает, что из типа функции можно проверить parametricity, из resource handle — вывести сериализацию живого ресурса, а из `Redacted` — выбрать допустимость разглашения. Такие поля требуют явно заданного checked witness, opaque predicate или отдельного codec. Для `Ref` и ресурсов snapshot является runtime-операцией; секреты требуют объявленной политики.

Схема данных и wire-codec остаются разными значениями. Импортируемая Effect Schema с неизвестными эффектами получает общий `Codec`; сертификат чистоты автоматически не присваивается. Arbitrary, JSON Schema и другие внешние интерпретаторы имеют собственные условия применимости: например, произвольный predicate не даёт автоматически конструктивный генератор. Неподдерживаемый экспорт должен требовать override или давать точную диагностику, а не заменять обещанный тип универсальной JSON-схемой.

### Schema exporters и компиляция выполнения

Выбраны расширяемые schema-интеграции и строгая диагностика неподдерживаемого wire-контракта. Здесь есть две разные задачи.

**Экспорт и интерпретация schema.** В первую версию входят:

| Target | Зафиксированный контракт | Ограничение |
| - | - | - |
| JSON Schema Draft 2020-12 | Строгий export явно выбранного JSON wire-codec | Unsupported acceptance rule даёт ExportError с путём |
| Standard Schema V1 | Sync или Promise adapter после удовлетворения decoding services | Owner/cancellation и JS input import остаются явными |
| Arbitrary | Structural generator | Неподдержанный refinement требует явного generator |
| Diagnostic formatter | Structured issues, пути и redaction | Не раскрывает Redacted или arbitrary opaque payload |
| OpenAPI 3.1 | Строгий export значения `HttpApi`: пути, методы, параметры, тела, ответы, ошибки и security; схемы — тот же strict JSON Schema 2020-12 | Непредставимая schema endpoint'а даёт ExportError с путём `группа.endpoint -> поле` |
| Другие targets | Плагины после первой версии | Отдельные версии и capability matrix |

Native [JSON Schema generation](https://effect.website/docs/v4/schema/json-schema) допускает best-effort approximation. Strict exporter Flick проверяет точную выразимость выбранного wire-контракта: unsupported predicate, opaque формат, effectful acceptance или невыразимая рекурсия дают ExportError с node/path. Молчаливые `{}` и `any` запрещены. Явный override привязан к конкретному узлу. Более простой wire-shape witness можно экспортировать отдельно, но его нельзя назвать полным validation contract effectful codec.

Например: `Profile.session -> JSON Schema: scoped resource has no wire codec`. Исправление — codec, явная проекция данных либо исключение поля из выбранного внешнего представления. Для статически известных schemas это build-диагностика, для динамически построенных — типизированный результат export-операции.

[Standard Schema adapter Effect](https://effect.website/docs/v4/schema/standard-schema) требует удовлетворённых decoding services и допускает sync/Promise validation. Adapter Flick принадлежит owner предоставленных сервисов; late validate после закрытия возвращает контролируемую ошибку. Вызов с произвольным JS input — runtime boundary с выбранным import contract. PureCodec допускает sync path после безопасного импорта; общий Codec — Promise path. Универсальный runSync не используется.

**Оптимизация выполнения Effect Schema.** В v4 есть [JIT и AOT schema compilers](https://effect.website/docs/v4/schema/compilers). Они генерируют JS для поддерживаемой части AST и оставляют остальные узлы интерпретатору. AOT даёт статический модуль; API пока experimental.

Interpreter — обязательный baseline; AOT — opt-in build optimization первой версии. Неявный runtime JIT не включается. Unsupported AOT node остаётся интерпретируемым, если семантика сохраняется; это не разрешает approximation при wire export. Registry скомпилированных parsers устанавливается явной инициализацией entry point.

Fast path может повторять checks/getters на невалидном входе при построении подробной диагностики; чистые schema-оптимизации требуют согласованного инертного data ABI. Compiler Effect пропускает fast paths, способные повторить defaults, transformations или другие эффекты. Adapter Flick сохраняет эти условия.

```text
Email.parse : String -> Result SchemaError Email
Email.value : Email -> String
```

Константу можно проверить во время компиляции, если конкретная проверка поддерживается компилятором. Произвольное значение требует проверки при выполнении. Типы стираются при генерации JS; само наличие имени `Email` не проверяет внешний объект.

## Что не следует обещать через refinement

Проверка неизменяемого значения может обосновывать его номинальный тип. Проверка «пользователь существует в БД» зависит от состояния внешнего мира и не гарантирует существование после возврата результата. Такие операции остаются runtime-вызовами, при необходимости внутри транзакции.

Тип Codec не доказывает изоморфизм. Отдельный библиотечный контракт `lawful` фиксирует успешный поддомен, выбранную Eq и допустимую canonical wire-форму; законы проверяются тестами компонента, а не произвольным доказательством checker. Defaults, lookup и нормализация не получают round-trip автоматически.

Defaults, нормализация, частичные transformations и effectful lookup не получают round-trip закон автоматически. Односторонняя миграция, проекция или маскированный вывод могут предоставлять только decoder/encoder. Compiler проверяет типы и требования, но не доказывает произвольные пользовательские законы.

## Ref и обобщение типов

`Ref` удобно предоставить обычными библиотечными функциями. Язык должен понимать их runtime-стрелку и применять правило generalization из раздела 5.

```text
Ref.make : a ->{} Ref a
Ref.get : Ref a ->{} a

Ref.set :
  forall c a.
  (Ref a)^c -> (a ->{} Unit)^c

Ref.modify :
  forall c d a b.
  (Ref a)^c -> ((a -> (b, a))^d ->{} b)^c
```

Ref инвариантен по содержимому. Partial application set/modify удерживает cell и её captures; runtime-запись дополнительно проверяет lifetime содержимого.

```flick
increment : Ref Number ->{} Number
let increment counter =
  Ref.modify counter (fun current ->
    let next = current + 1
    (next, next)
  )
```

`modify` выполняет одну операцию изменения состояния; отдельные `get` и `set` не составляют одну атомарную операцию. Callback обычного `Ref.modify` чистый. Изменение с runtime-callback требует другого контракта синхронизации. [Effect Ref](https://effect.website/docs/v4/state-management/ref).

Canonical Eq/Hash по содержимому Ref в первой версии отсутствуют: мутация могла бы изменить hash ключа. Явная identity-операция или wrapper с однажды выделенным identity key — отдельный API; он не сравнивает сохранённое значение. `deriving Hash` для record с Ref без такого wrapper/witness даёт диагностику.

## Eq и Hash как классы языка

```flick
class Eq a => Hash a =
  hash : a -> Number
```

Закон: если `eq x y`, то hashes совпадают. Обратное неверно: коллизии допустимы. Derived `Eq` и `Hash` должны использовать одну семантику полей. Типизация пользовательского instance не доказывает этот закон. [Effect Hash](https://effect.website/docs/v4/trait/hash).

Canonical Eq/Hash отображаются в согласованные native Effect protocols. Для альтернатив предоставляется `EqHash a` witness с обоими методами и constructors `HashSet.with witness` / `HashMap.with witness`. Коллекция хранит witness, а adapter упаковывает ключи в native protocol wrappers и разворачивает их при выдаче. Вставка/поиск всегда используют тот же witness; сменить его у заполненной коллекции нельзя. Union коллекций с различными witnesses требует явного rehash в выбранный witness. Это сохраняет движок коллекций Effect ценой обёрток. [Структурное Equal Effect](https://effect.website/docs/v4/trait/equal).

## Option и совместимость представления

Стандартный `Option` имеет исходные конструкторы `None` и `Some`, а в runtime — реальные `Option.none()` и `Option.some(value)` из Effect. Pattern matching проверяется исходным frontend и читает их нативное представление. [Effect Option](https://effect.website/docs/v4/data-types/option).

Эта привязка определяется идентичностью стандартного типа. Пользовательский `type Option ...` или объект с `_tag = "Some"` не получает ABI Effect автоматически. При импорте обычных nullable JS-значений преобразование выполняет явный binding.

Чистая функция над `Option` генерируется как обычная JS-функция. Импорт модуля данных Effect не означает запуск fiber. Backend использует стандартные реализации Effect и может устранять неиспользуемые импорты.

## Основная коллекция и представление List

Выбрано имя **`List a`** и представление **dense JS array с immutable source API**. Все `[a, b]` и `[]` создают List; скрытой перегрузки literal по ожидаемому типу нет. Отдельный `Chunk a` использует native Effect Chunk. Mutable foreign arrays представлены `Js.Array a` через runtime API; второго равнозначного имени Array в prelude нет.

| Представление | Сильные стороны | Цена |
| - | - | - |
| JS-массив с неизменяемым API — выбранный List | Прямой JS ABI, индексный доступ и обычные циклы; простая генерация кода | Добавление в начало, immutable update и конкатенация обычно копируют O(n) элементов |
| Cons-list `Nil / Cons` | O(1) prepend, head и tail; разделение хвостов; привычный ML pattern matching | Индексный доступ O(n); больше объектов и нагрузки на GC; нужен перевод в JS arrays |
| Effect `Chunk` | Готовая неизменяемая коллекция для многократной конкатенации | Дополнительная структура и стоимость операций; для многих задач обычный массив проще |

У List O(1) length/index и обычный проход циклом; prepend, immutable update, tail и concat обычно копируют O(n) элементов. O(1) cons/tail не обещаются. Рекурсивный обход с повторным `slice(1)` может стать квадратичным; стандартные traversals используют индексные loops. Будущий cons-list будет отдельным типом с другим representation, а не изменением ABI List.

Основной literal всегда означает List. Chunk, mutable JS arrays и другие контейнеры создаются явными constructors/conversions. Отсутствующие элементы sparse JS array не проходят как обычный List без адаптера.

`ReadonlyArray` в TS не гарантирует фактическую неизменяемость. FFI обязан копировать, передавать владение или работать с opaque mutable handle. Поверхностной копии достаточно только для самой последовательности: вложенные records тоже требуют согласованного контракта. Unsafe-входы не должны попадать в доверенный чистый API.

[Документация Effect Chunk](https://effect.website/docs/v4/data-types/chunk) рекомендует Chunk прежде всего при частой конкатенации и отдельно предупреждает о дополнительных расходах в других сценариях. Все эти коллекции остаются библиотекой над тем же Effect backend.

## Redacted и внешняя сериализация

`Redacted a` полезен как opaque тип с маскированным выводом и явным извлечением значения. Он не шифрует данные. Скрытие в логах не означает запрет сериализации.

В [Effect Schema v4](https://effect.website/docs/v4/schema/effect-data-types) canonical JSON encoding для Redacted может раскрывать внутреннее значение; доступна настройка `disallowJsonEncode: true`. Принято для языка: deriving не создаёт раскрывающий внешний codec без opt-in. Запрет, намеренная передача секрета и маскированная проекция задаются явно; маскированная проекция является односторонним encoder и не обещает round-trip.

```text
Redacted.make : a ->{} Redacted a
Redacted.reveal : Redacted a ->{} a
```

Используется canonical `Redacted` Effect, чьё содержимое внешний код может стереть через `wipeUnsafe`. Поэтому создание и чтение в source API консервативно runtime: создание выделяет самостоятельный стираемый объект, чтение зависит от его текущего состояния. Простое замораживание JS-оболочки не доказывает неизменяемость скрытого содержимого. Стирание через FFI тоже runtime; чтение стёртого значения даёт defect, если адаптер явно не переводит его в объявленную ошибку. Автоматические чистые Eq/Hash по содержимому не предоставляются. [Effect Redacted](https://effect.website/docs/v4/data-types/redacted).

## Числовая модель JavaScript

Решение принято: обычные числа следуют JavaScript. В source используется `Number` с прямым представлением JS `number`; для произвольных целых — отдельный `BigInt`. `BigDecimal`, `Duration` и другие специальные типы остаются библиотечными типами со своими операциями.

| Аспект | Контракт |
| - | - |
| `Number` | IEEE 754 binary64, включая NaN, обе бесконечности и signed zero |
| Литералы | Обычный числовой литерал имеет тип `Number`; суффикс `n` явно выбирает `BigInt` |
| Арифметика | JS-округление, overflow, деление и remainder; нет обещания точной целочисленной арифметики для всего Number |
| Безопасные целые | `SafeInteger` — refinement, а не другое представление; обычное сложение не сохраняет его автоматически |
| Bitwise-операции | Следуют JS-преобразованиям разрядности и должны иметь явно описанные операции |
| `BigInt` | Отдельный numeric domain; нет неявного смешения с Number |
| Межтиповые coercions | JS number representation не требует неявного сложения строки и числа или loose equality |
| JSON | Домен Number шире JSON; finite codec отвергает неподдерживаемое значение, lossless codec использует явную tagged/string форму |

Модель соответствует [ECMAScript Number и BigInt](https://tc39.es/ecma262/multipage/ecmascript-data-types-and-values.html). Ошибки домена примитивов BigInt, например деление на ноль, сохраняют JS-исключения; при выполнении внутри Effect это defects. Для recoverable варианта библиотека предоставляет checked-операцию с Result или объявленной typed error. Ни обычное `Number`-деление на ноль, ни NaN не следует молча переводить в typed error.

`Finite`, `SafeInteger`, `NonNegative` — возможные refinements для границ данных. Их арифметика возвращает `Number`, пока операция отдельно не гарантирует сохранение ограничения. `Schema Number` отражает весь выбранный domain; ограничение JSON задаёт codec. Нативный [JSON.stringify преобразует non-finite числа в null](https://tc39.es/ecma262/multipage/structured-data.html), поэтому его нельзя использовать как «lossless Number codec».

### Eq Hash и Ord для Number

| Equality | NaN равен NaN | +0 равен -0 | Цена |
| - | - | - | - |
| JS strict equality | Нет | Да | Нерефлексивность осложняет законы Eq и поведение ключей |
| `Object.is` | Да | Нет | Нужен согласованный hash и adapter относительно canonical Effect equality |
| Canonical Effect equality — выбрано | Да | Да | Для точного JS `===` нужна отдельная функция |

Canonical `Eq Number` и оператор `==` используют SameValueZero: NaN равен NaN, +0 равен -0. Это совпадает с проверенным [Effect Equal](https://github.com/Effect-TS/effect/blob/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f/packages/effect/src/Equal.ts#L187-L205); [native Hash.number](https://github.com/Effect-TS/effect/blob/81fc8d9c7855d0f31bf493f379ec7bc2a6150a6f/packages/effect/src/Hash.ts#L311-L350) согласован с ним. `Number.strict_equal` предоставляет JS `===`, `Number.same_value` — Object.is. Hash не является стабильным внешним идентификатором между версиями runtime.

`Ord Number.compare` задаёт total order: −Infinity < конечные числа < +Infinity < NaN; signed zeros сравниваются как EQ, все NaN как EQ. `compare x y = EQ` эквивалентно canonical Eq. Generic sorting и `Ord.less` используют этот порядок. Примитивные Number/BigInt `<`, `<=`, `>`, `>=` сохраняют JS-семантику: например, любое такое сравнение Number с NaN ложно. Это явное различие между numeric operators и lawful Ord, а не скрытая смена арифметики JS.

Даже если Eq считает +0 и -0 равными, compiler не вправе повсеместно заменять одно другим: арифметика может их различать. Equality коллекций не является разрешением на любое подстановочное преобразование.

## BigDecimal и Duration

BigDecimal — отдельный native Effect type с integer coefficient и scale. Первая версия не вводит `dec`-literals и не перегружает Number-арифметику. Значения создаются из строки или пары BigInt/checked scale; путь через Number явно признаёт уже произошедшее binary64 округление. [Effect BigDecimal](https://effect.website/docs/v4/data-types/bigdecimal).

Для source decimal API нет скрытой глобальной precision. Точные сложение/вычитание/умножение сохраняют конечный десятичный результат; непредставимый scale даёт checked DecimalError. `divide_to_scale` обязательно принимает конечный целый safe-integer scale и rounding policy: TowardZero, Floor, Ceiling, HalfEven, HalfAwayFromZero или Exact. Деление на ноль — DecimalError; Exact отвергает результат, требующий округления. Результат — `Result DecimalError BigDecimal`, native representation сохраняется. Adapter вычисляет quotient/remainder без промежуточного Number; нативное деление с собственными defaults не подставляется вместо этого контракта. Unsafe native операции остаются явно именованными bindings.

Duration — native неотрицательная длительность, включая явно выбранное `Duration.infinity`; это не timestamp или календарный период. В первой версии используются обычные checked constructors nanos(BigInt), millis(Number), seconds(Number), без `ms`/`s` literals. Отрицательное, NaN, неявная infinity и overflow при преобразовании единиц дают `Result DurationError Duration`. Точная наносекундная форма начинается с BigInt; Number-based формы сохраняют выбранное округление JS и не обещают большей точности. Lossy conversion в Number называется явно; infinity не превращается молча в конечное число. [Effect Duration](https://effect.website/docs/v4/data-types/duration).

Для BigDecimal/Duration доступны согласованные native Schema, Eq, Hash, Ord и Show через проверенные adapters. Специальные literals и numeric operator overloading отложены за первую версию. Checked library constructors оставляют единый числовой синтаксис Number/BigInt и не требуют нового runtime.

В первый целевой релиз входят полноценный deriving Schema, явные codecs, номинальные refinements с runtime-проверкой, нативные типы Effect, библиотечный Ref, мост Eq/Hash и runtime-доступ к Redacted. Compiler spike может реализовывать эти механизмы поэтапно, но полнота deriving остаётся требованием языка.
