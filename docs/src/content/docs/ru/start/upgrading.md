---
title: Переход на 0.2
description: Что изменилось после Flick 0.1.0 и как обновить проект.
source_hash: d77bbc44765e494d
---

Flick 0.2 разрабатывается в `main` и ещё не выпущен. До 1.0 минорный релиз
может ломать программы, и 0.2 ломает: `std` стал пакетом, который проект
скачивает, блоки `cli`, `codec` и `server fn` и derive для `Schema`, `Url`,
`Form`, `HttpApi` и `Cli.Value` переехали из компилятора в плагин `std`, а
библиотеки компилируются из исходников. На этой странице перечислены все
изменения после 0.1.0 и сказано, как обновить проект. Она пополняется по мере
того, как появляются новые изменения.

## Как обновиться

Соберите компилятор из checkout ветки `main` (см.
[Экосистема](../../ecosystem/)), укажите проекту `std` из того же checkout
(см. [следующий раздел](#std-приходит-из-пакета)) и выполните в проекте:

```sh
flick install
flick check --fix
```

`flick check --fix` применяет безопасные исправления: переписывает
`schema T` и добавляет импорты, которые нужны блокам плагинов, а заодно и
остальные исправления, которые применяет всегда. То, что осталось, он
сообщает, а разделы ниже говорят, что нужно для каждого изменения.

| Изменение | `flick check --fix` | Что вы увидите |
| - | - | - |
| [`std` приходит из пакета](#std-приходит-из-пакета) | нет: выполните `flick install` | `FLICK-P034` |
| [`schema T` стало `T.schema`](#schema-t-стало-tschema) | да | `FLICK-T001` |
| [Блокам плагинов нужен импорт](#блокам-плагинов-нужен-импорт) | да | `FLICK-P037` |
| [Коды диагностик под `std/`](#коды-диагностик-под-std) | нет | новые коды |
| [`deriving HttpApi` строже](#deriving-httpapi-строже) | нет | `std/D011`, `FLICK-C001` |
| [Библиотеки компилируются из исходников](#библиотеки-компилируются-из-исходников) | нет | `FLICK-R003` |
| [Экземпляр принадлежит одному пакету](#экземпляр-принадлежит-одному-пакету) | нет | `FLICK-C006` |
| [Плагины получают только разрешённое](#плагины-получают-только-разрешённое) | нет | `FLICK-P038`, `FLICK-P039` |
| [`LogLevel` стал перечислением](#loglevel-стал-перечислением) | нет | `FLICK-T001` |
| [Плагины следуют интерфейсу компилятора](#плагины-следуют-интерфейсу-компилятора) | нет | — |
| [`CallSite` вместо записей с `_at`](#callsite-вместо-записей-с-_at) | нет | `FLICK-T001` |
| [Замыкание над ошибкой это показывает](#замыкание-над-ошибкой-это-показывает) | нет | `FLICK-G001` |
| [Компилятор собирается Rust 1.95](#компилятор-собирается-rust-195) | нет | — |

`flick check --fix page.md` переписывает блоки кода `flick` страницы
Markdown. Там каждый блок проверяется сам по себе, а его импорты берутся на
веру, поэтому о пропущенном импорте плагина он не сообщает: добавьте импорт
вручную.

## `std` приходит из пакета

В 0.1.0 `std` был встроен в компилятор. Теперь в компиляторе только модули
ядра, а `std` — пакет. Проект, который не называет `std` в `[dependencies]`,
зависит от тега `std-v<версия>` релиза своего компилятора: `flick install`
скачивает его и закрепляет в `flick.lock`. Коммитьте `flick.lock`. Пока
`flick install` не выполнен, любая команда, которая проверяет модули,
останавливается:

```text
error[FLICK-P034]: `std` from `git+https://github.com/bondiano/flick-lang?tag=std-v0.1.0&subdir=packages/std` is not installed: run `flick install`
```

Пока 0.2 не выпущен, компилятор, собранный из `main`, по-прежнему сообщает
версию 0.1.0 и поэтому берёт `std` тега 0.1.0, в котором нет функций,
которые теперь пишут плагины компилятора. Вместо него назовите `std` из
того же checkout:

```toml
[dependencies]
std = { path = "../flick-lang/packages/std" }
```

или задайте этот каталог в `FLICK_STD_PATH`.

## `schema T` стало `T.schema`

Свидетель типа — его компаньон `T.schema`, а `schema` больше не ключевое
слово. В 0.1:

```flick fail
type alias Profile = { email : String }
  deriving Schema

profile_schema : Schema Profile
let profile_schema = schema Profile
```

Теперь `schema Profile` — вызов неизвестного `schema`, `FLICK-T001`, и его
исправление пишет `Profile.schema`:

```flick
type alias Profile = { email : String }
  deriving Schema

profile_schema : Schema Profile
let profile_schema = Profile.schema
```

`flick check --fix` переписывает это сам.

## Блокам плагинов нужен импорт

Блоки `cli`, `codec`, `server fn` и `sql table` теперь плагины `std`.
Файл, в котором есть такой блок, импортирует его голову:

| Блок | Импорт |
| - | - |
| `cli "name" { … }` | `import Cli exposing (plugin cli)` |
| `codec s { … }` | `import Codec exposing (plugin codec)` |
| `server fn f x = …`, `server f` | `import Server exposing (plugin server)` |
| `sql table Name "table" { … }` | `import Sql exposing (plugin sql)` |

В 0.1 блоку `codec` импорт был не нужен:

```flick ignore
type alias Profile = { email : String; nickname : Option String }
  deriving Schema

profile_json : PureCodec Json Profile
let profile_json =
  codec (schema Profile) {
    email = field "email_address" String.json;
    nickname = field "nickname" (option_as_null String.json);
  }
```

Теперь блок без импорта — `FLICK-P037`, и его исправление добавляет импорт:

```flick
import Codec exposing (plugin codec)

type alias Profile = { email : String; nickname : Option String }
  deriving Schema

profile_json : PureCodec Json Profile
let profile_json =
  codec Profile.schema {
    email = field "email_address" String.json;
    nickname = field "nickname" (option_as_null String.json);
  }
```

`flick check --fix` за один запуск добавляет импорт и переписывает
`schema Profile`. `cli`, `codec`, `server` и `fn` больше не ключевые слова;
`plugin` — ключевое слово только среди открытых имён импорта.

## Коды диагностик под `std/`

Плагин сообщает об ошибках под своими кодами, которые показываются под его
пакетом. Правила переехавших блоков и derive сохранили смысл под новыми
кодами:

| 0.1 | Теперь | О чём сообщает |
| - | - | - |
| `FLICK-L001` … `FLICK-L007` | `std/L001` … `std/L007` | правила блока `cli` |
| `FLICK-S007` в блоке `cli` | `std/L009` | неправильно записанная конструкция |
| `FLICK-C013` у `deriving Cli.Value` | `std/L010` | тип, для которого `Cli.Value` не выводится |
| `FLICK-D001`, `FLICK-D009`, `FLICK-D010` у `deriving` | `std/D001`, `std/D009`, `std/D010` | тип, для которого не выводится `Schema`, `Url` или `Form`, — на классе в `deriving` |
| `FLICK-D002` … `FLICK-D005`, `FLICK-D007` | `std/D002` … `std/D005`, `std/D007` | правила блока `codec` |
| несовпадение типов в блоке `codec` | `std/D012` | поле, кодек которого декодирует не тот тип, что у поля записи |
| `FLICK-D011` | `std/D011` | запись, которую `deriving HttpApi` не может прочитать как эндпоинты |
| `FLICK-E010` | `std/E010` | форма `server fn`; `server name` для значения, которое не `server fn`, остаётся `FLICK-E010` |
| `FLICK-Q001`, `FLICK-Q010` в `sql table` | `std/Q001`, `std/Q010` | столбец указан дважды, тип SQL или атрибут, которого нет в диалекте |
| `FLICK-Q011` | `std/Q011` | столбец другого типа, чем у его типа SQL, без `via` |
| `FLICK-E012`, `FLICK-L008` | — | больше не используются: плагин `std` есть в каждой сборке |

`flick explain std/D001` объясняет новый код, а `flick explain FLICK-L001`
говорит, что старый код больше не используется, и называет `std/L001`.
Обновите скрипты и фильтры CI, которые ищут старые коды; сам код менять не
нужно.

## `deriving HttpApi` строже

Плагин `std` выводит те же эндпоинты, маршруты и документы, с тремя
отличиями:

- Значение, названное по типу в его модуле, `credentials` для
  `Credentials`, служит кодеком части запроса или успешного ответа, только
  если это `PureCodec Json` этого типа. Иначе берётся JSON-кодек схемы типа,
  а без неё часть — `std/D011`.
- Проверка группы эндпоинтов `Authed` и `MaybeAuthed` получает только
  заголовки запроса, `HttpApi.Request Unit Unit Credentials Unit`.
- Ошибка без экземпляра `Problem` и путь, параметры `:name` которого не поля
  его части `path`, — `FLICK-C001` (раньше были `FLICK-D011`).

Это нужно поправить вручную.

## Библиотеки компилируются из исходников

В 0.1.0 npm-пакет отдавал свои модули Flick как скомпилированные
интерфейсы:

```json
{ "name": "@acme/text", "flick": { "iface-dir": "iface" } }
```

Теперь проект компилирует каждую библиотеку Flick из исходников, а
`iface-dir` не читается: импорт модулей такого пакета — `FLICK-R003`,
неизвестный модуль. Вместо этого пакет поставляет исходники:

```json
{ "name": "@acme/text", "flick": { "source-dir": "src" } }
```

или становится библиотекой Flick, которую проект называет в
`[dependencies]` по пути, git-тегу или архиву (см.
[Зависимости Flick](../../reference/project/#зависимости-flick)).
`flick pack` для библиотеки теперь пишет архив её исходников, и ему нужен
`[package]` с `name` и `version` (без них — `FLICK-P032`); приложение
по-прежнему упаковывается в npm-пакет со своими раннерами. Это изменение
для автора библиотеки.

## Экземпляр принадлежит одному пакету

Для проверки типов библиотека — другой пакет, и зависимость по пути тоже.
Объявить экземпляр может только пакет класса или пакет типа в голове
экземпляра. В 0.1.0 проект мог объявить экземпляр класса своей библиотеки
по пути для типа этой же библиотеки; теперь это `FLICK-C006`,
экземпляр-сирота. Перенесите экземпляр в библиотеку, которой принадлежит класс или тип, или
оберните тип в собственный тип проекта:

```flick
# type Id = Id String
# class Label a =
#   label : a -> String
#
type LocalId = LocalId Id

instance Label LocalId =
  label : LocalId -> String
  let label wrapped =
    match wrapped with
    | LocalId (Id raw) -> raw
```

## Плагины получают только разрешённое

Плагины появились в 0.2. Плагин зависимости читает файлы, переменные
окружения и хосты по HTTP только в той мере, в какой проект разрешает это в
`flick.toml`:

```toml
[plugins.pairs]
fs-read = ["schema/"]
net = ["api.example.com"]
env = ["API_TOKEN"]
```

Всё остальное — `FLICK-P038` со строкой, которую нужно добавить. Ответы по
HTTP сохраняются в `.flick/plugin-snapshots/`, и проект их коммитит. С
`--offline`, который теперь есть у каждой команды, или при заданной `CI`
запрос, на который нет снимка, — `FLICK-P039`. Плагин `std` ничего вовне не
достаёт, поэтому проекту, который использует только `std`, разрешать ничего
не нужно. См. [Доступ вовне](../../reference/plugins/#доступ-вовне).

## `LogLevel` стал перечислением

Перечисления появились в 0.2: `untagged type Method = Get = "GET" | Post =
"POST"` — тип, значения которого — их литералы (см.
[Генерация кода](../../reference/codegen/)). `LogLevel` — такое
перечисление, его конструкторы объявлены от наименее к наиболее строгому:

| 0.1.0 | 0.2 |
| - | - |
| `LogLevel.all`, `trace`, `debug`, `info`, `warn`, `error`, `fatal` | `LogLevel.All`, `Trace`, `Debug`, `Info`, `Warn`, `Error`, `Fatal` |
| `LogLevel.none_` | `LogLevel.Off`, литерал которого — `"None"` из Effect |
| `LogLevel.values` | `LogLevel.all`, по возрастанию строгости |
| `LogLevel.name level` | `LogLevel.value level` |
| `LogLevel.get_ordinal a < LogLevel.get_ordinal b` | `compare a b`, выведенный `Ord` |

`show` уровня даёт имя конструктора: `Off` там, где 0.1.0 печатал `None`.
Каждое старое имя даёт `FLICK-T001`; замените его вручную.

## Плагины следуют интерфейсу компилятора

Вывод для перечисления получает литералы его конструкторов, а `check`
плагина — базовый тип перечисления; вывод знает, какие конструкторы
принадлежат GADT (`refined`, `constrained` у варианта), а плагин во время
раскрытия спрашивает, перечисление ли тип (`literal-of`). Интерфейс остаётся
`0.1.0` до Flick 0.2 и меняется без новой версии: пересоберите компонент тем
`flick-plugin`, что и компилятор, в котором он работает.

## `CallSite` вместо записей с `_at`

Класс ядра `CallSite` даёт функции место её использования (см.
[Место вызова](../../reference/classes/#место-вызова)). `Atom.set`,
`update` и `modify` берут его сами, поэтому `Atom.set_at`, `update_at` и
`modify_at` удалены (`FLICK-T001`): вызывайте `Atom.set atom value` или
дайте своей функции `CallSite =>`, чтобы она сообщала место вызывающего.
Ключ serializable-атома по-прежнему `Module.name`, теперь это его
`CallSite.definition`.

## Замыкание над ошибкой это показывает

Значение `Errors e`, `Exit e a` или `Failure e` удерживает то, что
удерживают ошибки `e`, а переменная строки ошибок `e` удерживается так же,
как переменная типа. В 0.1.0 такое значение не удерживало ничего, и
замыкание над пойманной ошибкой могло скрыть регион из её данных.
Каррированная стадия сохраняет переменную, поэтому
`keep : forall e. Errors e -> (Unit -> Unit)` по-прежнему проходит проверку
и читается как удерживающее `e`. Голый тип, который не является стадией,
например поле, теперь отвергает замыкание:

```flick fail
keep_field : forall e. Errors e -> { run : Unit -> Unit }
let keep_field x = { run = fun () -> ignore x }
```

```text
error[FLICK-G001]: this function uses `x`, which holds `e`, but the expected type `Unit -> Unit` holds no captures
```

Назовите удерживаемое строкой захватов, как для переменной типа (см.
[Захваты](../../reference/regions/#захваты)):

```flick
keep_field : forall c e. (Errors e)^c -> { run : (Unit -> Unit)^c }
let keep_field x = { run = fun () -> ignore x }
```

`flick check --fix` это не переписывает: строка захватов появляется сразу
в `forall`, в параметре и в поле.

## Компилятор собирается Rust 1.95

Checkout ветки `main` собирается Rust 1.95 (для 0.1.0 хватало 1.94); его
`rust-toolchain.toml` называет эту версию для rustup. Cargo-фичи
`server-stdlib` больше нет: плагин `std` есть в каждой сборке.

## Запланировано в 0.2

Эти изменения запланированы и ещё не сделаны; когда изменение появится, у
него будет раздел на этой странице:

- блоки `sql` как плагин `std`, как сейчас `cli` и `codec`;
- позже — изменения в ядре эффектов.
