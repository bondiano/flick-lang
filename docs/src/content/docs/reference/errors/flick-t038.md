---
title: "FLICK-T038"
---

The cyclomatic complexity of a function, 1 plus each `if`, each `match`
arm after the first, each guard and each `&&` and `||`, is more than
`[lint] max-complexity` in `flick.toml` (10 by default). Each branch is a
path to test. Move groups of branches into functions of their own, or
replace a chain of conditions with a `match`.

Raise the limit with `max-complexity = 15` under `[lint]`, or turn the
warning off with `allow = ["FLICK-T038"]`.
