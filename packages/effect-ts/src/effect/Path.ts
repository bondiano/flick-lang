// Effect's `Path` for the bindings of the standard library: the Flick
// service `Path.Path` built from an Effect `Path`.

import { type Context, Effect, Layer, Path, PlatformError } from "effect";

import { nodeModule } from "../host.ts";
import { mapError } from "./PlatformError.ts";

export * from "effect/Path";

const fileUrl = (url: string): Effect.Effect<URL, PlatformError.PlatformError> =>
  Effect.try({
    try: () => new URL(url),
    catch: (cause) =>
      PlatformError.badArgument({ module: "Path", method: "fromFileUrl", description: `invalid URL ${url}`, cause }),
  });

/** The operations of `Path.Path`, curried as Flick calls them. */
const impl = (path: Path.Path) => ({
  sep: () => Effect.succeed(path.sep),
  basename: (p: string) => Effect.sync(() => path.basename(p)),
  basename_without: (suffix: string) => (p: string) => Effect.sync(() => path.basename(p, suffix)),
  dirname: (p: string) => Effect.sync(() => path.dirname(p)),
  extname: (p: string) => Effect.sync(() => path.extname(p)),
  format: (parsed: Path.Path.Parsed) => Effect.sync(() => path.format(parsed)),
  from_file_url: (url: string) => mapError(Effect.flatMap(fileUrl(url), path.fromFileUrl)),
  is_absolute: (p: string) => Effect.sync(() => path.isAbsolute(p)),
  join: (paths: ReadonlyArray<string>) => Effect.sync(() => path.join(...paths)),
  normalize: (p: string) => Effect.sync(() => path.normalize(p)),
  parse: (p: string) => Effect.sync(() => path.parse(p)),
  relative: (from: string) => (to: string) => Effect.sync(() => path.relative(from, to)),
  resolve: (paths: ReadonlyArray<string>) => Effect.sync(() => path.resolve(...paths)),
  resolve_within: (root: string) => (input: string) => Effect.sync(() => path.resolveWithin(root, input)),
  to_file_url: (p: string) => mapError(Effect.map(path.toFileUrl(p), (url) => url.href)),
  to_namespaced_path: (p: string) => Effect.sync(() => path.toNamespacedPath(p)),
});

type Impl = ReturnType<typeof impl>;

const build = <I>(tag: Context.Key<I, Impl>, layer: Layer.Layer<Path.Path>): Layer.Layer<I> =>
  Layer.effect(tag, Effect.map(Effect.service(Path.Path), impl)).pipe(Layer.provide(layer));

/** `Path.layer`: Effect's POSIX paths. */
export const flickLayer = <I>(tag: Context.Key<I, Impl>): Layer.Layer<I> => build(tag, Path.layer);

/** `Path.node`: the paths of Node.js. */
export const flickNode = <I>(tag: Context.Key<I, Impl>): Layer.Layer<I> =>
  Layer.suspend(() => build(tag, nodeModule("@effect/platform-node/NodePath").layer));
