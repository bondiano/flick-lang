// Effect's `ConfigProvider` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { ConfigProvider, Effect, FileSystem, Option, Path } from "effect";

import { tag } from "@flick-lang/runtime/tags";
import { effectFileSystem } from "./FileSystem.ts";

export * from "effect/ConfigProvider";

const VALUE = /*#__PURE__*/ tag("ConfigProvider.Node.ValueNode");
const SOURCE_ERROR = /*#__PURE__*/ tag("ConfigProvider.SourceError.SourceError");

/** `ConfigProvider.Node` as Flick code sees it. */
interface FlickNode {
  readonly _tag: string;
  readonly _0: string | number | ReadonlyArray<string>;
  readonly _1?: Option.Option<string>;
}

/** The Effect node of a `ConfigProvider.Node`. */
const node = (n: FlickNode): ConfigProvider.Node => {
  const value = Option.getOrUndefined(n._1 ?? Option.none());
  if (n._tag === VALUE || typeof n._0 === "string") return ConfigProvider.makeValue(String(n._0));
  return Array.isArray(n._0) ? ConfigProvider.makeRecord(new Set(n._0), value) : ConfigProvider.makeArray(Number(n._0), value);
};

/** The Effect `SourceError` of a failure of a Flick `load`. */
const sourceError = (e: unknown): ConfigProvider.SourceError => {
  const message: unknown = typeof e === "object" && e !== null && Reflect.get(e, "_tag") === SOURCE_ERROR ? Reflect.get(e, "_0") : undefined;
  return new ConfigProvider.SourceError({
    message: typeof message === "string" ? message : "the provider failed",
    cause: e,
  });
};

/** `ConfigProvider.make`. */
export const flickMake = (
  load: (path: ConfigProvider.Path) => Effect.Effect<Option.Option<FlickNode>, unknown>,
): ConfigProvider.ConfigProvider =>
  ConfigProvider.make((path) =>
    Effect.mapBothEager(load(path), { onFailure: sourceError, onSuccess: (n) => Option.getOrUndefined(Option.map(n, node)) }),
  );

interface Options {
  readonly preserveEmptyStrings: boolean;
}

/** `ConfigProvider.from_unknown`. */
export const flickFromUnknown = (options: Options, root: unknown): ConfigProvider.ConfigProvider =>
  ConfigProvider.fromUnknown(root, options);

/** `ConfigProvider.from_env_record`: the variables as pairs. */
export const flickFromEnvRecord = (
  options: Options,
  env: ReadonlyArray<readonly [string, string]>,
): ConfigProvider.ConfigProvider => ConfigProvider.fromEnvRecord(Object.fromEntries(env), options);

/** `ConfigProvider.from_dot_env_contents`. */
export const flickFromDotEnvContents = (
  options: { readonly expandVariables: boolean; readonly preserveEmptyStrings: boolean },
  text: string,
): ConfigProvider.ConfigProvider => ConfigProvider.fromDotEnvContents(text, options);

/** `ConfigProvider.from_dir`: through the Flick `FileSystem`, with Effect's POSIX paths. */
export const flickFromDir = (options: {
  readonly rootPath: Option.Option<string>;
  readonly preserveEmptyStrings: boolean;
}) =>
  Effect.flatMap(effectFileSystem, (fs) =>
    ConfigProvider.fromDir({
      rootPath: Option.getOrUndefined(options.rootPath),
      preserveEmptyStrings: options.preserveEmptyStrings,
    }).pipe(Effect.provideService(FileSystem.FileSystem, fs), Effect.provide(Path.layer)),
  );

/** `ConfigProvider.layer_add`. */
export const flickLayerAdd = (options: { readonly asPrimary: boolean }, provider: ConfigProvider.ConfigProvider) =>
  ConfigProvider.layerAdd(provider, options);
