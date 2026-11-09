---
title: "FLICK-U003"
---

A serializable atom is known to the registry by its key, and
`Atom.serializable` takes that key from the top-level binding it defines:
`let count = Atom.serializable codec (Atom.make 0)` in `App.Counter` has the
key `App.Counter.count`. Used anywhere else, one key could stand for many
atoms: inside a function each call would make another atom with the same
key, and two in one binding would share it. Define each serializable atom
with its own top-level `let` without parameters, applied to its codec and
state there; `Atom.serializable_family` gives one atom per key.

```flick
count : Atom.Writable Float
let count = Atom.serializable (Codec.json Schema.number) (Atom.make 0)

draft : String -> Atom.Writable String
let draft = Atom.serializable_family (Codec.json Schema.string) (fun _ -> Atom.make "")
```
