// Services and their operations.
import { Context, Effect } from "effect";

/** The implementation of a service: its operations, curried. */
export type ServiceImpl = Readonly<Record<string, unknown>>;

/** The key of `service S` declared in module `M`: `M.S`; `S` is the type of its implementation. */
export const service = <S extends object = ServiceImpl>(key: string): Context.Service<S, S> => Context.Service<S>(key);

/** `f a` for a curried function of the program. */
const apply = (f: unknown, a: unknown): unknown => {
  if (typeof f !== "function") throw new TypeError("the function takes fewer arguments");
  const result: unknown = Reflect.apply(f, undefined, [a]);
  return result;
};

/** The effect of a curried runtime function applied to `args`. */
export const applyRuntime = (fn: unknown, args: ReadonlyArray<unknown>): Effect.Effect<unknown, unknown, unknown> => {
  const result = args.reduce(apply, fn);
  return Effect.isEffect(result) ? result : Effect.die(new TypeError("a runtime function returned no effect"));
};

/** The operation `name` of `impl`, given the dictionaries of its constraints. */
export const implementation = <S extends object>(impl: S, name: keyof S, dicts: ReadonlyArray<unknown>): unknown => {
  const f: unknown = impl[name];
  if (dicts.length === 0) return f;
  if (typeof f !== "function") throw new TypeError("the operation takes no dictionaries");
  const given: unknown = Reflect.apply(f, undefined, dicts);
  return given;
};

/**
 * `S.op`: a function of the operation's arguments up to its first runtime
 * stage that looks the service up when that stage runs.
 */
export const operation = <S extends object>(
  tag: Context.Service<S, S>,
  name: keyof S,
  arity: number,
  dicts: ReadonlyArray<unknown> = [],
): unknown => {
  const call = (args: ReadonlyArray<unknown>): Effect.Effect<unknown, unknown, unknown> =>
    Effect.flatMap(Effect.service(tag), (impl) => applyRuntime(implementation(impl, name, dicts), args));
  const collect = (args: ReadonlyArray<unknown>): unknown =>
    args.length === arity ? call(args) : (a: unknown) => collect([...args, a]);
  return arity === 0 ? call([]) : collect([]);
};
