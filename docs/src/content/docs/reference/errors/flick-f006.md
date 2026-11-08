---
title: "FLICK-F006"
---

`flick ffi check` imports the package again, as the header of a generated
module records it, and compares the result with the file: a dependency
update changed the declarations, `policy.toml` changed, or someone edited
the generated file. Run the import again; reviewed bindings live in other
modules and are not touched.

```text
flick ffi import lookup-lib --out src/Ffi
flick ffi check
```
