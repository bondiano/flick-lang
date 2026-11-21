---
title: "FLICK-P059"
source_hash: "09b1166894e4018e"
---

`--target <name>` или `[build] target = "<name>"` указали бэкенд, который не
является ни официальной целью (`js`, `go`), ни коротким именем или идентификатором
компонента в `[build.backend]`.

Укажите `js` или `go`, либо закрепите сторонний компонент бэкенда в
`[build.backend]` файла `flick.toml`.

```toml
[build]
target = "go"
```
