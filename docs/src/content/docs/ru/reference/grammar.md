---
title: "Грамматика Flick"
---

Это поверхностный синтаксис, который принимают `flick check` и `flick fmt`.
Ссылки на разделы (§N) указывают на спецификацию языка
`Flick спецификация ML языка с HKT и Effect.md`. Каждый блок ```` ```flick ````
в этом файле и в спецификации разбирается набором тестов
(`flick-core/tests/corpus.rs`); если блок перестаёт разбираться, сборка завершается ошибкой.

## Лексическая структура

Исходный файл должен быть в UTF-8 (иначе `FLICK-S001`). Позиции в диагностике —
диапазоны байтов UTF-8; столбцы для правил отступов считаются в символах.

### Пробельные символы и комментарии

- Пробелы разделяют токены. Табуляции запрещены везде вне строк и
  комментариев (`FLICK-S009`), поскольку правила отступов зависят от столбцов.
- Переводы строк — `\n` или `\r\n`.
- `// …` — однострочный комментарий, `/// …` — комментарий документации
  (используется командой `flick doc`), `/* … */` — блочный комментарий;
  блочные комментарии допускают вложенность. Те же комментарии работают
  внутри `sql { … }`.

### Имена

| Токен | Форма | Назначение |
| - | - | - |
| имя | `[a-z_][A-Za-z0-9_]*` | значения, функции, поля, переменные типов |
| имя с заглавной буквы | `[A-Z][A-Za-z0-9_]*` | типы, конструкторы, модули, классы, сервисы, ошибки |
| `_` | отдельно | шаблон-пропуск |

Полные имена пишутся без пробелов вокруг `.`: `Users.get`,
`Ui.Event.exhaust`, `Sql.Template`. `user.name` — доступ к полю.

### Ключевые слова

```text
module exposing import as let rec fun match with try if then else task await
scope provide ignore type opaque refined where kind error service class
instance deriving foreign from forall not none true false schema codec
sql ui cli in when do
```

`do` зарезервировано только для того, чтобы его использование получало точную
диагностику (`FLICK-S010`); в Flick нет нотации `do`.

Контекстные слова — обычные имена везде, кроме определённой позиции:

- `unboxed` и `untagged` непосредственно перед `type` (см. `codegen.md`);
- режимы внешних функций `pure`, `sync`, `async`, `effect`, `blocked` и источник
  `global`;
- метаданные внешних функций `arity`, `this`, `catch`, `cancel`, `nullable`,
  `retention`, `optional`, `variadic`, `reviewed` (неизвестные метаданные —
  `FLICK-S011`);
- слова предложений SQL внутри `sql { … }`: `join`, `left_join`, `on`, `group_by`,
  `having`, `order_by`, `limit`, `offset`, `select`, `union`, `union_all`,
  `insert_into`, `values`, `update`, `set`, `delete_from`, `returning` (а также
  ключевые слова `from` и `where`). Внутри блока SQL такое слово завершает
  применение функции, поэтому предложение может продолжаться на следующей строке;
- `table` сразу после `sql` на верхнем уровне и `via` после типа столбца
  `sql table` (см. [SQL](../sql/));
- слова предложений CLI в начале предложения внутри `cli { … }`: `description`,
  `flag`, `arg`, `subcommand`, `handle`.

### Числовые литералы

Используется грамматика JavaScript `NumericLiteral`, поэтому литерал означает
в точности число JS (§9):

- десятичные `0`, `42`, `1.5`, экспоненциальная запись `1e-3`, `2E+10`;
- `0x1F`, `0o17`, `0b1010` (буквы префикса в любом регистре);
- `_` разделяет две цифры: `1_000_000`, `0xFF_FF`. Он не может начинать или
  завершать последовательность цифр, повторяться или стоять вплотную к `.`,
  `e` или префиксу;
- BigInt — целое число с суффиксом `n`: `10n`, `0x1Fn`, без дробной части
  и экспоненты;
- у десятичного числа нет ведущего нуля: `010` (устаревшая восьмеричная запись)
  и `09` — ошибки; пишите `0o10`;
- в отличие от JavaScript, `.5` и `5.` — ошибки: `.` используется для доступа
  к полю. Пишите `0.5` и `5.0`.

Некорректный литерал даёт одну диагностику `FLICK-S006`; он никогда не
разбивается на несколько токенов.

### Строковые литералы

Интерполяции нет.

- `"…"` на одной строке, с управляющими последовательностями `\n \t \r \\ \" \0`
  и `\u{…}` с 1–6 шестнадцатеричными цифрами, обозначающими скалярное значение
  Unicode (без суррогатов, не больше `10FFFF`). За `\0` не может следовать
  цифра. Любая другая управляющая последовательность — `FLICK-S005`.
- Необработанные строки `r"…"`, `r#"…"#`, `r##"…"##`… не содержат управляющих
  последовательностей, могут занимать несколько строк и заканчиваются на `"`,
  за которой следует то же количество `#`.
- Многострочные строки открываются `"""` с последующим переводом строки и
  заканчиваются на следующем `"""`; управляющие последовательности работают
  так же, как в `"…"`. Из значения удаляется первый перевод строки,
  последняя строка удаляется, если она содержит только пробелы перед
  закрывающим `"""`, и убирается общий отступ непустых строк (закрывающая
  строка тоже учитывается). Управляющие последовательности декодируются после
  этого, поэтому `\u{20}` — содержимое, а не отступ.

Переводы строк внутри любого строкового значения — `\n`, в том числе
в исходниках с `\r\n`. Выражения, состоящие только из литералов, вычисляются
во время компиляции по семантике JS (спецификация §8, «Вычисление литеральных
выражений»): `60 * 60 * 24`, `"a" ++ "b"`, `not false`.

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

Пользовательских операторов нет. От самого слабого приоритета к самому сильному:

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
| 9 | префиксные `-`, `not`, `await`, `ignore` | — |
| 10 | применение функции `f x y` | левая |
| 11 | `.field`, `Module.name` | левая |

`-x ** 2` означает `-(x ** 2)`; `-a * b` означает `(-a) * b`; `await f x`
означает `await (f x)`. Побитовые операции — функции `Number.bit_*`.

## Правила отступов

Тела функций, ветви сопоставления и члены `class`, `instance` и `service`
ограничиваются отступами; `in` не используется, а последнее выражение блока
служит его значением (§3). Фигурные скобки ограничивают записи и блоки `task`,
`scope`, `provide`, `sql`, `ui`, `cli`, `codec` и `rec`.

1. Объявления верхнего уровня начинаются в столбце 1. Строка с большим
   отступом продолжает текущее объявление.
2. *Блок* открывается после `=`, `->`, `then`, `else` и `try`:
   - если блок начинается на следующей строке, его столбец — столбец первого
     токена. Строка в этом столбце начинает следующую инструкцию, строка
     с большим отступом продолжает текущую инструкцию, строка с меньшим
     отступом завершает блок;
   - если блок начинается на той же строке, он содержит одну инструкцию;
     она продолжается на строках с отступом больше, чем у строки, на которой
     она началась (и больше, чем у любого внешнего блока в фигурных скобках),
     и заканчивается на любой другой строке.
3. Строка, которая начинается в столбце блока с инфиксного оператора, кроме
   `-`, с `|`, `then`, `else`, `with` или с закрывающего разделителя,
   продолжает инструкцию, а не начинает новую. Благодаря этому конвейеры
   `|>`, ветви `match` и ветки `else` могут выравниваться со своим выражением.
4. Внутри `{ … }` записи, типа записи или блока в фигурных скобках элементы
   выравниваются по первому элементу, который может идти после `{` на той же
   строке; элементы разделяются переводами строк или `;`. `;` разделяет
   инструкции только внутри фигурных скобок.
5. Внутри `( … )`, `[ … ]` и фигурных скобок строк переводы строк не имеют
   значения, но строка, начинающаяся левее столбца внешнего блока, завершает
   скобочную конструкцию (чтобы пропущенная `)` не поглотила последующие объявления).
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

Строка, столбец которой не соответствует ни одному внешнему блоку, или
ожидаемое на следующей строке тело без отступа дают одну диагностику
`FLICK-S009` на этой строке:

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

Правая часть `type` — всегда список вариантов, поэтому
`type Wrapper = Wrap Number` — ADT с одним конструктором; прозрачный
псевдоним, в том числе для типа записи, записывается через `type alias`.
`alias` — ключевое слово только непосредственно после `type`. Поле варианта —
атомарный тип, как в шаблоне конструктора: `Some (List a)`, `Handler (a -> b)`,
а `Circle (Point, Number)` имеет одно поле-кортеж, тогда как
`Circle Point Number` — два поля. Запись `of` перед полями даёт `FLICK-S010`.

`type error E` объявляет ошибку; её единственный конструктор тоже называется
`E`, а ошибка с полями записывается как `type error E = E T …`. Иное имя
конструктора — `FLICK-S007`.
У каждой именованной функции полная сигнатура находится непосредственно над её
`let` (§3), кроме метода экземпляра класса, сигнатура которого берётся из
класса; парсер принимает сигнатуру без определения, а последующие фазы
проверяют соответствие пары.

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

- `a -> b` — чистая функция; `a ->{r; e} b` — вызов среды выполнения,
  требующий сервисы `r` и завершающийся ошибками `e`; `a ->{} b` — вызов
  среды выполнения с обеими пустыми строками, никогда не совпадающий с `->`
  (§2, §4). `-> {` с пробелом — стрелка к типу записи.
- `none` — литерал пустой строки. Пропущенная сторона стрелки тоже пуста:
  `->{; E}` — это `->{none; E}`, `->{R;}` — это `->{R; none}` (см.
  [эффекты](../effects/#пустые-строки-none)). `{Users, Log}` — закрытая строка,
  завершающее `| r` делает её открытой.
- `T^c`, `T^{s}`, `T^(c + d)` уточняют тип захватами, которые он содержит;
  `^` связывает сильнее применения, поэтому `List a^c` — это `List (a^c)`.
- Кинды используют ту же грамматику: `Type`, `Type -> Type`, `ServiceRow`.

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

`if` — выражение, в котором всегда есть `else`. `ignore e` отбрасывает
результат промежуточной инструкции, отличный от `Unit` (§3). `await e`
однократно запускает сохранённый `Eff` в текущем файбере; `task { … }`
создаёт холодное вычисление (§4). В `provide e { … }` и `codec e { … }`
скобка `{` завершает головное выражение.
В `cli "name" { … }` парсер фиксирует только формы предложений; псевдонимы
у `arg`, повторяющиеся имена и типы параметров проверяются на этапе
развёртывания CLI (спецификация §10, «CLI eDSL»). Комментарий `///`
непосредственно над предложением `flag`, `arg` или `subcommand` служит
его текстом справки.

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

## Шаблоны

```text
or_pattern   = pattern { "|" pattern }
pattern      = Con { "." Con } { atom_pattern } | atom_pattern
atom_pattern = "_" | name | Con | ["-"] Number | ["-"] BigInt | String | "true" | "false"
             | "()" | "(" or_pattern ")" | "(" or_pattern ":" type ")"
             | "(" or_pattern "," or_pattern { "," or_pattern } ")"
             | "[" [or_pattern { "," or_pattern }] "]"
             | "{" field ["=" or_pattern] { ";" | line break … } "}"
```

Поле шаблона записи без `= pattern` связывает переменную с именем поля.
Шаблоны с альтернативами и условия следуют правилам Rust (спецификация §8,
«Варианты, guards и or-patterns»): каждая альтернатива связывает одни и те же
имена, имя связывается не больше одного раза в шаблоне или в параметрах
функции (`FLICK-S012`), а условие `when e` относится ко всей ветви.
`|` в начале строки внутри шаблона ветви продолжает шаблон с альтернативами;
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

Универсальная нотация `do`, операторы связывания `let*` и JSX `<…>` дают
по одной диагностике `FLICK-S010`, указывающей замену во Flick (прямые вызовы
с `let`, `task`/`await`, `ui { … }`), а не общую ошибку разбора.

## Восстановление после ошибок

Каждая лексическая проблема — одна диагностика и один токен. После
синтаксической ошибки парсер пропускает остаток инструкции и возобновляет
работу со следующей инструкции или объявления, поэтому сообщается обо всех
независимых ошибках, а одна ошибка не вызывает каскада других в той же инструкции.

## Форматирование

`flick fmt` изменяет только пробельные символы, поэтому AST не меняется
(форматтер повторно разбирает свой вывод и в противном случае отказывает
с `FLICK-P003`). Он лишь убирает пустые строки стрелки, которые обозначают тот
же тип: `->{none; E}` становится `->{; E}`, а `->{none; none}` — `->{}`;
`->{R; none}` и `Eff none none a` остаются в исходном виде.

- отступ становится равен двум пробелам на уровень с сохранением отношения
  столбца каждой строки (меньше, равен, больше) к расположенным выше блокам;
- один пробел между токенами, без пробелов внутри `( )`, `[ ]`, вокруг `.`,
  `^`, а также в `->{r; e}` и фигурных скобках строк `{Users, Log}`;
  один пробел внутри остальных фигурных скобок;
- унарный `-` примыкает к своему операнду; `forall a b.` сохраняет пробел после `.`;
- пробелы в конце строк удаляются, последовательности пустых строк сокращаются
  до одной, файл заканчивается одним переводом строки; комментарии остаются
  на своих местах, строка только с комментарием получает такой же отступ,
  как код в её столбце;
- многострочные строки и блочные комментарии копируются без изменений;
- строка длиннее 100 символов разбивается по самой внешней структуре: тело
  после `=` или `->` переносится на отдельную строку, в цепочке `if` каждая
  ветвь получает свою строку, затем переносы ставятся перед операторами
  с самым слабым приоритетом (`|>` выравнивается по началу своего блока),
  после стрелок типов (с сохранением `->{r; e}` целиком), перед аргументами
  применения и, наконец, по одному элементу на строку внутри самых широких
  скобок (единственное выражение в круглых скобках остаётся вплотную к ним).
  Каждое продолжение имеет отступ на два пробела больше. Перенос сохраняется,
  только если у файла остаётся тот же AST; единственный токен длиннее
  ограничения остаётся как есть.

`flick fmt --check` завершается с `1` и сообщает `FLICK-P002` для каждого
файла, который изменился бы; `flick fmt --stdin --stdin-filepath <file>`
читает исходник из stdin и записывает результат в stdout для редакторов.
