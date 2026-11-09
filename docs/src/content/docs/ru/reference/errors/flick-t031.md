---
title: "FLICK-T031"
source_hash: "67c276bf2a0ea389"
---

Предупреждение: `Seq.to_list` последовательности, которая никогда не заканчивается, такой как
`Seq.from`, `Seq.repeat`, `Seq.iterate` или `Seq.cycle`, и то, что `Seq.map` или
`Seq.filter` из них делает, никогда не возвращается. Сначала возьмите её часть с помощью
`Seq.take` или `Seq.take_while`.

```flick ignore
first_squares : List Float
let first_squares = Seq.from 1 |> Seq.map (fun n -> n * n) |> Seq.take 5 |> Seq.to_list
```
