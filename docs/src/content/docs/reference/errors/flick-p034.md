---
title: "FLICK-P034"
---

A `git` or `url` dependency is not in `flick.lock`, or its sources are not
in the package store: nothing fetched them yet, or the manifest changed
since. `flick install` fetches what `flick.lock` pins and adds what it
lacks; `flick install --locked` fails instead of changing the lock.

```toml
[dependencies]
text = { git = "https://github.com/acme/text", tag = "v1.2.0" }
```
