---
title: "FLICK-P037"
---

Retired. A block of the plugin of `std`, such as `cli "name" { … }`, needed
an import of its head, `import Cli exposing (plugin cli)`. The heads of
`std`'s plugin now start a block in every module without one.

The code remains registered for historical diagnostics and is never reused.
