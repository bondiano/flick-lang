---
title: "FLICK-T060"
---

The named module in `pack M as Sig` must publicly export every member that
`type module Sig` requires. Export an existing member through the module's
`exposing` list, or add a definition with the required name.

```flick
type module KeySource =
  type Key
  equal : Key -> Key -> Bool

// A module packed as KeySource must expose both Key and equal.
```
