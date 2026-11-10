---
title: "FLICK-P015"
---

The compiler requires an ABI-compatible `@flick-lang/runtime`. An installed
`@flick-lang/effect-ts` owns its exact dependency requirements in `package.json`;
its version is selected by the project. This error reports an incompatible
kernel ABI, an unreadable adapter manifest, or unsatisfied adapter dependencies.

Restore the dependencies selected by the project and its lockfile:

```sh
flick install
```

For an adapter dependency mismatch, satisfy the installed adapter's requirements.
For a kernel ABI mismatch, select a compatible runtime or compiler. If the
adapter manifest is damaged, reinstall the selected adapter from the lockfile.
`build`, `run` and `test` check compatibility before emitting executable code;
installation also checks the resulting package tree.
