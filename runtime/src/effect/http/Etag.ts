// Effect's `Etag` for the bindings of the standard library: the Flick
// service `Http.Etag.Generator` over Effect's generators.
import { type Context, Effect, type FileSystem, Layer, Option } from "effect";
import * as Etag from "effect/http/Etag";

import { tag } from "../../tags.ts";

export * from "effect/http/Etag";

const WEAK = /*#__PURE__*/ tag("Http.Etag.Etag.Weak");
const STRONG = /*#__PURE__*/ tag("Http.Etag.Etag.Strong");

/** The Flick `Http.Etag.Etag` of Effect's. */
export const toFlick = (etag: Etag.Etag) => ({ _tag: etag._tag === "Weak" ? WEAK : STRONG, _0: etag.value });

/** What Effect's generators read of a Flick `FileSystem.Info`. */
interface FlickInfo {
  readonly size: bigint;
  readonly mtime: Option.Option<number>;
}

const info = (i: FlickInfo) =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- Effect's generators read only `size` and `mtime`
  ({ size: i.size, mtime: Option.map(i.mtime, (ms) => new Date(ms)) }) as unknown as FileSystem.File.Info;

const impl = (generator: Etag.Generator["Service"]) => ({
  from_file_info: (i: FlickInfo) => Effect.map(generator.fromFileInfo(info(i)), toFlick),
});

type Impl = ReturnType<typeof impl>;

const of =
  (layer: Layer.Layer<Etag.Generator>) =>
  <I>(key: Context.Key<I, Impl>): Layer.Layer<I> =>
    Layer.effect(key, Effect.map(Effect.service(Etag.Generator), impl)).pipe(Layer.provide(layer));

/** `Http.Etag.layer`. */
export const flickLayer = /*#__PURE__*/ of(Etag.layer);

/** `Http.Etag.layer_weak`. */
export const flickLayerWeak = /*#__PURE__*/ of(Etag.layerWeak);
