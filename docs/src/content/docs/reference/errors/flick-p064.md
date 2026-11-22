---
title: "FLICK-P064"
---

A reachable use from the program's entry or a library's public exports depends
on a module, package, or foreign binding whose environment condition is not
satisfied by the selected environment.

Unused imports and unreachable functions are allowed so cross-environment
modules can share types and signatures. The diagnostic points to the use site
and shows the path from the entry or public export that reached it.

Either switch `[build] environment` (or `[target.<backend>]` options) to an
environment whose tags satisfy the condition, add the required dependency (such
as `@flick-lang/effect-ts` for Effect-adapter bindings on JS `rt`), or move
environment-specific code behind a module gated for that environment:

```flick
@when "js && browser"
module App.Browser exposing (mount)

let mount = ()
```
