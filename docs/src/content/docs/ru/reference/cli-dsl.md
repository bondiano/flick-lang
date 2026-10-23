---
title: "Приложения командной строки"
---

`Cli.Command r e` описывает командную строку: флаги, позиционные аргументы,
подкоманды и обработчик, которому нужны сервисы `r` и который завершается
с ошибками `e` (спецификация §10 «CLI eDSL»). При создании такого значения
ничего не разбирается и не запускается; `Cli.run` разбирает аргументы
с помощью CLI из Effect и только затем запускает обработчик.

```flick
greet : Cli.Command none none
let greet =
  cli "greet" {
    description "Prints a greeting"
    /// Print in capitals
    flag loud : Bool
    /// How many times
    flag times "n" : Number = 1
    /// Who to greet
    arg name : String
    handle fun { loud; times; name } ->
      let text = "Hello, " ++ name
      let line = if loud then String.to_upper text else text
      Platform.stdout_line (String.join "\n" (List.repeat times line))
  }
```

## Предложения

| Предложение | Значение |
| - | - |
| `description e` | текст справки команды, `e : String`; не более одного |
| `flag name "alias"… : T [= default]` | опция `--name`; за ней следуют дополнительные имена |
| `arg name : T [= default]` | следующий позиционный аргумент |
| `subcommand e` | вложенная команда, `e : Cli.Command r e` |
| `handle f` | обработчик, `f : Options ->{r; e} Unit`; не более одного |

Комментарий `///` непосредственно над `flag`, `arg` или `subcommand` —
его текст справки. Обработчик получает точную запись с полем на каждый
`flag` и `arg`, названным по предложению; в командной строке `dry_run`
становится `--dry-run`, однобуквенный псевдоним — `-n`, более длинный — `--name`.

| Тип | Поведение | Комбинатор |
| - | - | - |
| `Bool` | переключатель, `false` при отсутствии | `Cli.switch` |
| `T` | обязателен | `Cli.required` |
| `T = d` | `d` при отсутствии | `Cli.default d` |
| `Option T` | `None` при отсутствии; значение по умолчанию не допускается | `Cli.optional` |
| `List T` | повторяемый флаг; для `arg` — остаток командной строки, только на последней позиции | `Cli.many` |
| `NonEmpty T` | как `List T`, но не менее одного значения | `Cli.some` |

Значение типа `T` разбирается классом `Cli.Value T`: экземпляры класса
есть для `String`, `Bool`, `Number` (без NaN и пустого текста), `BigInt`,
`Redacted String`, `DateTime.DateTime` (ISO 8601) и `(String, String)`
(`key=value`, разделение по первому `=`). `deriving Cli.Value` создаёт
экземпляр класса для типа из конструкторов без полей — выбор среди их имён
в kebab-case, `GreenLight` становится `green-light`, — а также для
уточнённого типа, для которого разбирается базовый тип и проверяется предикат.

## Комбинаторы

Блок — синтаксический сахар: он разворачивается в конвейер тех же функций,
которые программы могут использовать напрямую, и обе формы компилируются
в одно и то же промежуточное представление (IR).

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

`Cli.subcommand command help` добавляет подкоманду (`help` описывает её
в справке родительской команды); `Cli.group` завершает команду без
собственного обработчика.

Следующие возможности доступны только через комбинаторы:

| Комбинатор | Значение |
| - | - |
| `Cli.at_least n`, `Cli.at_most n`, `Cli.between m n` | `List` с указанным количеством значений |
| `Cli.from_config config` | одно значение, читаемое из `Config.Config` при отсутствии |
| `Cli.from_prompt prompt` | одно значение, запрашиваемое через `Cli.Prompt.Prompt` при отсутствии |
| `Cli.hidden`, `Cli.metavar name` | применяются к кратности: исключение из справки или имя значения в ней |
| `Cli.optional_value text` | флаг без значения получает `text` |
| `Cli.file b`, `Cli.directory b`, `Cli.path b` | значение задаёт путь, который должен существовать при `b` |
| `Cli.file_text`, `Cli.file_parse format` | `Cli.Value` разбирает текст файла либо JSON-представление его данных INI, JSON, TOML или YAML |
| `Cli.short_description`, `Cli.alias`, `Cli.unlisted`, `Cli.example` | представление команды в справке |
| `Cli.action name help act` | переключатель `--name`, запускающий `act` вместо любого обработчика |

`Cli.help_doc path command` — запись `Cli.HelpDoc`, которую `--help`
показывает после перехода по подкомандам `path`; `Cli.help` — её текст,
а `Cli.completions shell command` — скрипт автодополнения. Интерактивные
вопросы — это `Cli.Prompt`: значения вроде `Cli.Prompt.text`,
`Cli.Prompt.select` или `Cli.Prompt.custom`, задаваемые через `Cli.Prompt.run`
на сервисе `Cli.Prompt.Tty` (`Cli.Prompt.node` — терминал процесса;
имитация — запись `Layer.succeed Tty`), по одному нажатию клавиши за раз.
Запись опций расширяется через `Record.Lacks` и `Record.UnionDisjoint`,
поэтому дважды указанное поле или образец обработчика, называющий поле,
которого нет у команды, — ошибка типов.

## Запуск

`Cli.run command args` разбирает `args` с помощью CLI из Effect и только
затем запускает обработчик указанной ими команды в текущем файбере
с сервисами Node.js, необходимыми CLI из Effect.
`Cli.run_version version command args` также задаёт текст `--version`
(`Cli.run` использует `0.0.0`), а `Cli.run_with settings` дополнительно
выбирает встроенные флаги и `Cli.Formatter` для справки и версии.

```flick
main : Unit ->{; AppError} Unit
let main () =
  try Cli.run greet (Platform.args ()) with
  | Cli.CliError message -> App.fail message
```

`--help`, `--version` и `--completions bash|zsh|fish` печатают в stdout
и не запускают обработчик. Неверный ввод — неизвестный флаг, пропущенный
аргумент, значение, отклонённое `Cli.Value`, — печатает справку команды
и завершается ошибкой `Cli.CliError` с текстом Effect; обработчик не
запускается, а `App.fail` превращает её в код завершения 1. Флаги команды
с подкомандами идут перед именем подкоманды и видны только обработчику
этой команды.

Команда сохраняет захваты своего обработчика: команда, обработчик которой
удерживает ресурс из `scope`, выполняется внутри этой области и не может
её покинуть (`FLICK-G001`).

## Правила

| Правило | Код |
| - | - |
| дважды использованы поле, длинное имя или псевдоним либо использовано одно из `--help`, `-h`, `--version`, `--completions` | `FLICK-L001` |
| `= default` у `Option` | `FLICK-L002` |
| `arg` со множеством значений перед последним `arg` | `FLICK-L003` |
| `description` или `handle` дважды | `FLICK-L004` |
| команда без `handle` и без подкоманд | `FLICK-L005` |
| `arg` рядом с подкомандами | `FLICK-L006` |
| дополнительные имена у `arg` | `FLICK-L007` |

Тип без `Cli.Value` — `FLICK-C001`, значение по умолчанию неверного
типа — `FLICK-T003`.
