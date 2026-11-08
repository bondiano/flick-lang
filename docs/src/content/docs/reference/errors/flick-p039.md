---
title: "FLICK-P039"
---

A plugin fetched over HTTP in an offline run, `--offline` or with `CI` set,
and no snapshot has the answer. An online run keeps each answer under
`.flick/plugin-snapshots/<package>/`, which a project commits so that CI
and offline builds read the same answers; an editor runs offline too.

Run `flick check` online once and commit the snapshots it writes. A
snapshot is refreshed by deleting it.
