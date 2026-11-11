---
title: "FLICK-R017"
---

A private module can be imported only by source and test files of its own package.
The module still belongs to its package when a dependency is compiled from source
or restored from the package cache. Import a public facade that re-exports the
needed names, or ask the package author to make the module public.
