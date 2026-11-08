---
title: "FLICK-P045"
---

The plugins of two packages say they load files of the same extension:

```toml
# protos/flick.toml
[package]
name = "protos"
version = "0.1.0"
plugin = "protos.wasm"
load = ["proto"]
```

and another dependency's `[package]` also has `load = ["proto"]`. A file
`App/User.proto` must be the module of one plugin, so the project uses the
plugin of the dependency named first and reports this.

Keep one of the packages, or have one of them load another extension.
