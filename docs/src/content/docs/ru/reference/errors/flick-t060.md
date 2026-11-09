---
title: "FLICK-T060"
source_hash: "d8c93589afaac170"
---

Именованный модуль в `pack M as Sig` должен публично экспортировать все члены, которые требует `type module Sig`. Добавьте существующий член в список `exposing` либо объявите его с нужным именем.

```flick
type module KeySource =
  type Key
  equal : Key -> Key -> Bool

// A module packed as KeySource must expose both Key and equal.
```
