---
title: "FLICK-T059"
source_hash: "5d7358a11df75e86"
---

Сигнатура записывает то, что компилятор и так ей предоставляет.

Внешняя функция с `retention owner` и одним регионом в сигнатуре получает
`Fiber.Outlives`, `Fiber.Provided` и `Detached`, поэтому их писать не нужно:

```flick
add_finalizer : (Eff r none Unit)^c ->{Scope s | r} Unit
foreign effect add_finalizer from "./scope.js" "addFinalizer"
  retention owner
```

Метод экземпляра имеет тип, который задаёт его класс, поэтому сигнатура, повторяющая его, может быть опущена. Та, чьи переменные типа используются в теле метода, остаётся, так как они находятся в области видимости там:

```flick fail T059
# type Box a = Box a
instance Functor Box =
  map : (forall v. a^v -> b)^c -> (Box a)^d -> (Box b)^(c + d)  // warns
  let map f (Box x) = Box (f x)
```

Исправление безопасно: `flick check --fix --only T059`.
