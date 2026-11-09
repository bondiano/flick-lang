---
title: "FLICK-P044"
---

An import exposes a head of the plugin of `std`, which every module has
without an import, from the plugin of another package:

```flick
module App.Main exposing (..)

import Queries exposing (plugin sql)
```

The heads of `std`'s plugin, `cli`, `server`, `codec` and `sql`, start a
block in every module, so a block of one of them always belongs to `std`.
Another plugin gives its blocks other heads.

Remove the head from the import, or rename it in the plugin that gives it.
