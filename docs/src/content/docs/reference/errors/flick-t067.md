---
title: "FLICK-T067"
---

# Public functor requires an explicit result signature

A public functor must declare the module signature of its result. Write
`module App.Make(K : Contracts.Input) : Contracts.Output`. The result may
depend on the argument through explicit type equalities. Local functors may
infer their result; an exported alias may inherit an explicit signature.
