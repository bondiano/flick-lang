---
title: "FLICK-E015"
source_hash: "7fbbf089846f9b06"
---

Ветка `return` обработчика принимает значение своего тела по завершении тела. У обработчика не более одной такой ветки, у неё нет условия (guard), и её шаблон совпадает с любым значением: значению просто некуда больше идти.

```text
collect : forall a r e. (Unit ->{Emit a | r; e} Unit) ->{r; e} List a
let collect body =
  try
    body ()
  with
  | Emit.emit x -> x :: resume ()
  // was: | return () when False -> []
  | return () -> []
```
