// Effect's `LayerRef` for the bindings of the standard library: options
// with `Option` fields, and the members of a ref as functions.
import { type Duration, type Layer, LayerRef, Option } from "effect";

export * from "effect/LayerRef";

/** `LayerRef.make`. */
export const flickMake = <I, E, R>(
  layer: Layer.Layer<I, E, R>,
  options: { readonly idleTimeToLive: Option.Option<Duration.Duration>; readonly preload: boolean },
) =>
  LayerRef.make(layer, {
    idleTimeToLive: Option.getOrUndefined(options.idleTimeToLive),
    preload: options.preload,
  });

export const flickGet = <I, E>(ref: LayerRef.LayerRef<I, E>): Layer.Layer<I, E> => ref.get;
export const flickInvalidate = <I, E>(ref: LayerRef.LayerRef<I, E>) => ref.invalidate;
export const flickRefresh = <I, E>(ref: LayerRef.LayerRef<I, E>) => ref.refresh;
