---
title: "FLICK-P055"
---

A contributed command returned a plan the host refuses before anything
happens: it writes a path its descriptor does not declare, or leaves the
project, or one of its steps runs a tool the host did not pin for it, or
reads a file the plan does not have. Nothing is written and no tool runs.
The command's authors should fix the plan or its declaration; a tool the
project does not allow is allowed in `[tools]`.
