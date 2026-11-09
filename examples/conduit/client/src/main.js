// The page's script and the Conduit press theme: Vite builds
// `Conduit/Client/Main.fl` with `flick build`, and runs the new program in
// place of the old one when a module changes, the top-level atoms keeping
// their values. The theme is the client's own two-ink stylesheet; the
// vendored RealWorld theme stays untouched in vendor/.
import './theme.css';

import { hotCarry, hotDispose, run } from "@flick-lang/effect-ts/runner/browser";

import { main } from "./Conduit/Client/Main.fl";

if (import.meta.hot) {
  hotCarry(import.meta.hot.data.states);
  import.meta.hot.dispose((data) => {
    data.states = hotDispose();
  });
  import.meta.hot.accept();
}

run(main);
