// `Option` of the standard library without Effect: the one binding Option.fl
// keeps, with Effect's semantics; the rest of the module is written in Flick.
import { none, type Option, some } from "./data.ts";

export const productMany =
  <A>(others: ReadonlyArray<Option<A>>) =>
  (self: Option<A>): Option<Array<A>> => {
    if (self._tag === "None") return none;
    const out = [self.value];
    for (const o of others) {
      if (o._tag === "None") return none;
      out.push(o.value);
    }
    return some(out);
  };
