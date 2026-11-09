// Effect's `LayerMap` for the bindings of the standard library: options
// with `Option` fields, a record of layers as a list of pairs.
import { type Duration, type Layer, LayerMap, Option } from "effect";

export * from "effect/LayerMap";

/** `LayerMap.make`. */
export const flickMake = <K, I, E, R>(
  lookup: (key: K) => Layer.Layer<I, E, R>,
  options: {
    readonly idleTimeToLive: Option.Option<Duration.Duration>;
    readonly preloadKeys: Option.Option<ReadonlyArray<K>>;
  },
) =>
  LayerMap.make(lookup, {
    idleTimeToLive: Option.getOrUndefined(options.idleTimeToLive),
    preloadKeys: Option.getOrUndefined(options.preloadKeys),
  });

/** `LayerMap.from_record`. */
export const flickFromRecord = <I, E, R>(
  layers: ReadonlyArray<readonly [string, Layer.Layer<I, E, R>]>,
  options: { readonly idleTimeToLive: Option.Option<Duration.Duration>; readonly preload: boolean },
) =>
  LayerMap.fromRecord(Object.fromEntries(layers), {
    idleTimeToLive: Option.getOrUndefined(options.idleTimeToLive),
    preload: options.preload,
  });

/** `LayerMap.get`. */
export const flickGet =
  <K>(key: K) =>
  <I, E>(map: LayerMap.LayerMap<K, I, E>): Layer.Layer<I, E> =>
    map.get(key);

/** `LayerMap.invalidate`. */
export const flickInvalidate =
  <K>(key: K) =>
  <I, E>(map: LayerMap.LayerMap<K, I, E>) =>
    map.invalidate(key);
