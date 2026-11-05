---
title: "Приложения командной строки"
source_hash: 5f84e91fcb647ccb
---

`Cli.Command r e` описывает флаги, позиционные аргументы, подкоманды
и обработчик команды. Обработчику нужны сервисы `r`; он может завершиться
с ошибками `e` (см. [CLI eDSL: флаги, аргументы и подкоманды](../../spec/08-type-programming-and-edsl/#cli-edsl-флаги-аргументы-и-подкоманды)).
Создание такого значения не разбирает аргументы и не запускает обработчик.
Это делает `Cli.run`: сначала разбирает аргументы с помощью CLI из Effect,
затем вызывает обработчик.

Блок `cli "name" { … }` — это плагин `cli` пакета `std` (см.
[плагины](../plugins/)): файл, в котором он написан, импортирует его.

```flick
import Cli exposing (plugin cli)

greet : Cli.Command none none
let greet =
  cli "greet" {
    description "Prints a greeting"
    /// Print in capitals
    flag loud : Bool
    /// How many times
    flag times "n" : Int = 1
    /// Who to greet
    arg name : String
    handle fun { loud; times; name } ->
      let text = "Hello, " ++ name
      let line = if loud then String.to_upper text else text
      Platform.stdout_line (String.join "\n" (List.repeat times line))
  }
```

<div id="предложения"></div>

## Конструкции блока

| Конструкция | Назначение |
| - | - |
| `description e` | текст справки команды, `e : String`; не более одного |
| `flag name "alias"… : T [= default]` | опция `--name`; строки после имени задают её псевдонимы |
| `arg name : T [= default]` | следующий позиционный аргумент |
| `subcommand e` | вложенная команда, `e : Cli.Command r e` |
| `handle f` | обработчик, `f : Options ->{r; e} Unit`; не более одного |

Комментарий `///` непосредственно над `flag`, `arg` или `subcommand`
задаёт текст справки для этой конструкции. Обработчик получает точную запись:
каждому `flag` и `arg` соответствует поле с тем же именем. В командной строке
`dry_run` превращается в `--dry-run`, однобуквенный псевдоним — в `-n`,
а более длинный — в `--name`.

Внутри `cli` языковой сервер дополняет все пять конструкций блока,
показывая синтаксис, справку и шаблон вставки. При наведении на слово
конструкции видны синтаксис и справка, а на имя объявленного флага или
аргумента — записанный тип и документация `///`. Конструкции и имена
параметров подсвечиваются по контексту; в типах, значениях по умолчанию и
теле обработчика работает обычное дополнение Flick.

| Тип | Поведение | Комбинатор |
| - | - | - |
| `Bool` | переключатель, `false` при отсутствии | `Cli.switch` |
| `T` | обязателен | `Cli.required` |
| `T = d` | `d` при отсутствии | `Cli.default d` |
| `Option T` | `None` при отсутствии; значение по умолчанию не допускается | `Cli.optional` |
| `List T` | повторяемый флаг; для `arg` — остаток командной строки, только на последней позиции | `Cli.many` |
| `NonEmpty T` | как `List T`, но не менее одного значения | `Cli.some` |

Класс `Cli.Value T` определяет, как разбирать значение типа `T`. Готовые
экземпляры класса есть для `String`, `Bool`, `Float` (без NaN и пустого текста),
`BigInt`, `Redacted String`, `DateTime.DateTime` (ISO 8601) и `(String, String)`
(`key=value`, разделение по первому `=`). Для типа из конструкторов без полей
`deriving Cli.Value` создаёт разбор по именам конструкторов в kebab-case:
например, `GreenLight` становится `green-light`, а у перечисления — по его
литералам. Для уточнённого типа сначала
разбирается значение базового типа, затем проверяется предикат.

## Комбинаторы

Блок — синтаксический сахар над библиотечными функциями: он означает вызовы
тех же функций, которые можно написать и напрямую. Каждая конструкция
применяется к описанию перед ней, а `Cli.handled spec handler` завершает
команду с обработчиком: описание идёт первым, поэтому обработчик проверяется
по записи опций, которую дают конструкции. Конвейер, написанный вручную,
читается так же:

```flick
greet : Cli.Command none none
let greet =
  Cli.command "greet"
    |> Cli.description "Prints a greeting"
    |> Cli.flag (Record.label : Record.Label "loud") [] "Print in capitals" Cli.switch
    |> Cli.flag (Record.label : Record.Label "times") ["n"] "How many times" (Cli.default 1)
    |> Cli.arg (Record.label : Record.Label "name") "Who to greet" Cli.required
    |> Cli.handle (fun { loud; times; name } -> Platform.stdout_line name)
```

`Cli.subcommand command help` добавляет подкоманду; `help` описывает её
в справке родительской команды. `Cli.group` завершает описание команды,
у которой нет собственного обработчика.

Следующие возможности доступны только через комбинаторы:

| Комбинатор | Значение |
| - | - |
| `Cli.at_least n`, `Cli.at_most n`, `Cli.between m n` | `List` с указанным количеством значений |
| `Cli.from_config config` | читает одно значение из `Config.Config`, если оно не передано |
| `Cli.from_prompt prompt` | запрашивает одно значение через `Cli.Prompt.Prompt`, если оно не передано |
| `Cli.hidden`, `Cli.metavar name` | применяются к описанию кратности: скрывают его из справки или задают имя значения в ней |
| `Cli.optional_value text` | флаг без значения получает `text` |
| `Cli.file b`, `Cli.directory b`, `Cli.path b` | путь должен существовать, если `b` истинно |
| `Cli.file_text`, `Cli.file_parse format` | `Cli.Value` разбирает текст файла либо данные INI, JSON, TOML или YAML, приведённые к JSON |
| `Cli.short_description`, `Cli.alias`, `Cli.unlisted`, `Cli.example` | представление команды в справке |
| `Cli.action name help act` | переключатель `--name`, запускающий `act` вместо любого обработчика |

`Cli.help_doc path command` возвращает запись `Cli.HelpDoc` со справкой,
которую `--help` показывает для подкоманды по пути `path`. Флаги команд
выше по пути лежат в `inheritedFlags` и показываются в разделе
`INHERITED FLAGS`. `Cli.help`
возвращает текст этой справки, а `Cli.completions shell command` — скрипт
автодополнения.

Для интерактивных вопросов служит `Cli.Prompt`. Значения `Cli.Prompt.text`,
`Cli.Prompt.select` и `Cli.Prompt.custom` запускаются через `Cli.Prompt.run`
с сервисом `Cli.Prompt.Tty`, который обрабатывает нажатия клавиш по одному.
`Cli.Prompt.node` использует терминал процесса; для имитации можно передать
запись через `Layer.succeed Tty`.

Запись опций расширяется с помощью `Record.Lacks` и `Record.UnionDisjoint`.
Повторное поле или образец обработчика, в котором указано отсутствующее
у команды поле, приводит к ошибке типов.

## Запуск

`Cli.run command args` разбирает `args` с помощью CLI из Effect,
затем запускает обработчик выбранной команды в текущем файбере.
Для этого используются сервисы Node.js, необходимые CLI из Effect.
`Cli.run_version version command args` также задаёт текст для `--version`;
`Cli.run` использует `0.0.0`. С помощью `Cli.run_with settings` можно
дополнительно выбрать встроенные флаги и `Cli.Formatter` для справки и версии.

```flick
# import App.Greet exposing (greet)
#
main : Unit ->{; AppError} Unit
let main () =
  try Cli.run greet (Platform.args ()) with
  | Cli.CliError message -> App.fail message
```

Флаги `--help`, `--version` и `--completions bash|zsh|fish|powershell`
выводят результат в stdout, не запуская обработчик. `--wizard` спрашивает
флаги и аргументы команды по одному, затем запускает её; `--log-level debug`
(любой уровень от `all` до `none`) задаёт минимальный уровень лога программы.
`--trace` запускает обработчик как спан с именем команды и отправляет его
спаны, логи и метрики по OTLP на `OTEL_EXPORTER_OTLP_ENDPOINT`
(по умолчанию `http://localhost:4318`) как сервис `OTEL_SERVICE_NAME`
(по умолчанию имя команды); без флага экспортёр не загружается.
`Cli.run_with` с `builtIns` без `Cli.TraceFlag` убирает флаг. При неверном вводе — неизвестном флаге,
пропущенном аргументе или значении, отклонённом `Cli.Value`, — выводится
справка команды, а вызов завершается ошибкой `Cli.CliError` с текстом Effect.
Обработчик при этом не запускается. В примере `App.fail` превращает эту
ошибку в код завершения 1.

Флаги команды с подкомандами указываются перед именем подкоманды и видны
только обработчику той команды, которой принадлежат.

Команда сохраняет захваты обработчика. Если он удерживает ресурс из `scope`,
команду можно выполнить только внутри этой области; покинуть её команда
не может (`FLICK-G001`).

## Правила

| Правило | Код |
| - | - |
| повторно указаны поле, длинное имя или псевдоним либо использовано зарезервированное имя `--help`, `-h`, `--version`, `--completions` | `std/L001` |
| значение `= default` у `Option` | `std/L002` |
| `arg` со множеством значений стоит не последним среди `arg` | `std/L003` |
| `description` или `handle` указаны дважды | `std/L004` |
| у команды нет ни `handle`, ни подкоманд | `std/L005` |
| у команды есть и `arg`, и подкоманды | `std/L006` |
| у `arg` указаны дополнительные имена | `std/L007` |
| конструкция не из пяти либо у блока нет имени | `std/L009` |
| блок в файле, который не импортирует `plugin cli` | `FLICK-P037`, исправляет `flick check --fix` |

Коды `std/…` — коды плагина; `flick explain std/L001` объясняет каждый. Тип без экземпляра `Cli.Value` приводит к `FLICK-C001`, значение по умолчанию
неверного типа — к `FLICK-T003`.
