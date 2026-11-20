---
title: "Ресурсы и их освобождение"
source_hash: a3f41fc015eb88d0
---

`scope` владеет тем, что получено в нём, и освобождает это, когда его тело
завершается ([регионы](../regions/)). Значение класса `Close` освобождается
через `Resource.own`: область закрывает его один раз и передаёт закрытию
исход своего тела, а никакой другой код закрыть его не может.
`Resource.make` получает значение и регистрирует освобождение, которое
видит исход, а `Scope.defer` регистрирует очистку, которая его видит.
`Resource.acquire_release` и `Scope.add_finalizer` остаются
низкоуровневым способом зарегистрировать освобождение.

## Владение значением

```flick
# import Resource exposing (Closing)
#
class Close a =
  close : forall t. Closing t -> a ->{Async} Unit
  close_exit : forall t e b. Closing t -> Exit e b -> a ->{Async} Unit
  let close_exit token _ value = close token value
```

```flick
# import Resource exposing (Close)
#
own : forall s a. Close a => a^{} ->{Async, Scope s} a^{s}
# let own = Resource.own
```

`own value` делает ближайший `scope` (или область региона `s`) владельцем
`value` и возвращает значение с квалификатором этого региона, поэтому оно
не может покинуть область. Когда область завершается, `close_exit`
получает исход тела: `Exit` с ошибками и значением, которые закрытие
прочитать не может, но по которому видно, завершилось ли тело успешно или
со сбоем (`Exit.is_success`) и было ли оно прервано (`Exit.has_interrupts`). Экземпляр,
определивший только `close`, получает `close_exit` по умолчанию, который
исход не смотрит.

```flick
import Resource exposing (Close(..), own)

say : String ->{} Unit
let say line = Host.raw_stdout_line line

type Conn = Conn String

instance Close Conn =
  let close _ (Conn name) = say ("close " ++ name)
  let close_exit _ exit (Conn name) =
    say ("close " ++ name ++ (if Exit.is_success exit then ": succeeded" else ": failed"))

main : Unit ->{Async} Unit
let main () = scope {
  let _a = own (Conn "a")
  let _b = own (Conn "b")
  scope {
    let _c = own (Conn "c")
    say "inner body"
  }
  say "outer body"
}
```

```text
inner body
close c: succeeded
outer body
close b: succeeded
close a: succeeded
```

## Порядок и исход

Область закрывается по фазам ([среда выполнения](../runtime/)): она
прерывает свои дочерние задачи и ждёт их, затем запускает финализаторы,
начиная с последнего зарегистрированного: среди них закрытия, которые
регистрирует `own`, и освобождения из `Resource.make` и `Scope.defer`.
Поэтому:

- каждое значение во владении закрывается ровно один раз — после всего,
  что получено в области позже него, и после завершения её дочерних задач;
- закрытие выполняется в маскированном режиме: прерывание области через
  `timeout` или закрытие родителя его не обрывает, а закрытие видит
  прерывание в исходе;
- если закрытие завершается сбоем, например через `Effect.die`, его дефект
  добавляется к причине завершения области, которая сохраняет причину
  тела; остальные закрытия всё равно выполняются.

То же выполняется и в адаптере Effect, и в собственной среде выполнения
Flick, и в Go (`tests/run/runtime/owned_close.fl`,
`tests/run/runtime/defer_make.fl`).

## Получение и отложенная очистка

```flick
make :
  forall r s e a c.
  (Detached r, Fiber.Outlives c s, Fiber.Provided r s) =>
  (Eff r e a)^c -> (forall e2 b. a -> Exit e2 b ->{r} Unit)^c ->{Scope s | r; e} a^{s}
# let make = Resource.make

defer :
  forall r s c.
  (Detached r, Fiber.Outlives c s, Fiber.Provided r s) =>
  (forall e b. Exit e b ->{r} Unit)^c ->{Scope s | r} Unit
# let defer cleanup = Scope.defer cleanup
```

`Resource.make acquire release` выполняет `acquire` и делает ближайший
`scope` (или `scope` региона `s`) владельцем значения: когда область
завершается, `release` выполняется один раз со значением и исходом тела.
Значение удерживает регион `s`, поэтому покинуть область не может. Между
получением и этой регистрацией прерывание не вклинивается, а неуспешное
получение ничего не регистрирует: для значения, которое не было получено,
освобождение не выполняется, а значения, полученные до него, всё равно
освобождаются. `Scope.defer cleanup` так же регистрирует `cleanup`, только
без значения. Оба участвуют в порядке, описанном выше, и их работа видит
исход так же, как `close_exit`.

Освобождение может использовать регионы и сервисы, которые переживают
область; более короткий регион или сервис, предоставленный внутри
области, — это `FLICK-G003`, а операция `control`, на которую к тому
времени не ответит ни один обработчик, — `FLICK-E017`. Эти ограничения
компилятор проверяет при каждом вызове; функция, обобщённая по ним,
указывает их явно, как это делает `make`.

```flick
type error Refused = Refused String

say : String ->{} Unit
let say line = Host.raw_stdout_line line

ended : Exit e b -> String
let ended exit = if Exit.is_success exit then "succeeded" else "failed"

type Conn = Conn String

disconnect : Conn -> Exit e b ->{} Unit
let disconnect (Conn name) exit = say ("disconnect " ++ name ++ ": " ++ ended exit)

connect : String ->{Async, Scope s} Conn^{s}
let connect name =
  Resource.make
    (task {
      say ("connect " ++ name)
      Conn name
    })
    disconnect

refuse : String ->{Async, Scope s; Refused} Conn^{s}
let refuse name = Resource.make (task { Fail.fail (Refused name) }) disconnect

session : Unit ->{Async; Refused} Unit
let session () = scope {
  let _a = connect "a"
  Scope.defer (fun exit -> say ("log flushed: " ++ ended exit))
  let _b = connect "b"
  let _c = refuse "c"
  say "not reached"
}

main : Unit ->{Async} Unit
let main () =
  scope {
    let _a = connect "a"
    Scope.defer (fun exit -> say ("deferred: " ++ ended exit))
    let _b = connect "b"
    say "body"
  }
  try session () with
  | Refused name -> say ("refused " ++ name)
```

```text
connect a
connect b
body
disconnect b: succeeded
deferred: succeeded
disconnect a: succeeded
connect a
connect b
disconnect b: failed
log flushed: failed
disconnect a: failed
refused c
```

Замыкание или задача может удерживать полученное значение после его
области, пока не использует его там; вызов, который использует его после
области, например операция сервиса над ним, — это `FLICK-G002`
([регионы](../regions/)). Проверки: `tests/check/pass/regions/made.fl`,
`tests/check/fail/regions/made.fl`.

## Закрывает только владелец

Закрытие принимает `Closing t`, который создаёт только `own` для закрытия,
которое он запускает: у программы нет значения, с которым можно вызвать
`close`, поэтому досрочно закрыть значение во владении она не может
(`FLICK-T001`). Токен называет регион, который живым держит только это
закрытие: сохранить токен в ячейке или замыкании, чтобы позже закрыть
другое значение, нельзя (`FLICK-T008`, `FLICK-G002`).

`own` принимает значение, которое не удерживает регион (`a^{}`): значение,
которым область уже владеет, или данные, которые его держат, закрылись бы
дважды или в области, которая заканчивается раньше той, что их использует.

```flick fail G001
import Resource exposing (Close(..), own)

type Conn = Conn (Unit ->{} Unit)

instance Close Conn =
  let close _ _ = ()

twice : Unit ->{Async} Unit
let twice () = scope {
  let conn = own (Conn (fun () -> ()))
  ignore (own conn)
}
```

`FLICK-G001`: значение удерживает регион `s`, а ожидаемый тип не допускает
захватов.

## Сервисы приобретения

Приобретение в `Resource.manual` и `Effect.resource_auto` выполняется
с сервисами вызова в собственном регионе `t`. Оно предполагает
`Fiber.Provided r t`: финализатор, который оно там регистрирует, например
освобождение из `Resource.acquire_release`, может использовать эти
сервисы, поскольку их поставщики переживают `t` так же, как регион ресурса.

## Пулы, транзакции и серверы

Эти ресурсы стандартной библиотеки поступают так, как закончилась работа,
которая их держала, как это делает `close_exit`:

| Ресурс | Успех | Типизированная ошибка | Дефект | Прерывание |
| - | - | - | - | - |
| элемент `Pool.get` или `Pool.use` | возвращается в пул | возвращается в пул | освобождается, пул получает новый | освобождается, пул получает новый |
| `Database.transaction` | фиксируется | откатывается | откатывается | её соединение закрывается, и сервер её откатывает |
| `Http.Server.serve`, `HttpApi.listen` | дожидается запросов | дожидается запросов | дожидается запросов | дожидается запросов |

Дефект или прерывание могут оставить элемент пула, например соединение,
сломанным посреди работы, поэтому пул больше его не выдаёт;
`Pool.invalidate` по-прежнему убирает элемент вручную. Транзакция, у
которой потерян ответ на `COMMIT`, завершается ошибкой с кодом
`"commit_unknown"` ([sql](../sql/)). Как бы ни закончилась область сервера,
он перестаёт принимать соединения и ждёт выполняющиеся запросы и их
финализаторы, не дольше времени корректной остановки. Проверки:
`tests/run/stdlib/effect/pool_outcomes.fl`,
`tests/run/stdlib/http/drain_outcomes.fl` и, на PostgreSQL,
`tests/postgres/outcomes.fl`.

## Очистка и эффекты control

Финализатор области, освобождение из `Resource.acquire_release` и
закрытие значения во владении выполняются при закрытии области, когда её
обработчики уже могли отпустить тело, поэтому операций `control` и `abort`
они не выполняют (`Detached`, `FLICK-E017`). Очистка `Effect.ensuring`,
`Effect.on_exit`, `Effect.on_interrupt` и освобождение в `Resource.bracket`
выполняются в файбере тела сразу после него и могут их выполнять:

- если тело завершилось значением, типизированной ошибкой или дефектом,
  очистка выполняется как обычный код, и на её операции отвечают (если
  дефект сочетается с `abort`, дефект сохраняется и не перехватывается
  обработчиком `abort`);
- если тело прервано или через него раскручивается `abort` (или арм
  `control`, отпустивший тело), операция `control` обработчика вокруг
  очистки обрывается: её арм не выполняется и очистка на ней
  останавливается, а прерывание или раскрутка продолжаются как были.
  Обработчик, который очистка установила сама, по-прежнему отвечает.

```flick
effect Emit =
  control emit : String ->{} Unit

effect Stop =
  abort stop : forall b. String ->{} b

say : String ->{} Unit
let say line = Host.raw_stdout_line line

collect : forall r e. (Unit ->{Emit | r; e} Unit) ->{r; e} List String
let collect body =
  try body () with
  | Emit.emit x -> [x] ++ resume ()
  | return () -> []

stopping : forall r e. (Unit ->{Stop | r; e} Unit) ->{r; e} String
let stopping body =
  try
    body ()
    "no stop"
  with
  | Stop.stop why -> "stopped: " ++ why

cleanup : Eff {Emit} none Unit
let cleanup = task {
  say "cleanup begins"
  Emit.emit "cleanup"
  say "cleanup ends"
}

main : Unit ->{} Unit
let main () =
  say (show (collect (fun () -> Effect.ensuring cleanup (task { say "body" }))))
  say (show (collect (fun () -> say (stopping (fun () -> Effect.ensuring cleanup (task { Stop.stop "why" }))))))
```

В Go и в собственной среде выполнения Flick программа печатает следующее;
адаптер Effect армы `control` не выполняет (`FLICK-P012`):

```text
body
cleanup begins
cleanup ends
["cleanup"]
cleanup begins
stopped: why
[]
```
