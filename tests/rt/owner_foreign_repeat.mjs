import { Effect, scopeCurrent } from "@flick-lang/effect-ts";

export const makeRepeat = (owner, normalize = (value) => value) => {
  let work;
  work = Effect.flatMap(scopeCurrent(), (current) => {
    if (current !== normalize(owner)) return Effect.die(new Error("wrong recursive task owner"));
    return Effect.succeed(work);
  });
  return work;
};
