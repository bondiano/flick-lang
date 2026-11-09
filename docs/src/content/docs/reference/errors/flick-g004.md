---
title: "FLICK-G004"
---

An operation that needs `Scope s` acquires a resource and registers its
release with the owner of region `s`, which only `scope { … }` provides. Call
it inside a scope, or state the requirement in the signature so that the
caller's scope owns the resource.

```flick
# import File exposing (File, Path, OpenError, ReadError, FileSystem)
read_config : Path ->{FileSystem, Async; OpenError, ReadError} String
let read_config path =
  scope {
    let file = File.open_scoped path
    File.read_all file
  }
```
