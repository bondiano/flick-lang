---
title: "FLICK-F014"
---

A package's declared value-ABI requirement under `[package] abi` (or
`[package.abi]`) in `flick.toml`, or a foreign binding request's ABI version,
does not match the versioned value-ABI table published by the selected backend.

Update the package's `[package.abi]` version requirement to match the active
backend, select a compatible backend release, or gate a backend-specific
package with `[package] when`:

```toml
[package]
name = "my-ffi"
version = "0.1.0"
when = "js"

[package.abi]
"flick.javascript-ffi" = "^0.1.0"
```
