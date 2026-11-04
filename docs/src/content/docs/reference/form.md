---
title: "Forms"
---

A form is a record type the user fills in as text. `deriving Form` gives
the type a text input per field. Validation is the type's own: the form is
read as the record and checked with its schema, refined types included. Each
problem lands under the input of the field its path names.

```flick
module App.Signup exposing (..)

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

/// Asks the server whether the name is taken; typing again interrupts it.
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

## What a type derives

A record type (a `type alias` of a record) that derives `Form` derives
`Schema` too. Each field is typed from text: a `String`, a `Float`, a
`Bool`, a refined type over one of them, or an `Option` of one, which is
`None` when its input is empty. Any other field is `std/D010`, which names
the field. The plugin of `std` writes both with `Form.make`, `Form.spec` and
`Form.unsafe_field`.

- `Signup.form : Form Signup` is what `Form.draft` takes.
- `Signup.fields` is a record with a `Form.Field` per field. A misspelt
  field, `Signup.fields.nmae`, is an ordinary type error.

## Filling in

`Form.draft Signup.form` holds the text and the problem of each field in
atoms. It is defined with a top-level `let`, like any atom.

`Form.input draft field label on_input` is the field's input:

- a visible label;
- the input, bound to the field's text and named by the label
  (`aria-label`);
- below it, the problem, an alert announced when it appears. The input
  points to it with `aria-describedby` and is `aria-invalid` while there is
  one.

`on_input` runs after each edit. `Form.text`, `Form.problem` and `Form.id`
give the atoms and the input's id, for a layout of your own.

## Sending

`Form.validate draft` reads each text as its field's type and checks the
record with the schema, reporting every issue rather than the first. It
gives `Some` record when all fields are valid. Otherwise it gives `None`,
sets each field's problem to its first issue, and focuses the first
invalid input in the page.

## Checks of your own

A check the schema cannot make, such as whether a name is taken on the
server, runs on input with `Ui.Event.latest`. A new keystroke interrupts
the check still running and waits for it to stop before the next one
starts, so a slow answer about old text never overwrites a newer one. The
check gives its verdict with `Form.report draft field message`.
