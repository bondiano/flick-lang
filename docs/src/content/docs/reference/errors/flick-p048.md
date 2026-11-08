---
title: "FLICK-P048"
---

A note: a plugin whose package says `hooks = ["ir"]` rewrote the IR of the
build after the checks of effects and before it was optimized. The compiler
checks what it gives for form only, so what the program does is the
plugin's to answer for, outside what Flick's checks and its model of the
language guarantee. A build with this note is not cached.
