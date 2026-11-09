---
title: "FLICK-U003"
source_hash: "801b4ee8e6b12de6"
---

Сериализуемый атом известен реестру по своему ключу, и
`Atom.serializable` получает этот ключ из верхнеуровневого связывания, которое он определяет:
`let count = Atom.serializable codec (Atom.make 0)` в `App.Counter` имеет
ключ `App.Counter.count`. Используемый в любом другом месте, один ключ может обозначать множество
атомов: внутри функции каждый вызов создаст новый атом с тем же
ключом, а два атома в одном связывании будут его разделять. Определяйте каждый сериализуемый атом
с помощью собственного верхнеуровневого `let` без параметров, применённого к его кодеку и
состоянию там; `Atom.serializable_family` даёт один атом на ключ.

```flick
count : Atom.Writable Float
let count = Atom.serializable (Codec.json Schema.number) (Atom.make 0)

draft : String -> Atom.Writable String
let draft = Atom.serializable_family (Codec.json Schema.string) (fun _ -> Atom.make "")
```
