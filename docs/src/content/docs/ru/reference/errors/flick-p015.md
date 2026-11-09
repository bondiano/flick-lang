---
title: "FLICK-P015"
source_hash: "fc4abdc01ed1b97b"
---

Установленные `@flick-lang/runtime`, `@flick-lang/effect-ts` или `effect` не соответствуют версии, для которой был собран этот компилятор, поэтому их ABI может отличаться. `check`, `build`, `run` и `test` отказываются продолжать работу, чтобы не создавать код с неправильным поведением. Установите версии, зафиксированные в шаблоне проекта.

```sh
flick add @flick-lang/runtime@0.1.0 @flick-lang/effect-ts@0.1.0 effect@4.1.2
flick install
```
