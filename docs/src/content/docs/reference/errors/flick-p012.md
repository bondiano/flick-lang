---
title: "FLICK-P012"
---

The go target (`emit = "go"`) compiles what `main` reaches, and reports
here a construct it does not compile yet, with what it names: a lazy `for`
that reads services, refined types, schemas deriving `Form`, or a `foreign`
binding without a Go implementation, in the standard library or beside the
program. It also reports a `main` that does not take one parameter, an app
without an entry, and a package module not available on the go target.
The js target builds the same program.

When the construct is the program's own `foreign` binding, add its Go
implementation beside the program (see the FFI reference). Otherwise keep
the construct out of what `main` reaches, or build with `emit = "js"`.
