---
title: "Type equality"
---

Row expressions are compared by their meaning throughout a type, including
inside invariant containers. Reordering services, repeating the same service
or grouping unions differently preserves the type.

```flick
service Log =
  line : String ->{} Unit

service Clock =
  now : Unit ->{} Int

same : Ref (Eff {Log, Clock} none Unit) -> Ref (Eff {Clock, Log} none Unit)
let same cell = cell
```

The reference still has one content type. Equality does not allow widening
the requirements or errors of a computation stored in it:

```flick fail T003
# service Log =
#   line : String ->{} Unit
# service Clock =
#   now : Unit ->{} Int
widen : Ref (Eff {Log} none Unit) -> Ref (Eff {Log, Clock} none Unit)
let widen cell = cell
```

The same distinction applies to nominal arguments and record fields. Type equality preserves service identities and their type
arguments, pure/runtime stages, regions and foreign contracts. It does not
make different nominal declarations interchangeable. See [kinds and types](../types/),
[effects](../effects/) and [GADTs](../gadts/).

A GADT branch's unification problem also uses type equality: a different
presentation of the same row cannot make a branch impossible. Patterns can
also [refine service, error and capture row indices](../row-indices/).
Region identities remain rigid.

These are shared language rules for JS and Go. A backend receives the checked
program; its output representation does not choose a different type equality.
