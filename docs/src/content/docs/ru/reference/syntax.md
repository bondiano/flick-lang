---
title: "Синтаксис"
source_hash: b28122fd6924ace2
---

Здесь описан синтаксис исходного кода, который принимают `flick check`
и `flick fmt`. Правила языка изложены в [спецификации](../../spec/).
Каждый блок ```` ```flick ```` на этой странице и в спецификации
разбирается в тестах (`flick-core/tests/corpus.rs`). Если парсер
перестаёт принимать блок, сборка завершается ошибкой.

## Лексическая структура

Исходный файл должен быть в UTF-8; другая кодировка вызывает `FLICK-S001`.
Позиции в диагностике задаются диапазонами байтов UTF-8,
а столбцы для правил отступов считаются в символах.

### Пробельные символы и комментарии

- Пробелы разделяют токены. Табуляция разрешена только внутри строк
  и комментариев (`FLICK-S009`), поскольку правила отступов зависят от столбцов.
- Допустимые переводы строк — `\n` и `\r\n`.
- `// …` — однострочный комментарий, `/// …` — комментарий документации
  для команды `flick doc`, `/* … */` — блочный комментарий.
  Блочные комментарии можно вкладывать друг в друга.

### Имена

| Токен | Форма | Назначение |
| - | - | - |
| имя | `[a-z_][A-Za-z0-9_]*` | значения, функции, поля, переменные типов |
| имя с заглавной буквы | `[A-Z][A-Za-z0-9_]*` | типы, конструкторы, модули, классы, сервисы, ошибки |
| метка | `'[a-z_][A-Za-z0-9_]*` | метки циклов (`'outer: loop { … }`, `break 'outer`) |
| `_` | отдельно | образец, который не связывает имя |

В квалифицированных именах вокруг `.` нет пробелов: `Users.get`,
`Ui.Event.exhaust`, `Sql.Template`. Запись `user.name` обращается к полю.

### Числовые литералы

Числовые литералы следуют грамматике JavaScript `NumericLiteral`
и обозначают в точности те же числа, что и в JS
(см. [Schema и стандартные типы данных](../../spec/07-schema/)):

- десятичные `0`, `42`, `1.5`, экспоненциальная запись `1e-3`, `2E+10`;
- `0x1F`, `0o17`, `0b1010`; буквы префикса могут быть в любом регистре;
- `_` разделяет две цифры: `1_000_000`, `0xFF_FF`. Он не может стоять
  в начале или в конце последовательности цифр, повторяться или примыкать
  к `.`, `e` или префиксу;
- BigInt — целое число с суффиксом `n`: `10n`, `0x1Fn`.
  Дробная часть и экспонента не допускаются;
- у десятичного числа не должно быть ведущего нуля:
  `010` (устаревшая восьмеричная запись) и `09` — ошибки; пишите `0o10`;
- в отличие от JavaScript, `.5` и `5.` — ошибки: `.` используется для доступа
  к полю. Пишите `0.5` и `5.0`.

Некорректный литерал вызывает одну диагностику `FLICK-S006`
и не разбивается на несколько токенов.

### Строковые литералы

- `"…"` занимает одну строку. Допустимы управляющие последовательности
  `\n \t \r \\ \" \0` и `\u{…}` с 1–6 шестнадцатеричными цифрами.
  Последняя должна обозначать скалярное значение Unicode:
  не суррогат и не больше `10FFFF`. За `\0` не может следовать цифра.
  Любая другая управляющая последовательность вызывает `FLICK-S005`.
- В необработанных строках `r"…"`, `r#"…"#`, `r##"…"##`… управляющие
  последовательности не обрабатываются. Такие строки могут занимать
  несколько строк исходного файла. Закрывающий разделитель — `"` с тем же
  количеством `#`, что и в открывающем.
- Многострочная строка начинается с `"""` и перевода строки, а заканчивается
  следующим `"""`. Управляющие последовательности работают так же,
  как в `"…"`. Из значения удаляется первый перевод строки.
  Последняя строка удаляется, если перед закрывающим `"""` на ней только
  пробелы. Затем убирается общий отступ непустых строк; строка с закрывающим
  разделителем тоже учитывается. Управляющие последовательности декодируются
  после этого, поэтому `\u{20}` считается содержимым, а не отступом.
- Дырка `\{e}` в однострочной строке `"…"` подставляет `display e`:
  `"n = \{n + 1}"` означает `"n = " ++ display (n + 1)`. `display` — метод
  класса `Display` из Prelude: `String` остаётся собой, без кавычек, которые
  добавляет `show`, а `Int`, `Float` и `Bool` записываются так же, как их
  записывает `show`. Тип без экземпляра вызывает `FLICK-C001`. Выражение
  дырки заканчивается на строке исходного файла, где записана строка
  (иначе `FLICK-S004`), и может содержать фигурные скобки и строки со своими
  дырками. В необработанных строках дырок нет, `\{` в многострочной строке
  вызывает `FLICK-S005`. В строковом образце и в сообщении `todo` дырок
  нет (`FLICK-S007` и `FLICK-S005`). Оптимизатор
  release-сборки склеивает константные части: `"\{2 + 3} of \{"x"}"`
  компилируется в `"5 of x"`.

Переводы строк исходного файла внутри строковых литералов преобразуются
в `\n`, даже если в файле используется `\r\n`. Выражения, состоящие только
из литералов, вычисляются при компиляции по правилам JS
(см. [Вычисление литеральных выражений](../../spec/06-typescript-and-ffi/#вычисление-литеральных-выражений)):
`60 * 60 * 24`, `"a" ++ "b"`, `not false`.

```flick
let examples =
  [
    "tab\t and \u{1F600}",
    r"C:\temp",
    r#"say "hi""#,
    "sum: \{2 + 3}",
    """
    first line
      indented line
    """,
  ]
```

### Операторы и пунктуация

```text
( ) [ ] { } , ; : . .. ..< = -> <- => | ^ _ ? @
|> <| >> << || && == != < <= > >= ++ + - * / % ** not
```

## Ключевые слова

Ниже перечислены зарезервированные слова Flick: что они обозначают
и где прочитать подробности. Такие слова нельзя использовать как имена.

```text
module exposing import as let mut rec assert fun match with try catch if then
else task await lazy scope provide ignore todo echo type opaque where kind
error service class instance deriving foreign from forall not none true false
in when for yield do break continue
```

Контекстные слова считаются обычными именами, кроме следующих позиций:

- `loop` перед `{` и `while` перед условием, за которым идёт `do`;
- `private` перед `module` в заголовке файла (см. [модули](../modules/)) и
  `plugin` перед головой в списке `exposing` (см.
  [плагины](../plugins/#как-подключить-плагин));
- `alias` сразу после `type`, а `unboxed` и `untagged` непосредственно перед
  ним (см. [генерацию кода](../codegen/));
- `pack` и `unpack` перед модулем и `as`, а `exists` перед именем и `:` в
  типе (см. [упакованные модули](../modules/#упакованные-модули-и-экзистенциальные-результаты));
- режимы внешних функций `pure`, `effect`, `blocked` и источник
  `global`;
- метаданные внешних функций `arity`, `this`, `catch`, `cancel`, `nullable`,
  `retention`, `optional`, `variadic`, `reviewed` (неизвестные метаданные —
  `FLICK-S011`);
- `sql` перед телом, именем и телом, `table` или `migration`: блок плагина
  пакета `std`, который читает его предложения, столбцы и DDL (см.
  [SQL](../sql/));
- `cli` перед строкой и `{`: блок плагина пакета `std`, который читает его
  предложения (см. [приложения командной строки](../cli-dsl/));
- `codec` перед схемой и `{`, `server` перед `fn` или именем и `component`
  перед именем объявления верхнего уровня: блоки плагина пакета `std` (см.
  [DSL кодеков](../schema/#dsl-кодеков), [серверные функции](../server-fn/) и
  [ui.md](../ui/#компоненты)). Эти блоки `std` есть в каждом модуле без
  импорта;
- `and` между связками группы `do`, `x <- a and y <- b`;
- `effect` перед именем объявления эффекта, `tail`, `abort` и `control` перед
  операцией сервиса или эффекта и `return` перед образцом ветки `try` (см.
  [effects.md](../effects/#эффекты-и-handlers));
- `proof` перед именем в начале объявления и `law` перед именем в начале
  члена класса (см. [Объявления](#объявления));
- `require` и `ensure` в начале пункта контракта функции, после `where` у
  `let` (см. [Объявления](#объявления)).

### В модулях

| Ключевое слово | Назначение | Пример |
| - | - | - |
| `module` | Задаёт имя модуля для файла | `module App.Main exposing (run)` |
| `private` | Ограничивает модуль файла его пакетом перед `module`; в других местах остаётся именем | `private module App.Internal exposing (helper)` |
| `exposing` | Перечисляет экспортируемые имена или имена, импортируемые без префикса модуля | `import App.Domain exposing (User, greet)` |
| `import` | Подключает другой модуль | `import Effect.Log as Log` |
| `as` | Задаёт локальное имя импортированного модуля; в `sql table` указывает тип столбца во Flick | `import Effect.Log as Log` |
| `pack` | Контекстное слово: модуль как значение `Module Sig` | `pack Argument as Source` |
| `unpack` | Контекстное слово: открывает упакованный модуль в собственной области | `unpack packed as K in K.value` |

Подробнее — в разделе [Модули и имена](../modules/).

### В объявлениях

| Ключевое слово | Назначение | Пример |
| - | - | - |
| `let` | Связывает имя со значением или функцией на уровне модуля либо внутри блока | `let total = a + b` |
| `mut` | `let mut` объявляет переменную функции, которой присваивает `name = value` | `let mut total = 0` |
| `rec` | В `let rec` разрешает функции вызывать себя; блок `rec { … }` объединяет взаимно рекурсивные функции | `let rec go n = …` |
| `assert` | `let assert` связывает образец, который может не подойти, и завершается с ошибкой, если он не подошёл | `let assert [x, ..._] = xs as "no items"` |
| `type` | Объявляет алгебраический тип данных, а в сочетании `type alias` — прозрачный псевдоним типа | `type Shape = Circle Float \| Empty` |
| `opaque` | Скрывает представление типа за пределами его модуля | `opaque type alias UserId = String` |
| `where` | Задаёт условие уточняющего типа, цель `proof` или `law` или фильтр в `sql { … }` | `where user.id == params.id` |
| `proof` | Контекстное слово: перед именем в начале объявления формулирует свойство определений модуля и доказывает его; компилятор проверяет доказательство и стирает объявление; в других местах остаётся именем | `proof same where same_type a a = refl` |
| `law` | Контекстное слово: перед именем в начале члена класса формулирует то, что выполняет каждый экземпляр; компилятор стирает закон, а цель `lawful` доказывает его для экземпляра; в других местах остаётся именем | `law identity (v : f a) where equal (map identity v) v` |
| `require` | Контекстное слово: в начале пункта контракта функции задаёт предусловие для её параметров | `require (0 <= used)` |
| `ensure` | Контекстное слово: в начале пункта контракта функции задаёт постусловие — функцию от результата | `ensure (fun r -> r >= 0)` |
| `kind` | Объявляет кинд и его конструкторы на уровне типов | `kind Nullability = NonNull \| Nullable` |
| `error` | В сочетании `type error` объявляет ожидаемую ошибку с номинальной идентичностью | `type error NotFound = NotFound UserId` |
| `service` | Объявляет операции сервиса, реализация которого выбирается при выполнении | `service Clock = now : Unit ->{} Float` |
| `class` | Объявляет класс типов | `class Named a = name : a -> String` |
| `instance` | Определяет реализацию класса для типа | `instance Named User = let name u = u.name` |
| `deriving` | Поручает компилятору создать экземпляры классов | `deriving Eq, Show, Schema` |
| `foreign` | Связывает значение JavaScript с сигнатурой Flick | `foreign read_text from "./fs.js" "readText"` |
| `from` | Указывает источник для `foreign` или таблицу в `sql { … }` | `from user in Users` |
| `forall` | Явно вводит универсально квантифицированные переменные типов | `keep : forall s. Path ->{Scope s} File s` |

Подробнее — в разделах [Кинды и типы](../types/),
[Классы и экземпляры](../classes/), [Эффекты и ошибки](../effects/), [Сервисы, слои и хост](../services/)
и [Внешние функции](../ffi/).

### В выражениях

| Ключевое слово | Назначение | Пример |
| - | - | - |
| `fun` | Создаёт анонимную функцию | `fun x -> x + 1` |
| `if` `then` `else` | Задают условное выражение; без `else` оно имеет тип `Unit` | `if n == 0 then 1 else n` |
| `match` `with` | Задают сопоставление с образцом | `match shape with \| Circle r -> r` |
| `when` | Задаёт дополнительное условие ветки сопоставления | `\| Rect w h when w == h -> "square"` |
| `with` | Также вводит обработчики `try` и обновление записи | `{ user with name = "Ada" }` |
| `try` | Обрабатывает ожидаемые ошибки и операции эффектов своего тела | `try find id with \| NotFound _ -> None` |
| `catch` | Постфиксный обработчик только типизированных ошибок (зарезервировано в выражениях и связываниях; допускается как метка поля записи, `.catch`, переименованный образец записи `{ catch = f }` и метаданные `foreign`) | `find id catch \| NotFound _ -> None` |
| `not` | Отрицает логическое значение; в `sql table` входит в ограничение `not null` | `not done` |
| `true` `false` | Два значения типа `Bool` | `if ok then true else false` |
| `none` | Обозначает пустую строку в типе | `a ->{none; NotFound} b` |
| `in` | Связывает имя со строкой таблицы в конструкции SQL `from` или `join`; начинает тело `unpack … as K in` | `join account in Accounts` |
| `do` | Последовательность инструкций через `Monad`: `x <- e` связывает результат с именем `x`, `if c` фильтрует | `do { x <- left; y <- right; pure (x, y) }` |
| `for` `yield` | Ленивый `Seq` элементов, которые даёт `yield`, — один генератор; `[for …]` — строгий `List`; `for … do body` выполняет тело для каждого элемента | `[for x <- xs if x > 0 yield x * 2]` |
| `loop` | Контекстное слово: повторяет блок до возврата результата через `break value` | `loop { break 42 }` |
| `while` | Контекстное слово: повторяет тело типа `Unit`, пока условие истинно | `while i < n do i = i + 1` |
| `break` | Выходит из ближайшего цикла или из объемлющего, чью метку называет; значение принимает только `loop` | `break 42`, `break 'outer` |
| `continue` | Пропускает остаток текущей итерации ближайшего цикла или цикла с меткой | `continue 'outer` |

У цикла может быть метка, `'outer: for x <- xs do …`; `break` и `continue`
называют её, чтобы выйти из объемлющего цикла или продолжить его (см.
[локальные циклы](../bindings/#локальные-циклы)).

### Эффекты и ресурсы

| Ключевое слово | Назначение | Пример |
| - | - | - |
| `task` | Создаёт `Eff r e a` из тела блока, не выполняя его | `task { Users.get id }` |
| `await` | Один раз выполняет `Eff` в текущем файбере и возвращает результат или вычисляет `Lazy` | `await request` |
| `lazy` | Создаёт `Lazy r e a`: блок выполнится не больше одного раза, при первом `await`; `let lazy x = e` связывает такое значение своей функции | `lazy { Config.load () }` |
| `scope` | Выполняет блок в новом регионе и при завершении освобождает полученные в нём ресурсы | `scope { File.read_all (File.open_scoped path) }` |
| `provide` | Выполняет блок с сервисами, созданными слоем | `provide live { run user }` |
| `ignore` | Отбрасывает результат промежуточного выражения | `ignore (Store.save user)` |

`ignore` вычисляет свой аргумент и отбрасывает его значение. Если это
значение типа `Eff`, описанная в нём работа не выполняется. Чтобы выполнить
`t` и отбросить результат, напишите `ignore (await t)`.

Подробнее — в разделах [`task` и `await`](../effects/#task-и-await),
[`lazy`](../effects/#lazy) и [Регионы и захваты](../regions/).

### Отладка

| Ключевое слово | Назначение | Пример |
| - | - | - |
| `todo` | Ещё не написанный код: имеет любой тип и падает при выполнении | `todo "parse the header"` |
| `echo` | Выводит значение и место вызова в stderr и возвращает это значение | `echo (total rows)`, `rows \|> echo` |

Оба проверяются в любом месте и оставляют предупреждение (`FLICK-T022`,
`FLICK-T023`); `flick pack` отказывается собирать пакет, где они ещё есть
(`FLICK-P024`). `echo` — не эффект: он нужен для отладки, как точка
останова. Оба слова можно использовать как метки записей: `{ echo = … }`.

### Встроенные предметные языки

| Блок | Назначение | Пример |
| - | - | - |
| `ui` | Создаёт представление интерфейса: в его коде видны имена модуля `Ui` | `ui { … }` |
| `refined` | Объявляет тип, значения которого проходят проверку, с функциями `parse`, `value` и `check` | `refined type Port = Float where fun n -> n > 0` |

Подробнее — в разделах [Схемы и кодеки](../schema/) и
[Пользовательские интерфейсы](../ui/). Блок интерфейса, компонент,
`component counter () = …`, интерфейс командной строки, `cli "deploy" { … }`,
и типизированный SQL-запрос, `sql { from u in Users; select u }`, — блоки
плагина пакета `std`, а не ключевые слова: каждое слово начинает блок там,
где блок может начаться, и остаётся именем в остальных местах. См.
[Приложения командной строки](../cli-dsl/) и [SQL](../sql/). Таковы же
`suite "name" { … }` и `test "name" { … }`, но только в начале объявления
верхнего уровня, а внутри них — `timeout` после имени случая, `test.each`
перед строками теста, `parallel` перед именем набора, хуки набора `before_all`, `before_each`, `after_each`
и `after_all { … }` и его `fixture name = …`, в том числе `fixture shared`
и `fixture shared concurrent` (см. [Тестирование](../testing/)).

### Подсказки в редакторе

При наведении на ключевое слово, с которого начинается выражение,
языковой сервер показывает тип всего выражения и объясняет действие слова.

| На что навести | Что покажет подсказка |
| - | - |
| `task` | `task { … } : Eff {Store} {StoreError} Float` — вычисление, создаваемое блоком |
| `lazy` | `lazy { … } : Lazy {Store} {StoreError} Float` — ленивое вычисление, создаваемое блоком |
| `scope` | `scope { … } : Float` — результат блока, возвращаемый после освобождения ресурсов |
| `provide`, `try`, `catch` | Результат блока или выражения и сервисы или ошибки, которые он устраняет |
| `match`, `if` | `match … with : String` — результат выбранной ветки |
| `fun` | `fun … -> … : Float -> Float` — тип функции |
| `sql`, `ui` | Значение, создаваемое блоком, например `sql { … } : Template Pg {} { label : String }` |
| `await` | `await … : Float` — результат и запускаемое вычисление `Eff` |
| `ignore` | `ignore : Eff {Store} {StoreError} Float -> Unit` — тип отбрасываемого значения и предупреждение, если это `Eff`, который не будет выполнен |

Для остальных ключевых слов подсказок нет. Типы связанных с ними имён
можно посмотреть, наведя указатель на сами имена.

## Таблица операторов

Собственные операторы объявлять нельзя. Ниже они перечислены от самого слабого приоритета к самому сильному:

| Уровень | Операторы | Ассоциативность |
| - | - | - |
| −1 | постфиксный `catch \| …` | отсутствует: цепочка внешних `catch` требует скобок `(a catch \| E -> b) catch \| F -> c` |
| 0 | `<\|` | правая; не смешивается с `\|>`: `x \|> f <\| y` — ошибка |
| 1 | `\|>` | левая |
| 2 | `\|\|` | правая |
| 3 | `&&` | правая |
| 4 | `==` `!=` `<` `<=` `>` `>=` | отсутствует: `a < b < c` — ошибка |
| 4½ | `a..b`, `a..<b`, `a..` | нет: `a..b..c` — ошибка |
| 5 | `++` | правая |
| 6 | `+` `-` | левая |
| 7 | `*` `/` `%` (`%` — остаток от деления JS) | левая |
| 8 | `**` | правая |
| 9 | префиксные `-`, `not`, `await`, `ignore`, `echo` | — |
| 9½ | `>>`, `<<` | `>>` левая, `<<` правая; не смешиваются: `f >> g << h` — ошибка |
| 10 | применение функции `f x y` | левая |
| 11 | `.field`, `Module.name`, постфиксный `?` | левая |

`-x ** 2` означает `-(x ** 2)`; `-a * b` означает `(-a) * b`; `await f x`
означает `await (f x)`; `0..n - 1` — это `0..(n - 1)`, а `i < 0..n` сравнивает с диапазоном.
`f <| g <| x + 1` означает `f (g (x + 1))`, а `f a <| x` — вызов `f a x`;
`List.map f >> List.filter p` — композиция двух применений, а `await f >> g`
означает `await (f >> g)`. Что делают операторы, описано в разделе [Числа и
операторы](../numbers/#операторы).
Побитовые операции — функции `Int.bit_*` и `Int.shift_*`.

Диапазон — это `Seq Int`: `a..b` считает от `a` до `b` включительно
(`Seq.range_to`), `a..<b` останавливается перед `b` (`Seq.range`), а `a..`
не заканчивается (`Seq.from`). Единственный в скобках, `[a..b]` — `List` тех же
чисел; `[a..]` — ошибка. Число перед `..` на нём заканчивается: `1..5` —
диапазон, `1.5` — число.

## Правила отступов

Границы тел функций, ветвей сопоставления и членов `class`, `instance`
и `service` определяются отступами. `in` не используется; последнее
выражение блока задаёт его значение
(см. [Прямые эффектные вызовы](../../spec/01-direct-calls/)).
Фигурные скобки ограничивают записи и блоки `task`, `scope`, `provide`,
`sql`, `ui`, `rec` и плагинов.

1. Объявления верхнего уровня начинаются в столбце 1. Строка с большим
   отступом продолжает текущее объявление.
2. *Блок* открывается после `=`, `->`, `then`, `else` и `try`:
   - если блок начинается на следующей строке, его столбец определяется
     первым токеном. Строка в этом столбце начинает следующую инструкцию,
     строка с большим отступом продолжает текущую, а строка с меньшим
     отступом завершает блок;
   - если блок начинается на той же строке, он содержит одну инструкцию.
     Она продолжается на строках с большим отступом, чем у её первой
     строки и у любого внешнего блока в фигурных скобках.
     Любая другая строка завершает инструкцию;
   - после `then` и `else` `{` открывает блок инструкций в фигурных
     скобках, как после `do` и `loop`; запись на этом месте пишется в
     круглых скобках (`FLICK-T074`).
3. Строка в столбце блока продолжает инструкцию, если начинается
   с инфиксного оператора, кроме `-`, с `|`, `then`, `else`, `with`, `catch`, `where`
   или с закрывающего разделителя. Поэтому конвейеры `|>` и `<|`, ветви
   `match` и `catch`, ветви `else` и цель `proof` могут начинаться в том же столбце,
   что и их выражение или объявление.
4. В записях, типах записей и блоках `{ … }` элементы выравниваются
   по первому элементу, даже если он идёт сразу после `{` на той же строке.
   Элементы разделяются переводами строк или `;`.
   Разделять инструкции через `;` можно только внутри фигурных скобок.
5. Внутри `( … )`, `[ … ]` и строк типов в фигурных скобках переводы
   строк не имеют значения. Но строка, которая начинается левее столбца
   внешнего блока, завершает скобочную конструкцию. Благодаря этому
   пропущенная `)` не поглощает последующие объявления.
6. Закрывающий разделитель может иметь меньший отступ, чем закрываемый им блок.
7. `else if` на одной строке продолжает цепочку, не открывая блок.
8. В теле блока, который об этом просит, как `ui { … }`, строки, продолжающие
   вызов, — его аргументы, по одному целому выражению на строку, а более
   глубокие строки под одной из них принадлежат ей самой: `div props`, затем
   строка на каждого ребёнка. Строка, которую правила 3 или 5 делают
   продолжением, им и остаётся. В остальных местах такие строки — аргументы
   токен за токеном, как в правиле 2.

```flick
# type UserId = UserId String
# type alias User = { name : String }
# type alias UserView = { title : String }
# type error NotFound = NotFound UserId
# service Users =
#   get : UserId ->{; NotFound} User
# service Log =
#   info : String ->{} Unit
# to_view : User -> UserView
# let to_view user = { title = user.name }
#
load_view : UserId ->{Users, Log; NotFound} UserView
let load_view id =
  let user = Users.get id
  let view = to_view user
  Log.info view.title
  view

classify : Float -> String
let classify n =
  if n < 0 then
    "negative"
  else if n == 0 then
    "zero"
  else
    "positive"

total : List Float -> Float
let total xs =
  xs
  |> List.filter (fun x -> x > 0)
  |> List.sum
```

Если отступ строки не соответствует ни одному внешнему блоку
или тело на следующей строке записано без отступа, на этой строке
выдаётся одна диагностика `FLICK-S009`:

```text
let between x =
  match x with
  | A ->
      foo
     bar        // FLICK-S009: column 6 continues nothing here
```

## Объявления

```text
file        = [module_header] { import } { decl | module_application }
module_header = ["private"] "module" ModulePath { module_params } [":" contract] ["exposing" exposing]
module_params = "(" Con ":" contract { "," Con ":" contract } ")"   // параметризованный модуль
contract    = ModulePath | module_params "->" contract ["with" "type" …]
import      = "import" ModulePath ["as" Con] ["exposing" exposing]
exposing    = "(" item { "," item } [","] ")"
item        = name | Con | Con "(..)" | ".." | "plugin" name   // см. plugins.md
ModulePath  = Con { "." Con }

decl        = signature | let | rec_group | type_decl | kind_decl | error_decl
            | service | class | instance | foreign | module_contract | inline_module | proof
            | head { head_arg } ("{" { token } "}" | { token })   // плагина, до следующего decl
attribute   = "@" name { name | String }   // перед объявлением, членом или вариантом
inline_module = "module" Con ["exposing" exposing] "=" indented { import | decl }
module_contract = "type" "module" Con "=" indented { module_member }
module_application = "module" Con "=" ModulePath "(" ModulePath { "," ModulePath } ")"
signature   = name ":" type
let         = "let" ["rec"] (name { atom_pattern } | pattern) [":" type] ["where" contract] "=" block
            | "let" "assert" pattern [":" type] "=" block ["as" String]
            | "let" "mut" name [":" type] "=" block            (только в блоке)
rec_group   = "rec" "{" { signature | let } "}"
type_decl   = ["opaque"] "type" Con { param } ["=" variants] [deriving]
            | ["opaque"] "type" "alias" Con { param } "=" type [deriving]
            | "refined" "type" Con { param } "=" type "where" expr [deriving]    (объявление плагина std)
            | ["opaque"] ("unboxed" | "untagged") "type" Con { param } "=" variants [deriving]
variants    = ["|"] variant { "|" variant }
variant     = Con { capture } ["=" literal] [":" type]
            // fields are atomic types; a literal only in an enum; `: type`, what a
            // constructor of a GADT builds, only in a `type` and without a literal
literal     = string | ["-"] number
param       = name | "(" name ":" kind ")"
deriving    = "deriving" Con { "," Con }
kind_decl   = "kind" Con "=" ["|"] Con { "|" Con }
error_decl  = "type" "error" Con { param } ["=" variant] [deriving]
service     = ("service" | "effect") Con { param } [ "=" operations ]
operations  = block of: [ "tail" | "abort" | "control" ] signature | let | type_decl
class       = "class" [context "=>"] Con { param } ["|" fundep { "," fundep }] ["=" class_members]
fundep      = { name } "->" { name }
instance    = "instance" [context "=>"] type ["=" members]
members     = block of: signature | let | type_decl
class_members = block of: signature | let | type_decl | law
law         = "law" name { atom_pattern | premise } "where" goal
contract    = block of: "require" expr | "ensure" expr   // one entry inline
foreign     = "foreign" [mode] name "from" ("global" String | String String) [metadata]
            | "foreign" "blocked" name String
mode        = "pure" | "effect"
metadata    = indented lines: meta_name { atom }
proof       = "proof" name { atom_pattern | premise } "where" goal "=" block
premise     = "(" name ":" goal ")"
goal        = "same_type" atom atom | "lawful" type ["from" expr] | expr
```

`proof` формулирует свойство определений модуля — цель после `where` —
и доказывает его после `=`: `proof same where same_type a a = refl`.
Компилятор проверяет доказательство и стирает объявление: во время
выполнения у него нет значения. `equal`, `holds`, `same_type`, `lawful` и
`refl` — имена, а не ключевые слова.

`law` в классе, как и цель, формулирует то, что выполняет каждый экземпляр,
но не имеет доказательства; компилятор стирает закон. `proof` с целью
`lawful C T` доказывает законы `C` и его суперклассов для экземпляра, к
которому разрешается `C T`: тело — запись, которая называет доказательство
для каждого закона, а запись каждого суперкласса держит в поле этого
суперкласса, как словарь.

Контракт функции стоит после `where` между её параметрами и `=`:
`require e` — `Bool` от параметров, `ensure e` — функция от результата к
`Bool`; по пункту в строке или один пункт в той же строке. Компилятор
типизирует контракт по сигнатуре функции в отдельном контексте, поэтому
контракт ничего не добавляет к типу функции и её словарям и нигде не
исполняется. Проверяет его `flick prove`.

```flick
consume : Int -> Int -> Int
let consume available used
where
  require (0 <= used && used <= available)
  ensure (fun remaining -> 0 <= remaining && remaining + used == available)
=
  available - used
```

`module_application` объявляется только на уровне файла. Внутри
`inline_module` он вызывает `FLICK-R019`; `module_contract` там разрешён.

Правая часть `type` — всегда список вариантов.
Например, `type Wrapper = Wrap Float` — ADT с одним конструктором.
Прозрачный псевдоним, в том числе для типа записи, объявляется
через `type alias`. Слово `alias` считается ключевым только сразу после `type`.
Поле варианта записывается атомарным типом; для сложных типов нужны скобки:
`Some (List a)`, `Handler (a -> b)`. У `Circle (Point, Float)` одно
поле-кортеж, а у `Circle Point Float` — два поля.
Слово `of` перед полями вызывает `FLICK-S010`.

`type error E` объявляет ошибку с единственным конструктором `E`.
Ошибка с полями записывается как `type error E = E T …`.
Другое имя конструктора вызывает `FLICK-S007`.

Полная сигнатура именованной функции записывается непосредственно над её
`let` (см. [Прямые эффектные вызовы](../../spec/01-direct-calls/)).
Исключение — метод экземпляра класса: его сигнатура может быть взята
из класса. Парсер принимает сигнатуру без определения; их соответствие
проверяется на следующих этапах компиляции.

```flick
module App.Shapes exposing (Shape(..), area)

import App.Geometry exposing (Point)
import Effect.Log as Log
# import File exposing (Path)
# import JsError as FsError

type Shape =
  | Circle Point Float
  | Rect { origin : Point; size : Point }
  | Empty
  deriving Eq
# type alias FsError = FsError.JsError
# area : Shape -> Float
# let area _ = 0.0

opaque type alias UserId = String

refined type Port = Float where fun n -> n > 0 && n < 65536
  deriving Eq, Ord

kind Nullability = NonNull | Nullable

type error NotFound = NotFound UserId

service Clock =
  now : Unit ->{} Float

class Eq a => Hash a =
  hash : a -> Int

class Collection c a | c -> a =
  empty : c

instance Show Shape =
  let show shape =
    match shape with
    | Circle _ _ -> "circle"
    | _ -> "other"

read_text : Path ->{Async; FsError} String
foreign read_text from "./fs.js" "readText"
  arity 1
  catch FsError.from_js
  cancel signal

rec {
  is_even : Float -> Bool
  let is_even n = if n == 0 then true else is_odd (n - 1)

  is_odd : Float -> Bool
  let is_odd n = if n == 0 then false else is_even (n - 1)
}
```

## Атрибуты

Атрибут — строка `@name`, возможно с аргументами, перед объявлением, членом
или конструктором. Это метаданные для компилятора или плагина, а не имя:
его ничто не разрешает, и программа не может объявить новый.

```flick
@label gauge
type Gauge = Gauge Float
```

| Атрибут | Перед чем | Что сообщает | Подробнее |
| - | - | - | - |
| `@implicit` | экземпляр | экземпляр преобразует один тип в другой там, где ожидаемый тип известен | [неявные преобразования](../classes/#неявные-преобразования) |
| `@derive` | класс | его выводит плагин: плагин пакета класса или названного пакета, `@derive std` | [вывод экземпляров](../classes/#автоматический-вывод-экземпляров-классов) |
| `@ts` | тип или сервис | его форма в TypeScript, `none` или `import` типа из пакета | [форма типа](../codegen/#форма-типа) |
| `@stores` | тип без конструкторов и тела | его значения хранят переданные им функции | [квалификаторы](../regions/#квалификаторы) |
| `@label` | тип | его значения верхнего уровня несут свои имена во время выполнения; вид, `@label atom`, определяет, какие сборки их сохраняют | [наблюдаемость](../observability/#действия-ui-и-записи-атомов) |
| `@cell`, `@maker`, `@owner` | тип; значение; сервис | каждое значение типа хранит одно значение; значение создаёт атом, действие или представление; обработчик сервиса владеет тем, что создано под ним | [проверки вызовов](../plugins/#проверки-вызовов) |
| `@lang`, `@sync` | объявление; сервис | только во встроенных модулях: элемент, который компилятор знает по роли; сервис, операции которого выполняются сразу | [прямые вызовы](../../spec/01-direct-calls/) |

Неизвестный атрибут, атрибут не на своём месте или с недопустимыми
аргументами вызывает `FLICK-S014`. При наведении на атрибут редактор
показывает, что он сообщает.

## Типы, строки и кинды

```text
type        = "forall" name { name } "." type
            | "exists" Con ":" contract "." type    // свидетель `unpack`
            | union "=>" type                  // constraint(s): `Show a =>`, `(Show a, Eq a) =>`
            | ["~"] union ["->" [effects] type] // right associative; `~` marks a suspended parameter
effects     = "{" [row] [";" [row]] "}"     // written `->{` with no space
row         = union { "," union } ["|" union]  // `none`, `r`, `Users, Log | r`
union       = app { "+" app }                  // row or capture union
app         = capture { capture }
capture     = atom { "^" (name | "(" type ")" | "{" … "}") }
atom        = name | Con { "." Con } | "none" | "(" ")" | "(" type ")"
            | "(" type "," type { "," type } ")"
            | "{" field ":" type { (";" | line break) field ":" type } ["|" union] "}"
            | "{" [union { "," union }] ["|" union] "}"
```

- `a -> b` — чистая функция. `a ->{r; e} b` — эффектная функция, которой
  нужны сервисы `r` и которая может завершиться с ошибками `e`.
  `a ->{} b` — эффектная функция с двумя пустыми строками; такая стрелка
  не совпадает с `->` (см. [Спецификацию](../../spec/),
  [Значения вычислений и их выполнение](../../spec/02-computations/)).
  Если между `-> {` есть пробел, это стрелка к типу записи. Запись вплотную
  к стрелке, `->{ x : Int }`, — это `FLICK-S007`, а строка с пробелом,
  применённая к результату, `-> {r; e} b`, — `FLICK-T002`; исправление
  каждой добавляет или убирает пробел.
- `none` — литерал пустой строки. Пропущенная сторона стрелки тоже пуста:
  `->{; E}` — это `->{none; E}`, `->{R}` — это `->{R; none}`
  (см. [эффекты](../effects/#пустые-строки-none)). `{Users, Log}` —
  закрытая строка; хвост `| r` делает её открытой.
- `T^c`, `T^{s}`, `T^(c + d)` задают верхнюю границу захватов значения.
  Приоритет `^` выше применения, поэтому `List a^c` — это `List (a^c)`.
- Кинды записываются по той же грамматике: `Type`, `Type -> Type`, `ServiceRow`.

```flick
# type alias User = { name : String }
#   deriving Schema
# type alias Settings = { theme : String }
#   deriving Schema
# type error NotFound = NotFound
# type error SaveError = SaveError
# service Users =
#   get : String ->{; NotFound} User
# service Log =
#   info : String ->{} Unit
# import App.Ui exposing (SaveProps)
#
decode_pair :
  (forall a. Schema a -> Unknown ->{r; SchemaError} a) ->
  Unknown -> Unknown ->{r; SchemaError} (User, Settings)
# let decode_pair decode user settings = (decode User.schema user, decode Settings.schema settings)

open_rows : a ->{Users, Log | r; NotFound | e} b
# let open_rows _ = todo

fields : forall fields. { name : String | fields } -> { name : String }
# let fields record = { name = record.name }

save_with :
  SaveProps c -> (Eff {Users} {SaveError} Unit)^c
# let save_with _ = task { () }
```

## Выражения

```text
block       = statement { line break statement }
statement   = "let" ["lazy"] ["mut"] … | signature | name "=" block | expr
expr        = "fun" atom_pattern { atom_pattern } "->" block
            | [label [":"]] "loop" "{" block "}"
            | [label [":"]] "while" expr "do" block
            | "break" [label] [expr] | "continue" [label]
            | "match" expr "with" arm { arm }
            | "try" block "with" handler_arm { handler_arm }
            | "if" expr "then" block [ "else" (if_expr | block) ]
            | "pack" ModulePath "as" contract | "unpack" expr "as" Con ["," name] "in" block
            | "lazy" expr                    // binds as `await`: `lazy f x` is `lazy (f x)`
            | operator expression over application [ "catch" arm { arm } ] (see the operator table)
arm         = "|" or_pattern ["when" expr] "->" block
handler_arm = arm | "|" "return" or_pattern "->" block
            // ветка операции — `| E.op p… -> …`, образец, чей путь кончается именем
application = postfix { postfix }
postfix     = atom { "." name | "?" }           // без пробела перед ними
atom        = name | Con { "." Con } ["." name] | Float | BigInt | String
            | "true" | "false" | "()" | "(" expr ")" | "(" expr ":" type ")"
            | "(" expr "," expr { "," expr } ")" | "[" [expr { "," expr } [","]] "]"
            | "{" "}" | "{" field "=" block { ";" | line break … } "}"
            | "{" expr "with" field "=" block { … } "}"
            | "task" "{" block "}" | "lazy" "{" block "}" | "ui" "{" block "}"
            | "do" "{" do_stmt { ";" | line break … } "}"
            | [label [":"]] "for" ("{" clause_list "}" | clause_list) ("yield" expr | "do" block)
            | "scope" [name] "{" block "}" | "provide" expr "{" block "}"
            | head { head_arg } ["{" { token } "}"]
                                            // голова, которую импорт открывает как `plugin head`, с
                                            // аргументом или телом; `cli` с телом
head_arg    = name | Con | Number | String | "." | "(" { token } ")"   // на строке головы
do_stmt     = bind { "and" bind } | "if" expr | statement
bind        = or_pattern "<-" expr
clause_list = for_clause { ";" | line break | before "if" … for_clause }
for_clause  = or_pattern "<-" expr | "if" expr | let_decl
clause      = ("from" | "join" | "left_join") name "in" expr
            | clause_word expr { "," expr }
```

`do { … }` выполняет инструкции через `Monad` из прелюдии: `p <- e` —
это `flat_map (fun p -> rest) e`, выражение без образца выполняется без связывания его результата с именем,
`if c` без `then` — это `if c then rest else nothing` из `Alternative`,
`let` действует до конца блока, а последняя инструкция — выражение монады
блока. Образец после `<-` должен быть неопровержимым, а остаток блока — чистая
функция: эффектный вызов в нём даёт `FLICK-E001` (см. [`do`, `for` и списковые выражения](../../spec/04-higher-order/#do-for-и-списковые-выражения)).
`p <- a and q <- b` связывает оба значения независимо через `map2` из
`Applicative`: ни одно из них не видит `p` и `q`, а группе, за которой идёт
только `pure e`, `Monad` не нужен. `and` — ключевое слово только там; в других
местах это имя.
`for { x <- xs; if p x } yield e` — ленивый `Seq`: один генератор выполняет
клаузы при каждой итерации. `x <- source` берёт элементы источника с
`Iterable` (`List`, `Seq`) и пропускает те, что не подходят под образец,
`if c` пропускает элемент, если `c` ложно, `let` действует до конца; клаузы
делают не больше чем Sync-вызовы (`Atom.Read`) и читают сервисы стадии, которая
строит последовательность. `[for … yield e]` — `for`, единственный в скобках, — строгий `List` тех
же элементов, собранный циклом. Без фигурных скобок клаузы разделяются `;`,
а `if` фильтра начинает новую. `do body` вместо `yield e` выполняет тело —
блок типа `Unit` — для каждого элемента прямо на месте: клаузы и тело могут
делать эффектные вызовы, их строки объединяются со строками функции, а цикл —
это `for…of` внутри её вычисления.
`<-` вне блока `do` или `for` — `FLICK-S013`; сравнение с отрицательным числом пишется
`a < -1`.

Постфиксный `e?`, записанный сразу после операнда, разворачивает его. От
`Result (Errors e) a` в эффектной функции это `from_result e`: значение или
сбой с ошибкой, которая попадает в строку ошибок функции. От `Option a` в
функции, возвращающей `Option`, это значение `Some`, а `None` сразу возвращает
`None` из функции. В остальных местах это ошибка (в том числе внутри `task`,
`scope`, `provide`, `try` и `for`, которые выполняются отдельно). `f x?` — это
`f (x?)`; пишите `(f x)?`.

`todo` (с необязательной строкой после него) — выражение любого типа, а
`echo` без операнда — функция, как в `x |> echo`. `_` — тип только в
сигнатуре или аннотации.

`if` — выражение; без `else` оно имеет тип `Unit`, как и его ветвь `then` (на `else ()` выдаётся предупреждение `FLICK-T028`). Ветвь в фигурных скобках содержит инструкции: `if a > b then { low = b; high = a }`. `ignore e` отбрасывает
результат промежуточной инструкции, отличный от `Unit`
(см. [Прямые эффектные вызовы](../../spec/01-direct-calls/)).
`await e` один раз запускает сохранённый `Eff` в текущем файбере,
а `task { … }` создаёт вычисление, не выполняя его
(см. [Значения вычислений и их выполнение](../../spec/02-computations/)).
В `provide e { … }` скобка `{` завершает выражение
перед блоком.

Аргументы и тело блока плагина сохраняются как последовательности токенов, которые затем читает плагин (см.
[плагины](../plugins/)). `cli "name" { … }` — блок плагина пакета `std`: он
читает предложения и проверяет псевдонимы у `arg` и повторяющиеся имена, а
опции проверяют типы `Cli`
(см. [CLI eDSL: флаги, аргументы и подкоманды](../../spec/08-type-programming-and-edsl/#cli-edsl-флаги-аргументы-и-подкоманды)).
Комментарий `///` непосредственно над предложением `flag`, `arg`
или `subcommand` задаёт текст справки для него.

```flick ignore several statements shown without the declarations around them
load_twice : UserId ->{Users; NotFound} (User, User)
let load_twice id =
  let request = task { Users.get id }
  let first = await request
  let renamed = { first with name = "Ada" }
  (renamed, await request)

find_optional : UserId ->{Users; DbUnavailable} Option User
let find_optional id =
  try
    Some (find_user id)
  with
  | NotFound _ -> None

read_config : Path ->{FileSystem; OpenError, ReadError} String
let read_config path =
  scope {
    let file = File.open_scoped path
    File.read_all file
  }

balances : Sql.Template Pg { user_id : UserId } BalanceRow
let balances =
  sql params {
    from user in Users
    left_join account in Accounts
      on account.user_id == user.id
    where user.id == params.user_id
    select { name = user.name; balance = account.balance }
  }

deploy : Cli.Command {Platform} none
let deploy =
  cli "deploy" {
    description "Deploy the current build"
    /// Print the plan without applying it
    flag dry_run "d" : Bool
    flag region : Option String
    arg targets : NonEmpty String
    handle fun { dry_run; region; targets } -> Deploy.run dry_run region targets
  }
```

<div id="шаблоны"></div>

## Образцы

```text
or_pattern   = pattern { "|" pattern }
pattern      = Con { "." Con } { atom_pattern } | atom_pattern
atom_pattern = "_" | name | Con | ["-"] Float | ["-"] BigInt | String | "true" | "false"
             | "()" | "(" or_pattern ")" | "(" or_pattern ":" type ")"
             | "(" or_pattern "," or_pattern { "," or_pattern } ")"
             | "[" [list_item { "," list_item }] "]"
list_item    = or_pattern | "..." (name | "_")
             | "{" field ["=" or_pattern] { ";" | line break … } "}"
```

Поле образца записи без `= pattern` связывает переменную с именем поля.
`...rest` может стоять только последним в образце списка и связывает
остаток списка.
Образцы с альтернативами и условия ветвей следуют правилам Rust
(см. [Варианты, условия веток и альтернативные образцы](../../spec/06-typescript-and-ffi/#варианты-условия-веток-и-альтернативные-образцы)).
Все альтернативы связывают одни и те же имена. Одно имя нельзя связать
дважды в образце или в параметрах функции (`FLICK-S012`).
Условие `when e` относится ко всей ветви.
Внутри образца ветви `|` в начале строки продолжает список альтернатив;
после `->` он начинает следующую ветвь.

```flick
# type alias Point = { x : Float; y : Float }
# type Shape =
#   | Circle Point Float
#   | Rect Float Float
#   | Labeled { name : String }
#   | Empty
#
describe : Shape -> String
let describe shape =
  match shape with
  | Circle _ _ | Empty -> "round or nothing"
  | Rect width height when width == height -> "square"
  | Rect _ _ -> "rectangle"
  | Labeled { name = "origin" | "zero" } -> "special"
  | _ -> "other"
```

## Конструкции, которых нет во Flick

Блок `do` без фигурных скобок, операторы связывания `let*` и JSX `<…>`
вызывают по одной диагностике `FLICK-S010`, а не общую ошибку разбора.
Сообщение предлагает соответствующую конструкцию Flick:
`do { … }`, прямые вызовы с `let`, `task`/`await` или `ui { … }`.

## Восстановление после ошибок

Каждой лексической ошибке соответствуют одна диагностика и один токен.
После синтаксической ошибки парсер пропускает остаток инструкции
и продолжает со следующей инструкции или объявления. Так он сообщает
обо всех независимых ошибках, не порождая каскад сообщений
из-за одной ошибки в той же инструкции.

## Форматирование

`flick fmt` приводит файл к каноническому форматированию, как `gleam format`:
результат зависит только от кода и ширины строки (100 столбцов), а не от того,
как был расположен исходник, за исключением нескольких решений автора,
перечисленных ниже. Литералы, многострочные строки и комментарии копируются
как есть. Форматтер убирает избыточный внешний `forall` из сигнатуры
нерекурсивного `let`, в том числе метода `instance`, если переменные остаются
неявно квантифицированными; `forall` сохраняется, когда он нужен для рекурсии,
при затенении переменной типа или при комментарии. Остальные изменения — разделители,
которые ничего не меняют: `;` между инструкциями становится переводом строки,
как и `;` между полями многострочной записи, в многострочной конструкции в скобках
добавляется завершающая `,`, многострочный список вариантов начинается с `|`, а пустые строки
эффектной стрелки сокращаются без изменения типа: `->{none; E}` становится
`->{; E}`, `->{R; none}` — `->{R}`, а `->{none; none}` — `->{}`.
Запись `Eff none none a` остаётся без изменений. Вызов в круглых скобках и тот
же вызов на отдельной строке под вызовом, аргументом которого он является, —
одна и та же программа, как их читает правило 8. Форматтер повторно разбирает каждое
объявление и весь файл и, если AST или комментарии изменились бы, завершается
с `FLICK-P003`.

Отступ — два пробела на уровень. Конструкция остаётся на одной строке, если
помещается; иначе она разбивается, начиная с самой внешней:

- значение после `=` (`let`, поля записей, синонимы типов), после `->` (`fun`,
  ветви `match`) или после `<-` переносится на следующую строку с отступом.
  Записи, списки, кортежи, `fun` и блоки в фигурных скобках (`task`, `ui`,
  `scope`, `provide`, `do`, `for`, `sql`, блок плагина) остаются после `=`
  и разбиваются внутри: `let config = {` … `}`. Несколько инструкций всегда
  начинаются со следующей строки, как и `match`, `try` и не помещающийся `if`;
- инструкции идут по одной на строку; блок в фигурных скобках с одним
  выражением может остаться на одной строке: `task { load id }`;
- применение функции ставит аргументы по одному на строку с отступом. Если
  последний аргумент — функция в круглых скобках, список, запись, кортеж или
  блок в фигурных скобках и всё до его первого переноса помещается, вызов
  остаётся на своей строке и разбивается только аргумент:
  `Ref.modify counter (fun current ->`, тело и `)` на отдельной строке. Вызов
  в круглых скобках остаётся вплотную к ним;
- в теле, где строки под вызовом — его аргументы (правило 8), как в
  `ui { … }`, вызов оставляет аргументы на своей строке. Если он не
  помещается, вызовы в круглых скобках в конце его аргументов переносятся на
  отдельные строки под ним с отступом, без скобок: `div props (p a) (p b)`
  становится `div props`, а под ним `p a` и `p b`. Вызов в круглых скобках,
  стоящий на такой строке один, теряет их, если его первая строка помещается.
  Переносится только вызов имени, но не `(-x)`, `if`, `match` или `fun`;
  вызовы, среди которых есть комментарий, и всё внутри скобок остаются как
  есть;
- цепочка операторов разбивается перед каждым оператором с самым слабым
  приоритетом, продолжения получают отступ; конвейер из `|>` или `<|`
  разбивается по одному шагу на строку, и его операторы стоят под первым
  операндом, если конвейер начинает строку;
- `if c then a else b` остаётся на одной строке или помещает каждую ветвь
  на свои строки; `match`, `try` и постфиксный `catch` ставят каждую ветвь
  на отдельную строку (а `catch` заключает в скобки смешанный конвейер слева,
  чтобы группировка была явной). Перед `,`, где парсер не принимает блок на отдельных
  строках, последняя ветвь остаётся на своей строке;
- списки, кортежи и `exposing (…)` ставят по одному элементу на строку
  с `,` после каждого; записи, типы записей, образцы и обновления
  (`base with` на отдельной строке) ставят по одному полю на строку без `;`,
  как блок ставит свои инструкции, хвост строки `| r` — последним; в одну
  строку поля разделяются `; `;
- сигнатура переносит тип на следующую строку, затем `forall a.`, контекст
  и каждый параметр функционального типа получают свои строки; в остальных
  местах функциональный тип разбивается после стрелок с отступом, а `->{r; e}`
  не разрывается;
- варианты стоят на одной строке, если помещаются, иначе по одному на строку
  после `|`; `deriving` занимает отдельную строку с отступом, кроме случая
  после `}` многострочной записи; члены классов, экземпляров, сервисов и `rec`,
  метаданные `foreign` и столбцы `sql table` всегда идут по одному на строку;
  в `sql` предложение `on` остаётся после своего `join`, если помещается.

Решения автора, как в Gleam: завершающая `,` (`[a, b,]`), перевод строки сразу
после `{` записи или пустая строка между элементами сохраняет многострочную
запись конструкции — по одному элементу на строку; список констант, записанный по несколько на строке
с завершающей `,`, заполняется по ширине; конвейер, у которого шаг записан
на отдельной строке, а `|>` или `<|` стоит в начале или в конце строки,
остаётся по шагу на строку, и каждый оператор переносится в начало своей
строки. Пустые строки между
объявлениями, инструкциями, членами, ветвями, полями и элементами
сохраняются, несколько подряд — как одна. Комментарий на отдельной строке
остаётся на отдельной строке перед тем, что за ним следует; комментарий
в конце строки остаётся в конце этой строки, а конструкция вокруг него
разбивается. Если однострочный комментарий нельзя разместить по правилам
форматирования, объявление сохраняется в исходном виде.

`flick fmt --check` завершается с кодом `1` и сообщает `FLICK-P002`
для каждого файла, который изменился бы после форматирования.
Для интеграции с редакторами команда `flick fmt --stdin --stdin-filepath <file>`
читает исходник из stdin и записывает результат в stdout.
