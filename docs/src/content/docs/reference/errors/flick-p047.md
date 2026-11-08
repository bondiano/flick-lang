---
title: "FLICK-P047"
---

A plugin whose package says `hooks = ["ir"]` gave the build an IR that
does not hold together: a module named twice, a local used where nothing
binds it, a global of a module the build has not, a span of a file the build
did not read, or a program of another version of the IR. The build stops,
naming the plugin, rather than compile it.

The plugin is at fault: it must give back a `Program` of the version it got,
whose every local is bound where it is used.
