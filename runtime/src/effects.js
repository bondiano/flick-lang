// Runtime constructs of compiled Flick code: services and their operations,
// `provide`, `try … with` and lifted callbacks.
import { Cause, Context, Effect, Layer, Result } from "effect";

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

/** `provide layer { body }`: the layer is built before the body runs. */
export const provide = (layer, body) => Effect.provide(body, layer);

const failsToDies = (cause) =>
  Cause.fromReasons(
    cause.reasons.map((r) => (Cause.isFailReason(r) ? Cause.makeDieReason(r.error) : r)),
  );

/**
 * `try body with arms`. `handler` returns the handling effect for a typed
 * error an arm matches, `undefined` otherwise. The error is the first
 * failure of the cause; a cause that also holds a defect is not recovered,
 * its failures become defects; without a matching arm the original cause
 * propagates.
 */
export const tryWith = (body, handler) =>
  Effect.catchCauseFilter(
    body,
    (cause) => {
      if (Cause.hasDies(cause)) {
        return Cause.hasFails(cause) ? Result.succeed({ die: failsToDies(cause) }) : Result.fail(cause);
      }
      const reason = cause.reasons.find(Cause.isFailReason);
      const handled = reason === undefined ? undefined : handler(reason.error);
      return handled === undefined ? Result.fail(cause) : Result.succeed({ handled });
    },
    (selected) => (selected.die ? Effect.failCause(selected.die) : selected.handled),
  );

export const layerSucceed = (tag, impl) => Layer.succeed(tag, impl);

export const layerMerge = (a, b) => Layer.merge(a, b);

/** Both layers; services `b` builds replace those of `a`. */
export const layerOverride = (a, b) => Layer.merge(a, b);
