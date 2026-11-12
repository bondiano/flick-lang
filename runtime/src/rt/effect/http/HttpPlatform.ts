// `Http.Platform` on Flick's own runtime: `node_of` is the platform of
// Node.js as Effect's `NodeHttpPlatform` is, with strong entity tags. A file
// response is a `Raw` body of a Node readable opened only when it is sent,
// with the file's content type, length, `ETag` and `Last-Modified`.
import type { BigIntStats } from "node:fs";
import type { Readable } from "node:stream";

import type { Option } from "../../../core/data.ts";
import { tag } from "../../../tags.ts";
import { fail, raise } from "../../cause.ts";
import type { Run } from "../../fiber.ts";
import { bodyRaw, makeResponse } from "../../http/model.ts";
import { nodeFs, nodeStream } from "../../http/write.ts";
import { type Layer, layerSucceed } from "../../layer.ts";
import { foreignAsync } from "../../owner.ts";
import { fromInput, set } from "./Headers.ts";
import { type FlickOptions, options as responseOptions } from "./HttpServerResponse.ts";
import { getType } from "./Mime.ts";

/** `Http.Platform.FileOptions`. */
interface FileOptions {
  readonly bytesToRead: Option<bigint>;
  readonly chunkSize: Option<number>;
  readonly offset: Option<bigint>;
  readonly response: FlickOptions;
}

const text = (value: string | undefined) => (value === undefined ? { _tag: "None" as const } : { _tag: "Some" as const, value });

const SYSTEM_TAGS: Readonly<Record<string, string>> = {
  ENOENT: "NotFound",
  EACCES: "PermissionDenied",
  EPERM: "PermissionDenied",
  EEXIST: "AlreadyExists",
  EISDIR: "BadResource",
  ENOTDIR: "BadResource",
  EBUSY: "Busy",
  ELOOP: "BadResource",
};

/** The Flick `PlatformError` of a failed `stat`, as Effect's Node file system reports it. */
const statError = (path: string) => (error: unknown) => {
  const code: unknown = Reflect.get(Object(error), "code");
  const syscall: unknown = Reflect.get(Object(error), "syscall");
  return {
    _tag: tag("PlatformError.PlatformError.PlatformError"),
    _0: {
      _tag: tag("PlatformError.Reason.System"),
      _0: {
        tag: { _tag: tag(`PlatformError.SystemErrorTag.${(typeof code === "string" ? SYSTEM_TAGS[code] : undefined) ?? "UnknownError"}`) },
        module_: "FileSystem",
        method: "stat",
        description: text(undefined),
        syscall: text(typeof syscall === "string" ? syscall : undefined),
        pathOrDescriptor: text(path),
      },
    },
  };
};

const badArgument = (description: string): never =>
  raise(
    fail({
      _tag: tag("PlatformError.PlatformError.PlatformError"),
      _0: { _tag: tag("PlatformError.Reason.Bad"), _0: { module_: "HttpPlatform", method: "fileResponse", description: text(description) } },
    }),
  );

const safe = (value: bigint, field: string): number => {
  const number = Number(value);
  return Number.isSafeInteger(number) ? number : badArgument(`${field} exceeds the safe integer range: ${value}`);
};

/** A readable that opens its source only when read, so a response never sent opens nothing. */
const lazyReadable = (evaluate: () => Readable): Readable => {
  const { Readable } = nodeStream();
  let source: Readable | undefined;
  return new Readable({
    read() {
      if (source === undefined) {
        try {
          source = evaluate();
        } catch (cause) {
          this.destroy(cause instanceof Error ? cause : new Error(String(cause)));
          return;
        }
        const opened = source;
        opened.on("data", (chunk) => {
          if (!this.push(chunk)) opened.pause();
        });
        opened.once("end", () => this.push(null));
        opened.once("error", (cause) => this.destroy(cause));
        opened.once("close", () => {
          if (!opened.readableEnded) this.destroy(new Error("Response stream closed prematurely"));
        });
      }
      source.resume();
    },
    destroy(error, callback) {
      source?.destroy(error ?? undefined);
      callback(error);
    },
  });
};

function* fileResponse(o: FileOptions, path: string): Run {
  const fs = nodeFs();
  const read: { stats?: BigIntStats } = {};
  yield foreignAsync(async () => {
    read.stats = await fs.promises.stat(path, { bigint: true });
  }, statError(path));
  const stats = read.stats;
  if (stats === undefined) throw new TypeError("stat gave no information");
  const offsetWanted = o.offset._tag === "Some" ? o.offset.value : 0n;
  const offset = offsetWanted > stats.size ? stats.size : offsetWanted;
  const available = stats.size - offset;
  const bytesToRead = o.bytesToRead._tag === "Some" ? o.bytesToRead.value : undefined;
  const contentLength = bytesToRead === undefined || bytesToRead > available ? available : bytesToRead;
  const start = safe(offset, "offset");
  const end = bytesToRead === undefined ? undefined : safe(offset + contentLength, "end");
  const given = responseOptions(o.response);
  let headers = given.headers ? fromInput(given.headers) : fromInput([]);
  if (given.contentType) headers = set("content-type", given.contentType)(headers);
  // a strong tag of the size and modification time, in hexadecimal
  headers = set("etag", `"${stats.size.toString(16)}-${stats.mtime.getTime().toString(16)}"`)(headers);
  headers = set("last-modified", stats.mtime.toUTCString())(headers);
  const mime = getType(path);
  const contentType = headers["content-type"] ?? (mime._tag === "Some" ? mime.value : "application/octet-stream");
  const stream = lazyReadable(() =>
    contentLength === 0n ? nodeStream().Readable.from([]) : fs.createReadStream(path, { start, end: end === undefined ? undefined : end - 1 }));
  return makeResponse({
    status: given.status ?? 200,
    statusText: given.statusText,
    headers: set("content-length", contentLength.toString())(set("content-type", contentType)(headers)),
    body: bodyRaw(stream),
  });
}

/** `Http.Platform.node_of service`: the platform of Node.js as the service `HttpPlatform`. */
export const flickNode = (service: object): Layer =>
  layerSucceed(service, { file_response: (o: FileOptions) => (path: string) => fileResponse(o, path) });
