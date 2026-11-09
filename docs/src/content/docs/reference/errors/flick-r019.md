---
title: "FLICK-R019"
---

A named static functor application creates a module path that other files can
import. An inline module is a private namespace inside one source file, so a
static application cannot be declared in its body. Place the application in a
file module, then import its result into the inline body if needed. Values,
types, classes, instances, services and `type module` contracts can be declared
directly inside an inline module.
