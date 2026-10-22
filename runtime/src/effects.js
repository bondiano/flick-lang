// Services and their operations.
import { Context, Effect } from "effect";

/** The key of `service S` declared in module `M`: `M.S`. */
export const service = (key) => Context.Service(key);

/**
 * `S.op`: a function of the operation's arguments up to its first runtime
 * stage that looks the service up when that stage runs.
 */
export const operation = (tag, name, arity) => {
  const call = (args) =>
    Effect.flatMap(Effect.service(tag), (impl) => args.reduce((f, a) => f(a), impl[name]));
  const collect = (args) =>
    args.length === arity ? call(args) : (a) => collect([...args, a]);
  return arity === 0 ? call([]) : collect([]);
};
