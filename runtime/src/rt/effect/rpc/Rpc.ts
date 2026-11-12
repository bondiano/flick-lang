// `Rpc` procedures as data, and their unary calls over HTTP in the wire
// protocol of Effect's RPC server: a POST carries one `Request` message,
// as a line (`ndjson`) or alone (`json`); the answer is the call's messages,
// the last of them its `Exit`. Serving, streams, sockets and the other
// serializations are the adapter's: a program that reaches one takes every
// `Rpc` binding from there, so a procedure is only ever one side's value.
import { tag } from "../../../tags.ts";
import { type Reason, dieReason, failReason, fromReasons, interruptReason, raise } from "../../cause.ts";
import type { Run } from "../../fiber.ts";
import { foreignAsync } from "../../owner.ts";
import type { Codec } from "../../schema.ts";

/** `Rpc.Rpc l f`: a procedure as data. */
export interface Procedure {
  readonly name: string;
  readonly payload: Codec | null;
  readonly success: Codec | null;
  readonly errors: ReadonlyArray<Codec>;
  readonly stream: boolean;
  /** Hooks and the service check only a server runs. */
  readonly middleware: ReadonlyArray<unknown>;
  readonly uninterruptible: boolean;
  readonly auth?: { readonly tag: { readonly key: string }; readonly check: unknown };
}

/** `Rpc.Group hs`. */
export interface Group {
  readonly procedures: ReadonlyArray<Procedure>;
}

/** `Rpc.Serialization`: the content type of a call and whether its messages come as lines. */
export interface Serialization {
  readonly contentType: string;
  readonly lines: boolean;
}

/** `Rpc.Transport`. */
export interface Transport {
  readonly transport: "http";
}

/** `Rpc.CallOptions`. */
export interface CallOptions {
  readonly serialization: Serialization;
  readonly transport: Transport;
}

export const rpcNdjson: Serialization = /*#__PURE__*/ Object.freeze({ contentType: "application/x-ndjson", lines: true });
export const rpcJson: Serialization = /*#__PURE__*/ Object.freeze({ contentType: "application/json", lines: false });
export const rpcHttp: Transport = /*#__PURE__*/ Object.freeze({ transport: "http" });

const with_ = (procedure: Procedure, change: Partial<Procedure>): Procedure => Object.freeze({ ...procedure, ...change });

export const rpcMake = (name: string): Procedure =>
  Object.freeze({ name, payload: null, success: null, errors: [], stream: false, middleware: [], uninterruptible: false });
export const rpcWithPayload = (codec: Codec, procedure: Procedure): Procedure => with_(procedure, { payload: codec });
export const rpcWithSuccess = (codec: Codec, procedure: Procedure): Procedure => with_(procedure, { success: codec, stream: false });
export const rpcWithStream = (codec: Codec, procedure: Procedure): Procedure => with_(procedure, { success: codec, stream: true });
export const rpcErrors: ReadonlyArray<Codec> = /*#__PURE__*/ Object.freeze([]);
export const rpcErrorCase = (codec: Codec, cases: ReadonlyArray<Codec>): ReadonlyArray<Codec> => Object.freeze([...cases, codec]);
export const rpcWithErrors = (cases: ReadonlyArray<Codec>, procedure: Procedure): Procedure =>
  with_(procedure, { errors: [...procedure.errors, ...cases] });
export const rpcWithMiddleware = (hook: unknown, procedure: Procedure): Procedure =>
  with_(procedure, { middleware: [...procedure.middleware, hook] });
export const rpcWithAuth = (service: { readonly key: string }, check: unknown, procedure: Procedure): Procedure =>
  with_(procedure, { auth: { tag: service, check } });
export const rpcUninterruptible = (procedure: Procedure): Procedure => with_(procedure, { uninterruptible: true });
export const rpcGroup: Group = /*#__PURE__*/ Object.freeze({ procedures: [] });
/** `Rpc.add`, after the dictionaries of its record constraints. */
export const rpcAdd = (_lacks: unknown, _union: unknown, procedure: Procedure, group: Group): Group =>
  Object.freeze({ procedures: [...group.procedures, procedure] });
export const rpcAuthenticated = (service: { readonly key: string }, group: Group): Group => {
  const missing = group.procedures.find((p) => p.auth?.tag !== service);
  if (missing !== undefined) throw new TypeError(`the procedure ${missing.name} has no \`with_auth\` making ${service.key}`);
  return group;
};

const CLIENT_ERROR = /*#__PURE__*/ tag("Rpc.ClientError.ClientError");
/** `Rpc.ClientError`, its message as Effect's client words it: the kind of failure first. */
const clientError = (kind: string, message: string): never => raise(fromReasons([failReason({ _tag: CLIENT_ERROR, _0: `${kind}: ${message}` })]));

const hex = (bytes: number): string =>
  Array.from(crypto.getRandomValues(new Uint8Array(bytes)), (b) => b.toString(16).padStart(2, "0")).join("");

let requests = 0;

interface Message {
  readonly _tag: string;
  readonly requestId?: unknown;
  readonly exit?: { readonly _tag: string; readonly value?: unknown; readonly cause?: ReadonlyArray<EncodedReason> };
  readonly defect?: unknown;
}

interface EncodedReason {
  readonly _tag: string;
  readonly error?: unknown;
  readonly defect?: unknown;
  readonly fiberId?: number | null;
}

const isMessage = (value: unknown): value is Message =>
  typeof value === "object" && value !== null && typeof Reflect.get(value, "_tag") === "string";

const decoded = (codec: Codec | null, value: unknown): unknown => {
  if (codec === null) return undefined;
  const result = codec.decode(value);
  if (result._tag === "Failure") throw new TypeError(`the RPC server answered a value its procedure does not decode: ${result.failure._0.map((i) => `${i.path}: ${i.message}`).join("; ")}`);
  return result.success;
};

/** A failure of the server's: the first error case that decodes it, as in the procedure's union. */
const reason = (procedure: Procedure, encoded: EncodedReason): Reason => {
  if (encoded._tag === "Die") return dieReason(encoded.defect);
  if (encoded._tag === "Interrupt") return interruptReason(encoded.fiberId ?? undefined);
  for (const codec of procedure.errors) {
    const result = codec.decode(encoded.error);
    if (result._tag === "Success") return failReason(result.success);
  }
  return dieReason(new TypeError(`the RPC server failed ${procedure.name} with an error it does not declare`));
};

/** The value of the call `id` from the messages of its answer, or its failure raised. */
const settle = (procedure: Procedure, id: number, messages: ReadonlyArray<unknown>): unknown => {
  if (messages.length === 0) return clientError("RpcClientDefect", "Received empty HTTP response from RPC server");
  for (const message of messages) {
    if (!isMessage(message)) return clientError("RpcClientDefect", "Error decoding HTTP response");
    if (message._tag === "Defect") return raise(fromReasons([dieReason(message.defect)]));
    if (message._tag !== "Exit" || Number(message.requestId) !== id || message.exit === undefined) continue;
    if (message.exit._tag === "Success") return decoded(procedure.success, message.exit.value);
    return raise(fromReasons((message.exit.cause ?? []).map((r) => reason(procedure, r))));
  }
  return clientError("RpcClientDefect", "HTTP response ended before RPC request completed");
};

const parse = (serialization: Serialization, text: string): ReadonlyArray<unknown> | undefined => {
  try {
    if (serialization.lines) return text.split("\n").filter((line) => line.trim() !== "").map((line): unknown => JSON.parse(line));
    const all: unknown = JSON.parse(text);
    return Array.isArray(all) ? all : undefined;
  } catch {
    return undefined;
  }
};

/** `Rpc.call_with options url procedure payload`. */
export function* rpcCall(options: CallOptions, url: string, procedure: Procedure, payload: unknown): Run {
  const { serialization } = options;
  const encoded = procedure.payload === null ? undefined : procedure.payload.encode(payload);
  if (encoded?._tag === "Failure") throw new TypeError(`the payload of ${procedure.name} does not encode`);
  const id = requests++;
  // shortcut: a fresh trace for each call, not the caller's span; propagate it once RT spans reach foreign calls
  // a procedure without a payload sends `null`, as Effect's `Void` encodes
  const request = JSON.stringify({
    _tag: "Request", id, tag: procedure.name, payload: encoded === undefined ? null : encoded.success,
    traceId: hex(16), spanId: hex(8), sampled: true, headers: [],
  });
  const body = serialization.lines ? `${request}\n` : request;
  const text = yield foreignAsync(
    (signal) => fetch(url, { method: "POST", headers: { "content-type": serialization.contentType }, body, signal }).then((r) => r.text()),
    (error) => ({ _tag: CLIENT_ERROR, _0: `HttpError: ${error instanceof Error ? error.message : String(error)}` }),
  );
  const messages = parse(serialization, String(text));
  return messages === undefined ? clientError("RpcClientDefect", "Error decoding HTTP response") : settle(procedure, id, messages);
}

const ndjson: CallOptions = /*#__PURE__*/ Object.freeze({ serialization: rpcNdjson, transport: rpcHttp });

/** `Rpc.call url procedure payload`: in NDJSON. */
export const rpcCallNdjson = (url: string, procedure: Procedure, payload: unknown): Run => rpcCall(ndjson, url, procedure, payload);
