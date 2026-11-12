---
title: "FLICK-R016"
---

A module of a program marks a declaration with `@lang`, or names the runtime
by a word in `@ts import`. Lang items are what the compiler relies on, such
as `Option` for `?` or `flat_map` for `do`, and only the built-in modules
declare them and name the runtime so. Remove the attribute: a program's own
types and functions need none, and the form of a type of a program names a
module by its specifier, as `@ts import "dayjs" "Dayjs"`.

```flick
module App.Ids exposing (UserId)

type UserId = UserId Int
```
