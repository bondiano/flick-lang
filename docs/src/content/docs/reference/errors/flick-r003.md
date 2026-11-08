---
title: "FLICK-R003"
---

No file and no installed package provides the imported module. A local module
`App.Users` lives in `src/App/Users.fl`; a test module may also import from the
test root. A module from an npm package needs the package in `package.json` and
installed: run `flick add <package>`, then `flick install`.

```flick ignore
import App.Users exposing (load_user)   // src/App/Users.fl
```
