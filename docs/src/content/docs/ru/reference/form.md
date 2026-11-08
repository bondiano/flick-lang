---
title: "Формы"
source_hash: f070f09c7ee16517
---

Форма — тип-запись, которую пользователь заполняет текстом. `deriving Form`
даёт типу поле ввода на каждое поле записи. Проверка — собственная проверка
типа: форма читается как запись и проверяется его схемой, включая
уточнённые типы. Каждая ошибка попадает под поле ввода того поля, которое
называет её путь.

```flick
module App.Signup exposing (..)
# import App.Api

refined type Name = String where Checks.non_empty
  deriving Eq, Show, Schema

type alias Signup = { name : Name; age : Float; nick : Option String }
  deriving Schema, Form

signup : Form.Draft Signup
let signup = Form.draft Signup.form

send : Action.Action Ui.ClickSnapshot
let send =
  Action.exhaust (fun _ -> task {
    match Form.validate signup with
    | None -> ()
    | Some s -> App.Api.register s
  })

/// Спрашивает сервер, занято ли имя; новый ввод прерывает проверку.
check_name : Ui.Handler Ui.InputSnapshot
let check_name =
  Ui.Event.latest (fun input -> task {
    let message = if App.Api.taken input.value then Some "taken" else None
    Form.report signup Signup.fields.name message
  })

page : Ui
let page = ui {
  fragment
    [
      Form.input signup Signup.fields.name "Name" (Some check_name),
      Form.input signup Signup.fields.age "Age" None,
      Form.input signup Signup.fields.nick "Nickname" None,
      button { disabled = false; on_click = send } [text "Send"],
    ]
}
```

## Что выводит тип

Для типа-записи (`type alias` записи) с `deriving Form` также выводится `Schema`.
Каждое поле читается из текста: `String`, `Float`, `Bool`, уточнённый тип
над одним из них или `Option` одного из них — `None`, когда поле ввода
пусто. Другое поле — `std/D010` с путём поля. Плагин `std` пишет оба значения
через `Form.make`, `Form.spec` и `Form.unsafe_field`.

- `Signup.form : Form Signup` принимает `Form.draft`.
- `Signup.fields` — запись с `Form.Field` на каждое поле. Опечатка в имени
  поля, `Signup.fields.nmae`, — обычная ошибка типов.

## Заполнение

`Form.draft Signup.form` держит текст и ошибку каждого поля в атомах.
Определяйте его top-level `let`, как любой атом.

`Form.input draft field label on_input` — поле ввода поля записи:

- видимая подпись;
- поле ввода, привязанное к тексту поля и названное подписью
  (`aria-label`);
- под ним — сообщение об ошибке с ролью `alert`, о появлении которого сообщают вспомогательные технологии. Поле ввода
  ссылается на неё через `aria-describedby` и помечено `aria-invalid`, пока
  она есть.

`on_input` выполняется после каждой правки. `Form.text`, `Form.problem` и
`Form.id` дают атомы и id поля ввода для собственной вёрстки.

## Отправка

`Form.validate draft` читает каждый текст как тип его поля и проверяет
запись схемой, сообщая все ошибки, а не первую. Если все поля верны, возвращает
запись в `Some`. Иначе возвращает `None`, сохраняет первое сообщение об ошибке
для каждого поля и переводит фокус на первое неверное поле ввода на странице.

## Свои проверки

Проверка, которую схема сделать не может, например занято ли имя на
сервере, выполняется на ввод через `Ui.Event.latest`. Новое нажатие
прерывает ещё идущую проверку и ждёт её остановки до начала следующей, так
что медленный ответ о старом тексте никогда не перезапишет более новый.
Проверка сообщает вердикт через `Form.report draft field message`.
