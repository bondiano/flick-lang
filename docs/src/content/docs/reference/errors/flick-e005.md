---
title: "FLICK-E005"
---

The payload of an error is data that outlives the code that raised it: it may
not hold a cell (`Ref`), a computation (`Eff`), a layer, a runtime function,
an `Unknown` that could hide one, or a value of a region such as a scoped
`File s`, even when a handler inside the scope catches the error. Pass a
path, an id or a copy of the data instead of the live value.

```flick
// was: type error ReadFailed = ReadFailed (Ref String)
type error ReadFailed = ReadFailed String
```
