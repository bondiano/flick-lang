---
title: "FLICK-T063"
---

`Module Sig` can carry a class or service only when `Sig` fixes its nominal
identity with a manifest requirement such as `class Eq = Prelude.Eq` or
`service Clock = Time.Clock`. An abstract class or service shape is enough
for a parameterized module parameter, but `pack` cannot recover its identity after
`unpack`.
