---
title: "FLICK-G002"
---

`scope { … }` releases what it acquired when its body ends. A value that
holds a region of the scope (a handle, or a closure or `task` that uses one)
cannot be returned from the scope, stored in a cell created outside it, or
raised as an error. Finish the work inside the scope, or move the scope into
the deferred work so each run acquires its own resource.

```flick
# import File exposing (File, Path, OpenError, ReadError, FileSystem)
// was: scope { let file = File.open_scoped path
//              task { File.read_all file } }
deferred_read : Path -> Eff {FileSystem, Async} {OpenError, ReadError} String
let deferred_read path =
  task {
    scope {
      let file = File.open_scoped path
      File.read_all file
    }
  }
```
