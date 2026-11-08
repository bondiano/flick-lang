---
title: "FLICK-P032"
source_hash: "b33c7ee918458627"
---

Зависимость в `[dependencies]` не указывает ровно один источник, или
`[package]` имеет версию или требование `flick`, которое не соответствует semver.
Зависимость может быть из директории, тега git-репозитория или
архива с контрольной суммой:

```toml
[package]
name = "ids"
version = "0.2.0"
flick = ">=0.1, <0.2"

[dependencies]
geo = { path = "../geo" }
std = { git = "https://github.com/bondiano/flick-lang", tag = "std-v0.1.0", subdir = "packages/std" }
text = { url = "https://example.com/text-1.0.0.tgz", sha256 = "<64 hexadecimal digits>" }
```
