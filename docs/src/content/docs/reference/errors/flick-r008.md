---
title: "FLICK-R008"
---

A module defines a name twice in one namespace: two values, two types or two
constructors with the same name. A signature and its `let` are one
definition. Rename or remove one of them.

```flick fail R008
type Light = Green | Red
type Stop = Red | Amber
```

The `suite` and `test` blocks of a module declare its `tests`, so a module
with blocks that also declares `tests` by hand defines it twice: move the
hand-written tests into a block, or drop the blocks.

```flick fail R008
tests : List Test.Test
let tests = [Test.test "by hand" (fun () -> Test.Pass)]

test "a block" {}
```
