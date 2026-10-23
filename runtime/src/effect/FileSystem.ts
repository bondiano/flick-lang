// Effect's `FileSystem` for the bindings of the standard library: the Flick
// service `FileSystem.FileSystem` built from an Effect `FileSystem`, the
// operations of an open file, and back, the Effect `FileSystem` that Effect
// code (`ConfigProvider.fromDir`, `Logger.toFile`) needs, from the Flick one.
import * as NodeFileSystem from "@effect/platform-node/NodeFileSystem";
import { type Context, Effect, FileSystem, Layer, Option, type PlatformError } from "effect";

import type { Data } from "../data.ts";
import { applyRuntime, type ServiceImpl, service } from "../effects.ts";
import { tag } from "../tags.ts";
import { mapError } from "./PlatformError.ts";

export * from "effect/FileSystem";

type Bytes = ReadonlyArray<number>;

const enumeration = <T extends string>(type: string, names: ReadonlyArray<readonly [string, T]>) => {
  const toJs = new Map(names.map(([name, js]) => [tag(`FileSystem.${type}.${name}`), js]));
  const toFlick = new Map(names.map(([name, js]) => [js, Object.freeze({ _tag: tag(`FileSystem.${type}.${name}`) })]));
  return {
    js: (value: Data): T => {
      const js = toJs.get(value._tag);
      if (js === undefined) throw new TypeError(`not a ${type}: ${value._tag}`);
      return js;
    },
    flick: (js: T): Data => {
      const value = toFlick.get(js);
      if (value === undefined) throw new TypeError(`not a ${type}: ${js}`);
      return value;
    },
  };
};

export const openFlag = enumeration<FileSystem.OpenFlag>("OpenFlag", [
  ["Read", "r"],
  ["ReadWrite", "r+"],
  ["Write", "w"],
  ["WriteExclusive", "wx"],
  ["WriteRead", "w+"],
  ["WriteReadExclusive", "wx+"],
  ["Append", "a"],
  ["AppendExclusive", "ax"],
  ["AppendRead", "a+"],
  ["AppendReadExclusive", "ax+"],
]);

const seekMode = enumeration<FileSystem.SeekMode>("SeekMode", [
  ["Start", "start"],
  ["Current", "current"],
]);

const fileType = enumeration<FileSystem.File.Type>("FileType", [
  ["RegularFile", "File"],
  ["Directory", "Directory"],
  ["SymbolicLink", "SymbolicLink"],
  ["BlockDevice", "BlockDevice"],
  ["CharacterDevice", "CharacterDevice"],
  ["Fifo", "FIFO"],
  ["Socket", "Socket"],
  ["UnknownType", "Unknown"],
]);

const millis = (date: Option.Option<Date>): Option.Option<number> => Option.map(date, (d) => d.getTime());

/** `FileSystem.Info`, in the order of its fields. */
export const info = (i: FileSystem.File.Info) => ({
  type_: fileType.flick(i.type),
  mtime: millis(i.mtime),
  atime: millis(i.atime),
  birthtime: millis(i.birthtime),
  dev: i.dev,
  ino: i.ino,
  mode: i.mode,
  nlink: i.nlink,
  uid: i.uid,
  gid: i.gid,
  rdev: i.rdev,
  size: i.size,
  blksize: i.blksize,
  blocks: i.blocks,
});

interface WriteOptions {
  readonly flag: Option.Option<Data>;
  readonly mode: Option.Option<number>;
}

interface OpenOptions extends WriteOptions {
  readonly noFollow: boolean;
}

interface TempFileOptions {
  readonly directory: Option.Option<string>;
  readonly prefix: Option.Option<string>;
  readonly suffix?: Option.Option<string>;
}

const writeOptions = (o: WriteOptions) => ({
  flag: Option.getOrUndefined(Option.map(o.flag, openFlag.js)),
  mode: Option.getOrUndefined(o.mode),
});

const tempOptions = (o: TempFileOptions) => ({
  directory: Option.getOrUndefined(o.directory),
  prefix: Option.getOrUndefined(o.prefix),
  suffix: o.suffix === undefined ? undefined : Option.getOrUndefined(o.suffix),
});

const bytes = (data: Uint8Array): Bytes => Array.from(data);

/** The operations of `FileSystem.FileSystem`, curried as Flick calls them. */
const impl = (fs: FileSystem.FileSystem) => ({
  access: (o: { ok: boolean; readable: boolean; writable: boolean }) => (path: string) =>
    mapError(fs.access(path, o)),
  copy: (o: { overwrite: boolean; preserveTimestamps: boolean }) => (from: string) => (to: string) =>
    mapError(fs.copy(from, to, o)),
  copy_file: (from: string) => (to: string) => mapError(fs.copyFile(from, to)),
  chmod: (mode: number) => (path: string) => mapError(fs.chmod(path, mode)),
  chown: (uid: number) => (gid: number) => (path: string) => mapError(fs.chown(path, uid, gid)),
  glob: (o: { root: Option.Option<string>; exclude: ReadonlyArray<string> }) => (pattern: string) =>
    mapError(fs.glob(pattern, { root: Option.getOrUndefined(o.root), exclude: o.exclude })),
  exists: (path: string) => mapError(fs.exists(path)),
  link: (from: string) => (to: string) => mapError(fs.link(from, to)),
  make_directory: (o: { recursive: boolean; mode: Option.Option<number> }) => (path: string) =>
    mapError(fs.makeDirectory(path, { recursive: o.recursive, mode: Option.getOrUndefined(o.mode) })),
  make_temp_directory: (o: TempFileOptions) => mapError(fs.makeTempDirectory(tempOptions(o))),
  make_temp_directory_scoped: (o: TempFileOptions) => mapError(fs.makeTempDirectoryScoped(tempOptions(o))),
  make_temp_file: (o: TempFileOptions) => mapError(fs.makeTempFile(tempOptions(o))),
  make_temp_file_scoped: (o: TempFileOptions) => mapError(fs.makeTempFileScoped(tempOptions(o))),
  open: (o: OpenOptions) => (path: string) => mapError(fs.open(path, { ...writeOptions(o), noFollow: o.noFollow })),
  read_directory: (recursive: boolean) => (path: string) => mapError(fs.readDirectory(path, { recursive })),
  read_file: (path: string) => mapError(Effect.map(fs.readFile(path), bytes)),
  read_file_string: (path: string) => mapError(fs.readFileString(path)),
  read_link: (path: string) => mapError(fs.readLink(path)),
  real_path: (path: string) => mapError(fs.realPath(path)),
  remove: (o: { recursive: boolean; force: boolean }) => (path: string) => mapError(fs.remove(path, o)),
  rename: (from: string) => (to: string) => mapError(fs.rename(from, to)),
  stat: (path: string) => mapError(Effect.map(fs.stat(path), info)),
  symlink: (from: string) => (to: string) => mapError(fs.symlink(from, to)),
  truncate: (length: Option.Option<number>) => (path: string) =>
    mapError(fs.truncate(path, Option.getOrUndefined(length))),
  utimes: (atime: number) => (mtime: number) => (path: string) => mapError(fs.utimes(path, new Date(atime), new Date(mtime))),
  write_file: (o: WriteOptions) => (path: string) => (data: Bytes) =>
    mapError(fs.writeFile(path, Uint8Array.from(data), writeOptions(o))),
  write_file_string: (o: WriteOptions) => (path: string) => (data: string) =>
    mapError(fs.writeFileString(path, data, writeOptions(o))),
});

type Impl = ReturnType<typeof impl>;

/** The Effect file system behind each Flick one `node` built, for its streams (`Stream.watch`). */
const natives = new WeakMap<object, FileSystem.FileSystem>();

/** The Effect file system behind a Flick one, if `node` built it. */
export const nativeFileSystem = (flick: object): FileSystem.FileSystem | undefined => natives.get(flick);

/** `FileSystem.node`: the file system of Node.js. */
export const flickNode = <I>(key: Context.Key<I, Impl>): Layer.Layer<I> =>
  Layer.effect(
    key,
    Effect.map(Effect.service(FileSystem.FileSystem), (fs) => {
      const record = impl(fs);
      natives.set(record, fs);
      return record;
    }),
  ).pipe(
    Layer.provide(NodeFileSystem.layer),
  );

export const flickFileStat = (file: FileSystem.File) => mapError(Effect.map(file.stat, info));

export const flickSeek = (offset: bigint, from: Data, file: FileSystem.File) =>
  mapError(file.seek(offset, seekMode.js(from)));

export const flickSync = (file: FileSystem.File) => mapError(file.sync);

export const flickReadAlloc = (size: number, position: Option.Option<bigint>, file: FileSystem.File) =>
  mapError(
    Effect.map(file.readAlloc(size, { position: Option.getOrUndefined(position) }), (read) => Option.map(read, bytes)),
  );

export const flickFileTruncate = (length: Option.Option<number>, file: FileSystem.File) =>
  mapError(file.truncate(Option.getOrUndefined(length)));

export const flickWrite = (data: Bytes, file: FileSystem.File) => mapError(file.write(Uint8Array.from(data)));

export const flickWriteAll = (data: Bytes, file: FileSystem.File) => mapError(file.writeAll(Uint8Array.from(data)));

/** The service key of `FileSystem.FileSystem`. */
const FlickFileSystem = service("FileSystem.FileSystem");

const call = <A>(f: unknown, args: ReadonlyArray<unknown>) =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker typed the operation; it fails with the Flick form of `PlatformError`, which Effect passes on without inspecting it
  applyRuntime(f, args) as Effect.Effect<A, PlatformError.PlatformError>;

/**
 * The Effect `FileSystem` of the Flick service in the context, for the
 * Effect code that reads directories and files and opens files; its
 * failures are the Flick errors the service fails with.
 */
export const effectFileSystem: Effect.Effect<FileSystem.FileSystem, never, ServiceImpl> = Effect.map(
  Effect.service(FlickFileSystem),
  (flick) =>
    FileSystem.makeNoop({
      readDirectory: (path, options) => call(flick["read_directory"], [options?.recursive ?? false, path]),
      readFileString: (path) => call(flick["read_file_string"], [path]),
      open: (path, options) =>
        call(flick["open"], [
          {
            flag: options?.flag === undefined ? Option.none() : Option.some(openFlag.flick(options.flag)),
            mode: Option.fromUndefinedOr(options?.mode),
            noFollow: options?.noFollow ?? false,
          },
          path,
        ]),
    }),
);
