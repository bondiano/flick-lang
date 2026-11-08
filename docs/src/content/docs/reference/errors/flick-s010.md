---
title: "FLICK-S010"
---

The construct comes from other ML languages and Flick does not have it: a
`do` block laid out without braces, `let*` binding operators, `of` before
constructor fields and JSX `<…>`. A `do` block takes braces, `do { … }`;
effectful calls are written directly, deferred work uses `task { … }` and
`await`, constructor fields follow the name as in a pattern
(`Rect Float Float`), UI uses `ui { … }`.

```flick ignore
load_pair : UserId ->{Users; NotFound} (User, User)
let load_pair id =
  let first = Users.get id
  let second = await (task { Users.get id })
  (first, second)
```
