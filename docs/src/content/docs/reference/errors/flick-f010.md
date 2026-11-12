---
title: "FLICK-F010"
---

`retention owner` says the JS code keeps an argument until the owner of a
region closes, so the compiler holds the declaration to `Fiber.Outlives` and
`Fiber.Provided`. Those classes belong to the `std` package. A build that
loads only the core modules, such as a host that embeds the compiler without
handing it the implicit packages, has neither, and cannot check the
declaration.

Load `std`: in a project it comes with the toolchain, and `FLICK_STD_PATH`
must name a directory with its `src/modules.txt`. A declaration whose JS
code keeps nothing past the call says `retention call` instead, which needs
no `std`.
