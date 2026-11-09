---
title: "FLICK-T006"
---

A signature describes the definition right below it. A signature with no
definition, or with the definition elsewhere, is an error; services, classes and
`foreign` declarations are the places where a signature stands alone or above a
`foreign`.

```flick
now : Unit ->{} Float
foreign now from global "Date.now"
```
