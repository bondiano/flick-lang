---
title: "FLICK-P061"
---

A Flick program executed by a `run-flick` plan step (such as `flick schema export`)
or by `flick doc --check-examples` failed at runtime, timed out after 60 seconds,
exceeded the stdout size limit, or could not be launched on the selected backend.

Inspect the runtime output included in the diagnostic and fix the failing code at
the highlighted span. For a documentation example that is intended to show an
error rather than run, mark its code fence with `fail` or `ignore`.

```sh
flick doc --out-dir api --check-examples
```
