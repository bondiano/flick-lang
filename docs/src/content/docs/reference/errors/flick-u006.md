---
title: "FLICK-U006"
---

React may render a view again at any time, and a registry computes a
derived atom whenever an atom it read changes. An atom, an action or a view
made there is a new one on every render: a state starts over, subscribers
lose it, an action forgets the task it runs, and a view made anew is a new
component, whose subtree React mounts again. A render is any function
whose services are `Atom.Read` alone: the render of `Ui.view`, the
computation of `Atom.derived`, or a function with such a signature; the
task of a handler in it runs on an event and may make what it needs. A
function that makes one, in the module or in a module it imports, makes
one in a render too, whether the render calls it or passes it on.

Make each once with a top-level `let`, and one per key with `Atom.family`:
`Atom.at family key` gives the same atom for an equal key, which needs
`Hash`, so it holds no function. Pass what differs between elements to an
action as data with `Action.with`.

```flick ignore
drafts : Atom.Family Float (Atom.Writable String)
let drafts = Atom.family (fun _ -> Atom.make "")

note_view : Ui.View Float
let note_view = Ui.view (fun id -> ui { Ui.field { bind = Atom.at drafts id; placeholder = ""; on_input = None } })
```
