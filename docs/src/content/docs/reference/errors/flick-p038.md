---
title: "FLICK-P038"
---

A plugin asked for something outside its blocks that the project does not
let it reach: a file, a variable of the environment, or a host over HTTP.
Only the project being built grants it, in its own `flick.toml`, for the
plugin of each package; a dependency cannot grant itself anything.

```toml
[plugins.sql]
fs-read = ["schema/", "db/extra.sql"]   # paths from the project's root
net = ["db.example.com", "localhost:8080"]  # a host at any port, or at one
env = ["SQL_TOKEN"]
```

The message names the line to add. A block of a library's own module
reaches nothing.
