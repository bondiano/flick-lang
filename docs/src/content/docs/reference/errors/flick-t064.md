---
title: "FLICK-T064"
---

An `unpack` opens a fresh witness for each abstract type in the packed
module. A value whose type mentions that witness cannot leave the `unpack`
scope as an ordinary type. Return a value independent of the abstract type,
or declare an explicit `exists K : Sig. K.T` result and keep its witness.
