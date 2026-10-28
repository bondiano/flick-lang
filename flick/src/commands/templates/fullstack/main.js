// The page's script. Vite builds `App/Web.fl` with `flick build`; when a
// module changes it runs the new program in place of the old one, and the
// labelled atoms (each top-level `let` of a state) keep their values.
import { hotCarry, hotDispose, run } from "@flick-lang/runtime/runner/browser";

import { main } from "./App/Web.fl";

if (import.meta.hot) {
  hotCarry(import.meta.hot.data.states);
  import.meta.hot.dispose((data) => {
    data.states = hotDispose();
  });
  import.meta.hot.accept();
}

run(main);
