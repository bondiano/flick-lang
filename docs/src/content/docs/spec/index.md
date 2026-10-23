---
title: "Flick 0.7 Specification"
---

**Working specification of Flick 0.7. October 21, 2026. Decisions for the first implementation.**

The goal is to design **Flick**, a functional language with ML-like syntax, HKT, type classes and a single Effect backend. The standard library and runtime contracts are based on Effect. Pure sections compile to plain JS/TS as part of the same backend.

Flick is an ML-like language with HKT, type classes and one Effect backend. Direct runtime calls use an ordinary let; `task` builds a cold Eff, and await runs it in the current fiber. A full signature sits above every named function. The first version chooses MPTC/FD, higher-rank callbacks, pure generalization, exact records and List on top of a JS array, fresh regions/captures, explicit FFI with a `.d.ts` importer, full Schema deriving, a PostgreSQL eDSL, `ui { ... }` on top of React and `cli { ... }` on top of Effect CLI. The contracts and the cost of the decisions are fixed below. This is the specification of a Flick language that is not yet implemented; the decision flags do not indicate compiler readiness.

The document can be commented on and edited paragraph by paragraph and example by example. The section “What must be decided before implementation” now contains the accepted answers. The comparisons with alternatives explain tradeoffs; features explicitly excluded from the first version do not remain open blockers. The section “Implementation verification criteria” keeps the still-open checks of the future compiler/runtime.
