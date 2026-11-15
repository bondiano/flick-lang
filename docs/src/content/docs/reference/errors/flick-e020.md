---
title: "FLICK-E020"
---

A selected build environment must supply a root handler for every gated
effect its reachable program still requires. Browser profiles have no root
handler for database or server-only services such as `Db`, `Database` or
`Server`, so a reachable use is `unhandled effect … on js/browser` at that
requirement. The same program can build on `js/node` when that environment
lists the handler. Unused unsupported code does not fail a reachable-only
check; a library's public exports are checked under the export policy.
Missing toolchains stay `FLICK-P015` / `FLICK-P029`, not this code.

```flick
# flick.toml: [build] environment = "js/browser"
module App.Main exposing (run)

service Db =
  query : String ->{} String

run : String ->{Db} String
let run key = Db.query key
```
