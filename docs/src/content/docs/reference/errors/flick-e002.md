---
title: "FLICK-E002"
---

A signature is an upper bound on the services and errors its body needs: the
body may need fewer, never more. The same holds for a value or a `task` checked
against an expected type. Add what is missing to the signature, provide the
service, or handle the error with `try … with`.

```flick ignore
// was: load_name : UserId ->{Users; NotFound} String
load_name : UserId ->{Users, Log; NotFound} String
let load_name id =
  let user = Users.get id
  Log.info user.name
  user.name
```
