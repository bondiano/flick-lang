---
title: "FLICK-S008"
---

An opening `(`, `[` or `{` has no matching closing delimiter. The related span
points at the opener. Closing delimiters may be dedented, but must come before
the next declaration.

```flick ignore
let pair =
  (first,
   second)
```
