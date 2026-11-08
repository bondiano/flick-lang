---
title: "FLICK-P043"
---

The `std` this run uses was written for another compiler, and one of its
modules does not check with this one. A released compiler takes the `std` of
its own release, so this happens with a compiler built from source, or when
`FLICK_STD_PATH` or `[dependencies]` names a `std` of another version.

A compiler built from a checkout takes the `std` of that checkout while the
checkout is where it was built. Otherwise name the `std` of the same source:

```toml
[dependencies]
std = { path = "../flick-lang/packages/std" }
```

or set `FLICK_STD_PATH` to that directory.
