// The browser's local storage as a state: `Atom.stored`.
import { Option } from "effect";
import * as Atom from "./effect/reactivity/Atom.ts";

/** `Atom.stored key`: the text the browser's local storage keeps under `key`. */
export const atomStored = (key: string): Atom.Writable<Option.Option<string>> => {
  const read = (): Option.Option<string> =>
    typeof localStorage === "undefined" ? Option.none() : Option.fromNullishOr(localStorage.getItem(key));
  return Atom.keepAlive(
    Atom.writable<Option.Option<string>, Option.Option<string>>(
      (get) => {
        if (typeof window !== "undefined") {
          // another tab's write
          const changed = (event: StorageEvent): void => {
            if (event.key === key || event.key === null) get.setSelf(read());
          };
          window.addEventListener("storage", changed);
          get.addFinalizer(() => window.removeEventListener("storage", changed));
        }
        return read();
      },
      (ctx, value) => {
        if (typeof localStorage !== "undefined") {
          if (Option.isSome(value)) localStorage.setItem(key, value.value);
          else localStorage.removeItem(key);
        }
        ctx.setSelf(value);
      },
    ),
  );
};
