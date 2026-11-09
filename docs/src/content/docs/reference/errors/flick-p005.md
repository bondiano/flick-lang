---
title: "FLICK-P005"
---

`flick.toml` is not valid: a TOML syntax error, a key the schema does not know,
or a value of the wrong type, for example a `kind` other than `app` or `lib`.

```toml
schema-version = 1

[project]
kind = "app"
entry = "App.Main"
```
