---
title: "FLICK-R005"
---

The constructors of a type are exported only when the module lists them with
`Type(..)`; exporting `Type` alone keeps them private, and an opaque type never
exports them. Import the type alone and build values with the module's
functions, or export the constructors from the module that defines the type.

```flick ignore
module App.Shapes exposing (Shape(..), area)

import App.Shapes exposing (Shape(..))
```
