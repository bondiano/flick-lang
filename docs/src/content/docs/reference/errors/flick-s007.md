---
title: "FLICK-S007"
---

The parser found a token where the grammar does not allow it. The message
names what was expected. After the error the parser skips to the next
statement or declaration, so later declarations are still checked.

```flick ignore
// error: `let add left right = left +`
let add left right =
  left + right
```
