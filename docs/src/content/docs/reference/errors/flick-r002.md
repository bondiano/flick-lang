---
title: "FLICK-R002"
---

Two files declare the same full module name, for example a module in the
test root with the name of a source module, or a local module with the name
of a module from an installed package. Module names are global within a
project: rename one of them.

```flick ignore
// test/App/UsersTest.fl, not test/App/Users.fl
module App.UsersTest exposing (tests)
```
