---
title: "Грамматика Flick"
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
  Все три вида комментариев допустимы и внутри `sql { … }`.

### Имена

| Токен | Форма | Назначение |
| - | - | - |
| имя | `[a-z_][A-Za-z0-9_]*` | значения, функции, поля, переменные типов |
| имя с заглавной буквы | `[A-Z][A-Za-z0-9_]*` | типы, конструкторы, модули, классы, сервисы, ошибки |
| `_` | отдельно | образец, который не связывает имя |

В квалифицированных именах вокруг `.` нет пробелов: `Users.get`,
`Ui.Event.exhaust`, `Sql.Template`. Запись `user.name` обращается к полю.

### Ключевые слова

```text
module exposing import as let rec fun match with try if then else task await
scope provide ignore todo echo type opaque refined where kind error service class
instance deriving foreign from forall not none true false schema codec
sql ui cli in when do
```

Слово `do` зарезервировано, но нотации `do` во Flick нет.
При его использовании выдаётся отдельная диагностика `FLICK-S010`.

Контекстные слова считаются обычными именами, кроме следующих позиций:

- `unboxed` и `untagged` непосредственно перед `type` (см. `codegen.md`);
- режимы внешних функций `pure`, `sync`, `async`, `effect`, `blocked` и источник
  `global`;
- метаданные внешних функций `arity`, `this`, `catch`, `cancel`, `nullable`,
  `retention`, `optional`, `variadic`, `reviewed` (неизвестные метаданные —
  `FLICK-S011`);
- слова предложений SQL внутри `sql { … }`: `join`, `left_join`, `on`, `group_by`,
  `having`, `order_by`, `limit`, `offset`, `select`, `union`, `union_all`,
  `insert_into`, `values`, `update`, `set`, `delete_from`, `returning`,
  а также ключевые слова `from` и `where`. В блоке SQL такое слово завершает
  применение функции, поэтому предложение можно продолжить на следующей строке;
- `table` сразу после `sql` на верхнем уровне и `via` после типа столбца
  `sql table` (см. [SQL](../sql/));
- слова предложений CLI в начале предложения внутри `cli { … }`: `description`,
  `flag`, `arg`, `subcommand`, `handle`.

### Числовые литералы

Числовые литералы следуют грамматике JavaScript `NumericLiteral`
и обозначают в точности те же числа, что и в JS
(см. [Schema и стандартные типы данных](../../spec/09-schema/)):

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

Интерполяции нет.

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

Переводы строк исходного файла внутри строковых литералов преобразуются
в `\n`, даже если в файле используется `\r\n`. Выражения, состоящие только
из литералов, вычисляются при компиляции по правилам JS
(см. [Вычисление литеральных выражений](../../spec/08-typescript-and-ffi/#вычисление-литеральных-выражений)):
`60 * 60 * 24`, `"a" ++ "b"`, `not false`.

```flick
let examples =
  [
    "tab\t and \u{1F600}",
    r"C:\temp",
    r#"say "hi""#,
    """
    first line
      indented line
    """,
  ]
```

### Операторы и пунктуация

```text
( ) [ ] { } , ; : . .. = -> => | ^ _
|> || && == != < <= > >= ++ + - * / % ** not
```

## Таблица операторов

Собственные операторы объявлять нельзя. Ниже они перечислены от самого слабого приоритета к самому сильному:

| Уровень | Операторы | Ассоциативность |
| - | - | - |
| 1 | `\|>` | левая |
| 2 | `\|\|` | правая |
| 3 | `&&` | правая |
| 4 | `==` `!=` `<` `<=` `>` `>=` | отсутствует: `a < b < c` — ошибка |
| 5 | `++` | правая |
| 6 | `+` `-` | левая |
| 7 | `*` `/` `%` (`%` — остаток от деления JS) | левая |
| 8 | `**` | правая |
| 9 | префиксные `-`, `not`, `await`, `ignore`, `echo` | — |
| 10 | применение функции `f x y` | левая |
| 11 | `.field`, `Module.name` | левая |

`-x ** 2` означает `-(x ** 2)`; `-a * b` означает `(-a) * b`; `await f x`
означает `await (f x)`. Побитовые операции — функции `Number.bit_*`.

## Правила отступов

Границы тел функций, ветвей сопоставления и членов `class`, `instance`
и `service` определяются отступами. `in` не используется; последнее
выражение блока задаёт его значение
(см. [Прямые эффектные вызовы](../../spec/03-direct-calls/)).
Фигурные скобки ограничивают записи и блоки `task`, `scope`, `provide`,
`sql`, `ui`, `cli`, `codec` и `rec`.

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
     Любая другая строка завершает инструкцию.
3. Строка в столбце блока продолжает инструкцию, если начинается
   с инфиксного оператора, кроме `-`, с `|`, `then`, `else`, `with`
   или с закрывающего разделителя. Поэтому конвейеры `|>`, ветви `match`
   и ветви `else` могут начинаться в том же столбце, что и их выражение.
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

```flick
load_view : UserId ->{Users, Log; NotFound} UserView
let load_view id =
  let user = Users.get id
  let view = to_view user
  Log.info view.title
  view

classify : Number -> String
let classify n =
  if n < 0 then
    "negative"
  else if n == 0 then
    "zero"
  else
    "positive"

total : List Number -> Number
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
file        = [module_header] { import } { decl }
module_header = "module" ModulePath ["exposing" exposing]
import      = "import" ModulePath ["as" Con] ["exposing" exposing]
exposing    = "(" item { "," item } [","] ")"
item        = name | Con | Con "(..)" | ".."
ModulePath  = Con { "." Con }

decl        = signature | let | rec_group | type_decl | kind_decl | error_decl
            | service | class | instance | foreign | sql_table
signature   = name ":" type
let         = "let" ["rec"] (name { atom_pattern } | pattern) [":" type] "=" block
rec_group   = "rec" "{" { signature | let } "}"
type_decl   = ["opaque"] "type" Con { param } ["=" variants] [deriving]
            | ["opaque"] "type" "alias" Con { param } "=" type [deriving]
            | "refined" "type" Con { param } "=" type "where" expr [deriving]
            | ["opaque"] ("unboxed" | "untagged") "type" Con { param } "=" variants [deriving]
variants    = ["|"] variant { "|" variant }
variant     = Con { capture }                   // fields are atomic types
param       = name | "(" name ":" kind ")"
deriving    = "deriving" Con { "," Con }
kind_decl   = "kind" Con "=" ["|"] Con { "|" Con }
error_decl  = "type" "error" Con { param } ["=" variant] [deriving]
service     = "service" Con { param } [ "=" members ]
class       = "class" [context "=>"] Con { param } ["|" fundep { "," fundep }] ["=" members]
fundep      = { name } "->" { name }
instance    = "instance" [context "=>"] type ["=" members]
members     = block of: signature | let | type_decl
foreign     = "foreign" mode name "from" ("global" String | String String) [metadata]
            | "foreign" "blocked" name String
mode        = "pure" | "sync" | "async" | "effect"
sql_table   = "sql" "table" Con String "{" column { column } "}"
column      = name ":" { name | "not" } "as" type ["via" expr]
metadata    = indented lines: meta_name { atom }
```

Правая часть `type` — всегда список вариантов.
Например, `type Wrapper = Wrap Number` — ADT с одним конструктором.
Прозрачный псевдоним, в том числе для типа записи, объявляется
через `type alias`. Слово `alias` считается ключевым только сразу после `type`.
Поле варианта записывается атомарным типом; для сложных типов нужны скобки:
`Some (List a)`, `Handler (a -> b)`. У `Circle (Point, Number)` одно
поле-кортеж, а у `Circle Point Number` — два поля.
Слово `of` перед полями вызывает `FLICK-S010`.

`type error E` объявляет ошибку с единственным конструктором `E`.
Ошибка с полями записывается как `type error E = E T …`.
Другое имя конструктора вызывает `FLICK-S007`.

Полная сигнатура именованной функции записывается непосредственно над её
`let` (см. [Прямые эффектные вызовы](../../spec/03-direct-calls/)).
Исключение — метод экземпляра класса: его сигнатура может быть взята
из класса. Парсер принимает сигнатуру без определения; их соответствие
проверяется на следующих этапах компиляции.

```flick
module App.Shapes exposing (Shape(..), area)

import App.Geometry exposing (Point)
import Effect.Log as Log

type Shape =
  | Circle Point Number
  | Rect { origin : Point; size : Point }
  | Empty
  deriving Eq, Show

opaque type alias UserId = String

refined type Port = Number where fun n -> n > 0 && n < 65536
  deriving Eq, Hash

kind Nullability = NonNull | Nullable

type error NotFound = NotFound UserId

service Clock =
  now : Unit ->{} Number

class Eq a => Hash a =
  hash : a -> Number

class Collection c a | c -> a =
  empty : c

instance Show Shape =
  let show shape =
    match shape with
    | Circle _ _ -> "circle"
    | _ -> "other"

read_text : Path ->{; FsError} String
foreign async read_text from "./fs.js" "readText"
  arity 1
  catch FsError.from_js
  cancel signal

rec {
  is_even : Number -> Bool
  let is_even n = if n == 0 then true else is_odd (n - 1)

  is_odd : Number -> Bool
  let is_odd n = if n == 0 then false else is_even (n - 1)
}
```

## Типы, строки и кинды

```text
type        = "forall" name { name } "." type
            | union "=>" type                  // constraint(s): `Show a =>`, `(Show a, Eq a) =>`
            | union ["->" [effects] type]      // right associative
effects     = "{" "}" | "{" [row] ";" [row] "}" // written `->{` with no space
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
  не совпадает с `->` (см. [Решения первой версии](../../spec/02-decisions/),
  [Значения вычислений и их выполнение](../../spec/04-computations/)).
  Если между `-> {` есть пробел, это стрелка к типу записи.
- `none` — литерал пустой строки. Пропущенная сторона стрелки тоже пуста:
  `->{; E}` — это `->{none; E}`, `->{R;}` — это `->{R; none}`
  (см. [эффекты](../effects/#пустые-строки-none)). `{Users, Log}` —
  закрытая строка; хвост `| r` делает её открытой.
- `T^c`, `T^{s}`, `T^(c + d)` задают верхнюю границу захватов значения.
  Приоритет `^` выше применения, поэтому `List a^c` — это `List (a^c)`.
- Кинды записываются по той же грамматике: `Type`, `Type -> Type`, `ServiceRow`.

```flick
decode_pair :
  (forall a. Schema a -> Unknown ->{r; SchemaError} a) ->
  Unknown -> Unknown ->{r; SchemaError} (User, Settings)

open_rows : a ->{Users, Log | r; NotFound | e} b

fields : forall fields. { name : String | fields } -> { name : String }

save_button :
  SaveButtonProps c -> (Ui {Users} {SaveError})^c
```

## Выражения

```text
block       = statement { line break statement }
statement   = "let" … | signature | expr
expr        = "fun" atom_pattern { atom_pattern } "->" block
            | "match" expr "with" arm { arm }
            | "try" block "with" arm { arm }
            | "if" expr "then" block "else" (if_expr | block)
            | operator expression over application (see the operator table)
arm         = "|" or_pattern ["when" expr] "->" block
application = postfix { postfix }
postfix     = atom { "." name }
atom        = name | Con { "." Con } ["." name] | Number | BigInt | String
            | "true" | "false" | "()" | "(" expr ")" | "(" expr ":" type ")"
            | "(" expr "," expr { "," expr } ")" | "[" [expr { "," expr } [","]] "]"
            | "{" "}" | "{" field "=" block { ";" | line break … } "}"
            | "{" expr "with" field "=" block { … } "}"
            | "task" "{" block "}" | "ui" "{" block "}"
            | "scope" [name] "{" block "}" | "provide" expr "{" block "}"
            | "codec" postfix "{" field "=" block … "}" | "schema" Con { "." Con }
            | "sql" [name] "{" clause { clause } "}"
            | "cli" String "{" cli_clause { cli_clause } "}"
clause      = ("from" | "join" | "left_join") name "in" expr
            | clause_word expr { "," expr }
cli_clause  = ("flag" | "arg") name { String } ":" type ["=" expr]
            | ("description" | "subcommand" | "handle") expr
```

`todo` (с необязательной строкой после него) — выражение любого типа, а
`echo` без операнда — функция, как в `x |> echo`. `_` — тип только в
сигнатуре или аннотации.

`if` — выражение с обязательной ветвью `else`. `ignore e` отбрасывает
результат промежуточной инструкции, отличный от `Unit`
(см. [Прямые эффектные вызовы](../../spec/03-direct-calls/)).
`await e` один раз запускает сохранённый `Eff` в текущем файбере,
а `task { … }` создаёт вычисление, не выполняя его
(см. [Значения вычислений и их выполнение](../../spec/04-computations/)).
В `provide e { … }` и `codec e { … }` скобка `{` завершает выражение
перед блоком.

В `cli "name" { … }` парсер проверяет только форму предложений.
Псевдонимы у `arg`, повторяющиеся имена и типы параметров проверяются
при развёртывании CLI
(см. [CLI eDSL: флаги, аргументы и подкоманды](../../spec/10-type-programming-and-edsl/#cli-edsl-флаги-аргументы-и-подкоманды)).
Комментарий `///` непосредственно над предложением `flag`, `arg`
или `subcommand` задаёт текст справки для него.

```flick
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
atom_pattern = "_" | name | Con | ["-"] Number | ["-"] BigInt | String | "true" | "false"
             | "()" | "(" or_pattern ")" | "(" or_pattern ":" type ")"
             | "(" or_pattern "," or_pattern { "," or_pattern } ")"
             | "[" [or_pattern { "," or_pattern }] "]"
             | "{" field ["=" or_pattern] { ";" | line break … } "}"
```

Поле образца записи без `= pattern` связывает переменную с именем поля.
Образцы с альтернативами и условия ветвей следуют правилам Rust
(см. [Варианты, условия веток и альтернативные образцы](../../spec/08-typescript-and-ffi/#варианты-условия-веток-и-альтернативные-образцы)).
Все альтернативы связывают одни и те же имена. Одно имя нельзя связать
дважды в образце или в параметрах функции (`FLICK-S012`).
Условие `when e` относится ко всей ветви.
Внутри образца ветви `|` в начале строки продолжает список альтернатив;
после `->` он начинает следующую ветвь.

```flick
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

Нотация `do`, операторы связывания `let*` и JSX `<…>` вызывают
по одной диагностике `FLICK-S010`, а не общую ошибку разбора.
Сообщение предлагает соответствующую конструкцию Flick:
прямые вызовы с `let`, `task`/`await` или `ui { … }`.

## Восстановление после ошибок

Каждой лексической ошибке соответствуют одна диагностика и один токен.
После синтаксической ошибки парсер пропускает остаток инструкции
и продолжает со следующей инструкции или объявления. Так он сообщает
обо всех независимых ошибках, не порождая каскад сообщений
из-за одной ошибки в той же инструкции.

## Форматирование

`flick fmt` меняет пробельные символы и разделители в концах строк,
сохраняя AST. Форматтер повторно разбирает результат и, если AST изменился,
завершается с `FLICK-P003`. Кроме того, он сокращает запись пустых строк
эффектной стрелки без изменения типа: `->{none; E}` становится `->{; E}`,
а `->{none; none}` — `->{}`. Записи `->{R; none}` и `Eff none none a`
остаются без изменений.

- Отступ — два пробела на уровень. При этом сохраняется положение начала
  каждой строки относительно внешних блоков: левее, в том же столбце или правее.
- Между токенами ставится один пробел. Внутри `( )`, `[ ]`, `->{r; e}`
  и строк `{Users, Log}` нет пробелов сразу после открывающей или перед
  закрывающей скобкой. Вокруг `.` и `^` пробелов тоже нет.
  Внутри остальных фигурных скобок ставится один пробел у каждого края.
- Унарный `-` примыкает к операнду; в `forall a b.` сохраняется пробел после `.`.
- Пробелы в концах строк удаляются, несколько пустых строк подряд заменяются
  одной. Файл заканчивается одним переводом строки. Комментарии остаются
  на своих местах; строка только с комментарием получает тот же отступ,
  что и код в её столбце.
- Многострочные строки и блочные комментарии копируются без изменений.
- Строка длиннее 100 символов разбивается начиная с самой внешней конструкции.
  Тело после `=` или `->` переносится на отдельную строку, в цепочке `if`
  каждая ветвь получает свою строку. Затем переносы ставятся перед операторами
  с самым слабым приоритетом (`|>` выравнивается по началу блока), после
  стрелок типов (с сохранением `->{r; e}` целиком), перед аргументами
  применения и, наконец, между элементами внутри внешних скобок —
  по одному элементу на строку. Единственное выражение в круглых скобках
  остаётся вплотную к ним. Продолжение получает дополнительный отступ
  в два пробела. Перенос сохраняется, только если AST не изменился.
  Отдельный токен длиннее ограничения остаётся без изменений.
- В записи, типе записи и образце записи после поля в конце строки ставится
  `;`, в том числе перед `}`. После хвоста строки `| r` разделитель не ставится.
  В списке, кортеже и `exposing (…)` последний элемент заканчивается `,`,
  если закрывающая скобка находится на следующей строке, как в prettier.
  В однострочных записях и списках завершающий разделитель не ставится.
  Парсер принимает эти разделители, в том числе `;` после поля со значением
  в виде многострочного блока. AST с разделителями и без них одинаков.

`flick fmt --check` завершается с кодом `1` и сообщает `FLICK-P002`
для каждого файла, который изменился бы после форматирования.
Для интеграции с редакторами команда `flick fmt --stdin --stdin-filepath <file>`
читает исходник из stdin и записывает результат в stdout.
