---
title: "FLICK-P065"
---

`[build] environment = "<backend>/<host>"` names a host that neither the
selected backend nor any loaded `[host]` dependency package provides, two
loaded host packages declare the same host for the same backend, or the chosen
host package's `[host] requires` condition is not met by the backend's active
tags.

Check the host spelling against the backend's built-in hosts and your
dependencies, or enable the backend options required by the host:

```toml
[build]
environment = "js/node"
```
