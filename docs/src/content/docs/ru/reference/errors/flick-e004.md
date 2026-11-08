---
title: "FLICK-E004"
source_hash: "04a4ad9f1db76d3c"
---

`Layer.merge` создает окружение с одной реализацией каждого сервиса; два слоя, которые оба создают сервис, оставляют выбор неоднозначным. Удалите сервис из одного слоя или укажите, какой из них побеждает, с помощью `Layer.override`, где второй слой заменяет первый.

```flick ignore
// was: Layer.merge users_live users_test
Layer.override users_live users_test
```
