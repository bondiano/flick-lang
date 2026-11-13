// Reading the body of a `ServerRequest`, as Effect's `HttpIncomingMessage`
// reads it: the bytes once, at most the `MaxBodySize` of the task that
// reads first, and a failure to read or parse them a `RequestParseError`.
import { fail, raise } from "../cause.ts";
import { type Run, Under, currentCtx, extend, lookup } from "../fiber.ts";
import { foreignAsync } from "../owner.ts";
import { type ServerRequest, httpServerError } from "./model.ts";

// the key of Effect's reference, so a limit set on either side reads the same
const MAX_BODY_SIZE = /*#__PURE__*/ Object.freeze({ key: "effect/http/HttpIncomingMessage/MaxBodySize" });

/** Runs `task` with the largest body a request it reads may have. */
export const withMaxBodySize = (size: bigint, task: () => unknown): Under =>
  new Under(extend(currentCtx(), MAX_BODY_SIZE, size), task);

const maxBodySize = (): number | undefined => {
  const size = lookup(MAX_BODY_SIZE);
  return typeof size === "bigint" ? Number(size) : undefined;
};

const parseError = (request: ServerRequest) => (cause: unknown) => httpServerError("RequestParseError", request, cause);

/** The bytes of the body. */
export const readBytes = (request: ServerRequest): unknown => {
  const limit = maxBodySize();
  return foreignAsync((signal) => request.source.read(limit, signal), parseError(request));
};

const textDecoder = /*#__PURE__*/ new TextDecoder();
const formDecoder = /*#__PURE__*/ new TextDecoder("utf-8", { ignoreBOM: true });

const decoded = (bytes: unknown): Uint8Array => {
  if (bytes instanceof Uint8Array) return bytes;
  throw new TypeError("a request body is no bytes");
};

export function* readText(request: ServerRequest): Run {
  return textDecoder.decode(decoded(yield readBytes(request)));
}

/** The body as JSON: `null` when it is empty. */
export function* readJson(request: ServerRequest): Run {
  const text = String(yield readText(request));
  try {
    const json: unknown = text === "" ? null : JSON.parse(text);
    return json;
  } catch (cause) {
    return raise(fail(parseError(request)(cause)));
  }
}

type Pairs = ReadonlyArray<readonly [string, string]>;

/** The body as form parameters, given to `then`; a leading `?` is data, not a query's mark. */
export function* readUrlParams(request: ServerRequest, then: (params: Pairs) => unknown = (params) => params): Run {
  const text = formDecoder.decode(decoded(yield readBytes(request)));
  let params: Pairs;
  try {
    params = [...new URLSearchParams(text.replace(/^\?/, "%3F"))];
  } catch (cause) {
    return raise(fail(parseError(request)(cause)));
  }
  return then(params);
}
