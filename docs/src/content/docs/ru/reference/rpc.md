---
title: "Типизированные RPC"
---

Опишите процедуру один раз и используйте это описание по обе стороны
соединения. `Rpc` связывает имя с кодеками запроса, результата
и типизированных ошибок; группа несёт точный тип записи обработчиков.
Пропущенный или лишний обработчик обнаруживается при проверке типов,
а не при первом запросе.

Общее описание согласует код клиента и сервера, но не версии развёртывания:
независимо обновляемые участники всё равно должны договориться о контракте
передачи, формате сообщений и транспорте. Выбирайте
[типизированные HTTP API](../http-api/), когда публичный контракт — это
эндпоинты, статусы и OpenAPI; RPC — когда интерфейсом служат именованные
операции и типизированные вызовы.

Следующий модуль обслуживает и вызывает собственный HTTP API.
Сохраните первый блок Flick как `RpcDemo.fl`;
[схемы и кодеки](../schema/) объясняют значения на границе передачи.

```flick
module RpcDemo exposing (main)

import Stream.Core exposing (Stream)

type alias User = { id : Number; name : String }
  deriving Schema

type error NotFound = NotFound { id : Number }
  deriving Schema

get_user : Rpc.Rpc "get_user" (Number ->{r; NotFound} User)
let get_user =
  Rpc.make (Record.label : Record.Label "get_user")
    |> Rpc.with_payload (Codec.json Schema.number)
    |> Rpc.with_success (Codec.json (schema User))
    |> Rpc.with_errors (Rpc.errors |> Rpc.error_case (Codec.json (schema NotFound)))

count_to : Rpc.Rpc "count_to" (Number ->{r;} Stream none none Number)
let count_to =
  Rpc.make (Record.label : Record.Label "count_to")
    |> Rpc.with_payload (Codec.json Schema.number)
    |> Rpc.with_stream (Codec.json Schema.number)

type alias Handlers r =
  {
    get_user : Number ->{r; NotFound} User;
    count_to : Number ->{r;} Stream none none Number;
  }

users : Rpc.Group r (Handlers r)
let users = Rpc.group |> Rpc.add get_user |> Rpc.add count_to

handlers : Handlers none
let handlers =
  {
    get_user = fun id ->
      if id == 1 then { id = 1; name = "Ada" } else from_result (Err (NotFound { id = id }));
    count_to = fun n -> Stream.from_list (List.range 1 (n + 1));
  }

main : Unit ->{; AppError} Unit
let main () =
  try
    scope s {
      let server = Rpc.listen Rpc.defaults users handlers
      let url = "http://127.0.0.1:" ++ show (Rpc.port server) ++ "/rpc"
      let user = Rpc.call url get_user 1
      Platform.stdout_line user.name
      let numbers = Stream.run_collect (Rpc.call_stream url count_to 3)
      Platform.stdout_line (show numbers)
    }
  with
  | Rpc.ServeError message -> App.fail ("serve: " ++ message)
  | Rpc.ClientError message -> App.fail ("client: " ++ message)
  | NotFound e -> App.fail ("not found: " ++ show e.id)
```

```sh
flick build RpcDemo.fl --out-dir dist/rpc-demo
node dist/rpc-demo/flick-main.js
```

Успешный запуск выводит `Ada` и `[1, 2, 3]`. `NotFound` сохраняется
как типизированная ошибка предметной области при передаче по сети;
`ClientError` означает ошибку транспорта или декодирования.
Сервер принадлежит окружающей области: выход из неё освобождает
и слушающий сокет.

## Процедуры

Вызов `Rpc.make label` создаёт процедуру с именем `l`, обработчик которой
принимает и возвращает `Unit`. Сигнатура процедуры задаёт тип обработчика:
строка сервисов `r` остаётся открытой, а ошибки `e` — это те, что
объявлены через `with_errors`. До этого ошибок нет, поэтому обработчик
с ошибкой без варианта — ошибка типов.

| Комбинатор | |
| - | - |
| `with_payload codec` | запрос, кодируемый как JSON |
| `with_success codec` | ответ вызова |
| `with_stream codec` | ответ как поток: каждый элемент кодируется и отправляется по мере появления; ошибка обработчика завершает поток ошибкой |
| `with_errors (errors \|> error_case codec \|> …)` | задаёт типизированные ошибки обработчика и способ отправки каждой |
| `with_middleware hook` | перехватчик перед обработчиком при каждом вызове, см. ниже |
| `uninterruptible` | сервер доводит обработчик до конца, даже когда клиент исчез или прервал вызов: для записи, которая не должна оборваться на середине |

## Формат сообщений

`Rpc.Serialization` задаёт кодирование сообщений независимо от транспорта.
Для потокового HTTP начните с NDJSON. WebSocket уже сохраняет границы
сообщений, а TCP — нет, поэтому ему нужен формат с кадрированием.
Сериализаторы и транспорты — отдельные значения: неиспользуемые бинарный
формат и WebSocket могут быть исключены из браузерного бандла.

| Значение | Формат |
| - | - |
| `ndjson` | JSON, одно сообщение в строке; элементы потока приходят по мере отправки. Значение по умолчанию |
| `json` | один документ JSON; поверх HTTP все сообщения вызова приходят вместе, поэтому элементы потока приходят после его завершения |
| `json_rpc` | документы JSON-RPC 2.0 для собеседников, говорящих на нём |
| `ndjson_rpc` | JSON-RPC 2.0, одно сообщение в строке |
| `binary` | кадры Effect по схемам: меньше JSON, но оба конца должны быть Effect, и это добавляет объёма бандлу |

## Обслуживание

| Функция | Транспорт |
| - | - |
| `listen options group handlers` | POST-запрос на вызов поверх HTTP по пути `options.path` |
| `listen_websocket …` | каждому клиенту — WebSocket по пути `path`, сообщения которого переносят его вызовы (`ws://127.0.0.1:8080/rpc`); каждый формат потоковый |
| `listen_tcp …` | каждому клиенту — TCP-соединение с `host` и `port` (`tcp://127.0.0.1:8080`). Только Node. Байтовому потоку подходит только формат с разбивкой на сообщения (`ndjson`, `ndjson_rpc`, `binary`); иначе — `ServeError`, потому что TCP может разрезать документ между чтениями |
| `serve_stdio serialization group handlers` | отвечает на вызовы из сообщений stdin в stdout, пока stdin не закончится, как языковые серверы в редакторах; больше ничего не может писать в stdout в это время. Только Node |

Функции `listen` работают на Node и получают `Server s` в `Scope s`.
Закрытие области прекращает приём новых вызовов и закрывает текущие
с их финализаторами. Обработчики используют сервисы, доступные
в месте запуска сервера. `Rpc.port server` возвращает фактический порт,
включая выбранный системой при `port = 0`. `Rpc.defaults` задаёт
`127.0.0.1`, порт `0`, `/rpc` и `ndjson`;
`ServeError` сообщает о неудачном запуске.

`serve_stdio` работает до окончания stdin и требует сервис `Stdio`:
предоставьте `Stdio.node`. Оставьте stdout только для сообщений протокола;
диагностику пишите в stderr.

### Рядом с другими маршрутами

`Rpc.handler serialization group handlers` отдаёт группу как HTTP-обработчик
(POST-запрос на вызов), который `Http.Router` монтирует как обычный маршрут;
`handler_websocket` — то же для `GET`-маршрута с WebSocket. Обработчик
держит регион `s` области, в которой создан, как и маршруты и
маршрутизатор над ним: возврат любого из них за пределы этой области —
`FLICK-G001`.

```flick
serve_both : Http.Server.ServeOptions -> Handlers none ->{
  Http.Platform.HttpPlatform,
  Http.Etag.Generator;
  Http.ServerError.ServeError
}
  Unit
let serve_both options handlers =
  scope s {
    rpc : (ServerRequest ->{} ServerResponse)^{s}
    let rpc = Rpc.handler Rpc.ndjson users handlers
    let app =
      Http.Router.router Http.Router.config
        [
          Http.Router.route (Some Http.Method.POST) "/rpc" (fun request -> rpc request),
          Http.Router.route (Some Http.Method.GET) "/hello/:name" hello,
        ]
    let server = Http.Server.serve options app
    Platform.stdout_line (Http.Server.format_address (Http.Server.address server))
  }
```

Серверы Node живут за отдельной точкой входа: браузерная программа,
которая только вызывает процедуры, не втягивает HTTP-сервер и
сокет-сервер Node.

## Вызовы

| Функция | |
| - | - |
| `call url procedure payload` | ответ вызова: успех, объявленная ошибка как типизированный сбой или `ClientError` |
| `call_with options url procedure payload` | как `call`, с форматом сообщений и транспортом клиента |
| `call_stream url procedure payload` | элементы потоковой процедуры по мере поступления |
| `call_stream_with options url procedure payload` | как `call_stream`, с форматом сообщений и транспортом клиента |
| `connect options url` | одно соединение в области, общее для вызовов до её закрытия: WebSocket остаётся открытым, HTTP переиспользует соединения |
| `call_on client procedure payload`, `call_stream_on …` | вызов через подключённый клиент |
| `spawn serialization command args` | запускает дочерний процесс и соединяется с ним через его stdin и stdout, на которые отвечает его `serve_stdio`; его stderr — stderr программы, закрытие области останавливает потомка. Только Node |

`CallOptions` называет формат сообщений и транспорт: `Rpc.http`
(POST-запрос на вызов, по умолчанию), `Rpc.websocket` (сообщения вызовов
через глобальный WebSocket — в бандл он попадает, только если программа
его называет) или `Rpc.tcp` (TCP-соединение, открываемое при первом
вызове, поэтому отказ соединения — `ClientError` именно этого вызова;
только Node). Начинайте с `Rpc.call_defaults`. Адрес `http://`
обслуживается `listen`, `ws://` — `listen_websocket`, `tcp://` —
`listen_tcp`.

## Перехватчики и заголовки

`Rpc.with_middleware hook` выполняет перехватчик перед обработчиком
при каждом вызове, после уже добавленных перехватчиков, с сервисами
вызова `listen`. Перехватчик видит декодированный запрос и заголовки
его HTTP-запроса или WebSocket с именами в нижнем регистре.
Типизированный сбой возвращается как сбой обработчика, поэтому он должен
входить в объявленные ошибки процедуры. Здесь достаточно обычной функции,
без сервисов-перехватчиков Effect, построенных на классах.

Вызов `Rpc.with_headers headers task` выполняет `task` с вызовами,
несущими и эти заголовки, которые читают перехватчики: например,
`authorization`.

Следующий перехватчик проверяет лишь наличие заголовка, а не подлинность
токена. Настоящая аутентификация должна проверить учётные данные,
прежде чем разрешать выполнение обработчика.

```flick
type error Unauthorized = Unauthorized { reason : String }
  deriving Schema

require_authorization_header : Rpc.Call Unit ->{; Unauthorized} Unit
let require_authorization_header call =
  match Http.Headers.get "authorization" call.headers with
  | Some _ -> ()
  | None -> from_result (Err (Unauthorized { reason = "no authorization" }))
```

Используйте `Rpc.uninterruptible`, только если обработчик должен закончить
работу после отмены клиентом. Это может задержать остановку сервера,
но не делает запись атомарной и не защищает от завершения процесса.
Транзакционность остаётся обязанностью самой операции.

## Тесты обработчиков

Вызов `Rpc.call_local group handlers procedure payload` вызывает
процедуру в памяти: клиент и сервер соединены без транспорта;
`call_stream_local` делает то же для потока. Кодеки при этом не
выполняются, поэтому их тестам всё ещё нужен настоящий сервер, а
перехватчики `with_middleware` не видят заголовков.

Вызовы в памяти подходят для проверки логики и объявленных ошибок.
Совместимость кодеков, заголовки и кадрирование потоков проверяйте
настоящим обменом через транспорт. `Rpc` оборачивает нестабильные модули
Effect; закрепляйте согласованные версии компилятора и среды выполнения.
