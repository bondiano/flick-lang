---
title: "FLICK-T067"
---

# Public parameterized module requires an explicit result signature

A public parameterized module must declare the module signature of its result. Write
`module App.Make(K : Contracts.Input) : Contracts.Output`. The result may
depend on the argument through explicit type equalities. Local parameterized modules may
infer their result; an exported alias may inherit an explicit signature.
