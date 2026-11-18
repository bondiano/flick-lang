// The program's `Host.Environment` as the adapter reads it. What a function of
// the standard library reads of the environment when nothing else says,
// a configuration, the terminal's or OTLP's settings, goes through it, so
// the function's row names the service and a fake one answers.
import { ConfigProvider, Context, Effect, Layer } from "effect";

import { callEffect } from "./bridge.ts";
import { service } from "./effects.ts";

interface HostEnvironment {
  readonly env: (name: string) => unknown;
  readonly vars: (unit: undefined) => unknown;
}

const HOST_ENVIRONMENT = /*#__PURE__*/ service<HostEnvironment>("Host.Environment");

/** What an operation gives: an Effect, or the value or the call of the kernel a service of Flick's own runtime gives. */
const performed = <A>(result: unknown): Effect.Effect<A> =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the operation's signature fixes its result
  (Effect.isEffect(result) ? result : callEffect(() => result)) as Effect.Effect<A>;

/** `Host.Environment.env name`, `undefined` when the variable is unset. */
export const hostEnv = (name: string): Effect.Effect<string | undefined, never, unknown> =>
  Effect.flatMap(Effect.service(HOST_ENVIRONMENT), (impl) =>
    Effect.map(performed<{ readonly _tag: string; readonly value?: string }>(impl.env(name)), (found) =>
      found._tag === "Some" ? found.value : undefined,
    ),
  );

/** `ConfigProvider.from_env` of the variables `Host.Environment.vars` lists. */
export const hostProvider: Effect.Effect<ConfigProvider.ConfigProvider, never, unknown> = /*#__PURE__*/ Effect.flatMap(
  Effect.service(HOST_ENVIRONMENT),
  (impl) =>
    Effect.map(performed<ReadonlyArray<readonly [string, string]>>(impl.vars(undefined)), (pairs) =>
      ConfigProvider.fromEnvRecord(Object.fromEntries(pairs)),
    ),
);

/** Whether a layer made a provider current. */
export const hasProvider = <R>(context: Context.Context<R>): boolean =>
  context.mapUnsafe.has(ConfigProvider.ConfigProvider.key);

/** The provider a layer made current, or else `hostProvider`. */
export const currentProvider: Effect.Effect<ConfigProvider.ConfigProvider, never, unknown> = /*#__PURE__*/ Effect.flatMap(
  Effect.context(),
  (context) => (hasProvider(context) ? Effect.succeed(Context.get(context, ConfigProvider.ConfigProvider)) : hostProvider),
);

/** `layer`, whose configuration Effect reads from `currentProvider`. */
export const configured = <A, E, R>(layer: Layer.Layer<A, E, R>) =>
  Layer.unwrap(
    Effect.map(currentProvider, (provider) => Layer.provide(layer, Layer.succeed(ConfigProvider.ConfigProvider, provider))),
  );
