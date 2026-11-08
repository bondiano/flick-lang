---
title: "FLICK-U005"
---

An atom holds one value. A binding built purely is generalized, so
`let items = Atom.make []` would have the type
`forall a. Atom.Writable (List a)`: one state that one reader takes for a
list of numbers and another for a list of strings, each reading what the
other wrote. A top-level value that holds an atom (or a family of them)
therefore has a type without variables inside the atom's type: annotate it
with the type of its value. In a block, an atom without a type is not
generalized, and a signature, annotation or higher-rank argument that gives
it variables is rejected the same way. A function making an atom per call
may stay polymorphic.

```flick ignore
items : Atom.Writable (List Float)
let items = Atom.make []

make_list : Unit -> Atom.Writable (List a)
let make_list () = Atom.make []
```
