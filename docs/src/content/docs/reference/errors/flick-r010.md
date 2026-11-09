---
title: "FLICK-R010"
---

A name refers to more than one entity: two imports expose it by name, two
`exposing (..)` imports both provide a name that is used, or two imports share
one qualifier. A re-export of the same entity is not ambiguous. Qualify the
name or import it by name from one module.

```flick ignore
import App.Users as Users exposing (..)
import App.Orders as Orders exposing (..)

let first = Users.find
```
