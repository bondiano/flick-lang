---
title: "FLICK-P066"
---

An environment condition in `@when`, `[package] when`, or `[host] requires`
names a tag that is not declared by any known backend descriptor or loaded
`[host]` package.

Environment tags are validated against the combined vocabulary so typos fail
immediately at the condition instead of silently evaluating to false. Check the
spelling against the backend's tags and axes or add the host package that
defines the tag:

```flick
@when "js && !browser"
module App.Server exposing (start)

let start = ()
```
