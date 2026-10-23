# User interfaces

`Ui r e` (spec §10) describes a user interface whose event handlers need
the services `r` and may fail with `e`. Building one is pure: it runs no
handler, reads no state and touches no DOM. `Ui.run` renders it with React
19 (`react-dom/client`) and runs each event as a task.

```flick
type alias SaveButtonProps c =
  {
    label : String
    disabled : Bool
    on_save : (Unit ->{Users; SaveError} Unit)^c
  }

save_button : SaveButtonProps c -> (Ui {Users} {SaveError})^c
let save_button props =
  ui {
    button
      {
        disabled = props.disabled
        on_click = Ui.Event.exhaust (fun _ ->
          task { props.on_save () }
        )
      }
      [text props.label]
  }
```

`c` is a capture row (see [regions](regions.md#captures)). Like any new type
variable of a signature it is quantified over the whole signature, so
`forall c.` can be left out ([types](types.md)).

The two `^c` are what let `save_button` accept any callback. A function type
without a qualifier holds no regions. So with `on_save : Unit ->{Users;
SaveError} Unit`, a callback that uses a `File s` from an enclosing `scope`
is rejected where it is passed (`FLICK-G001`), even when the `Ui` never
leaves the scope. With `^c` on the field the callback may hold anything, and
`^c` on the result keeps those captures in the `Ui`, so a `Ui` that holds a
region still cannot leave its scope. A qualifier on the result alone does not
help, since the field would still allow no captures. A top-level signature is
checked as written, so these qualifiers are never inferred. A component whose
callbacks never hold scoped resources can drop both of them.

`ui { … }` is a block in which the names `Ui` exports need no `Ui.`: a local
binding still wins, then a name of `Ui`, then the module's own names. It
adds nothing else, so `ui { button … [text …] }` and `Ui.button … [Ui.text
…]` compile to the same IR.

## Elements and props

Props are exact records: a missing field, an unknown one or a wrong type is
an error. A key is metadata, given with `Ui.keyed`, never a prop.

| Function | Element | Children |
| - | - | - |
| `text s` | text, through React's text channel: markup stays text | none |
| `fragment`, `optional`, `empty` | no element | `List`, `Option`, none |
| `div`, `span` `{ class_name }` | `div`, `span` | `List (Ui r e)` |
| `paragraph`, `heading`, `list`, `item` | `p`, `h1`, `ul`, `li` | `List (Ui r e)` |
| `button { disabled; on_click }` | `button type="button"` | `List (Ui r e)` |
| `input { value; placeholder; on_input }` | `input`; `value` is the initial text | none |
| `link { href : SafeUrl }` | `a` | `List (Ui r e)` |
| `html (s : SanitizedHtml)` | `div` with the HTML | none |
| `select (SelectProps a r e c)` | `ul role="listbox"`; choosing an item runs `on_change` with it | items through `render_item` |
| `panel { header; body; footer }` | `section` with `header`, `main`, `footer` | named places |
| `tabs { selected } (NonEmpty (Tab r e))` | a tab list and the selected panel | `Tab`s from `tab label ui` |
| `component c props` | a foreign React component | its props |

`text` is `forall r e. String -> Ui r e`, so it sits among the children of
any element. `SafeUrl` comes only from `Ui.safe_url`, which accepts relative
URLs and the schemes `http`, `https` and `mailto` and refuses control
characters; `SanitizedHtml` only from `Ui.trusted_html`, the program's own
word. A `String` is neither. A foreign component is a `Ui.Component p` bound
with `foreign pure`; React renders it, Flick never calls it.

## Handlers and policies

A handler field has the opaque type `Ui.Handler snapshot r e`, which only a
policy of `Ui.Event` makes, so no handler is fire-and-forget:

| Policy | An event that comes while earlier tasks run |
| - | - |
| `exhaust f` | is ignored |
| `latest f` | interrupts the running task, waits until it stopped, then runs; one event waits at most, the newest |
| `queue capacity f` | waits, `capacity` events at most; beyond that it is rejected |
| `concurrent limit f` | runs, `limit` tasks at most; beyond that it is rejected |

`capacity` and `limit` are positive integers: a constant that is not one is
`FLICK-U001`, a computed one is checked when the handler is made. A
rejected event is an infrastructure defect for the supervisor, never an
error of `e`. `f` gets an inert snapshot taken inside the DOM callback —
`ClickSnapshot` (`button`, `alt`, `ctrl`, `shift`, `meta`), `InputSnapshot`
(`value`), or the item of a `select`. `Ui.Event.prevent_default decide h`
calls the pure `decide` on the snapshot before the callback returns and
prevents the default action when it says so; otherwise the default action
happens.

Handlers keep their captures: a button whose handler holds a file of a
scope cannot leave that scope (`FLICK-G001`). Render is pure, so `Ref.get`
in a view is a runtime call where none is allowed (`FLICK-E001`).

## Running a root

```flick
run :
  forall c r e m.
  Container -> (Failure e ->{r; none} Unit)^c -> Ref m -> (m -> (Ui r e)^c)^c ->{r; none} Never
```

`Ui.run container supervisor model view` renders `view` of the contents of
`model`, and again after each handler task ends; handlers change the model
with `Ref.set`. Each task runs with the services of the `run` call. A
failure of a task, and each rejected event, goes to `supervisor`; an
interruption by a policy or by closing does not. A failure of the
supervisor itself ends `run` as a defect.

`run` lasts until the fiber running it is interrupted, usually by the end
of the scope that spawned it. Closing has two phases: React cleanup stops
taking events at once and unmounts the tree, removing its listeners; then
the handler tasks are interrupted and joined, supervisors included, and
only then `run` ends — so the scope releases its resources after the tasks
have stopped. `Ui.container id` finds the element to render into.

The adapter is `@flick-lang/runtime/ui`; `react` and `react-dom` 19 are
peer dependencies of the runtime, installed by the application.
