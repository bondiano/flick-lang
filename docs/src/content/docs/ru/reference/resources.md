---
title: "Ресурсы и их освобождение"
source_hash: e2b30d4d6138e8be
---

`scope` владеет тем, что получено в нём, и освобождает это, когда его тело
завершается ([регионы](../regions/)). Значение класса `Close` освобождается
через `Resource.own`: область закрывает его один раз и передаёт закрытию
исход своего тела, а никакой другой код закрыть его не может.
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
прочитать не может, но по которому видно, завершилось ли тело успешно
(`Exit.is_success`) или было прервано (`Exit.has_interrupts`). Экземпляр,
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
среди них и закрытия, которые зарегистрировал `own`, начиная с последнего.
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
Flick, и в Go (`tests/run/runtime/owned_close.fl`).

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
