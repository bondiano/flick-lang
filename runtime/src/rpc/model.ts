// `Rpc` values: procedures and groups as frozen data, made into Effect's
// `Rpc` and `RpcGroup` when served or called. The codecs are Flick's (Effect
// Schema codecs of the JSON wire form), so they are the RPC schemas as they are.
import { type Layer, Schema, type Scope } from "effect";
import * as Rpc from "effect/rpc/Rpc";
import * as RpcGroup from "effect/rpc/RpcGroup";
import * as RpcSerialization from "effect/rpc/RpcSerialization";

import type { Tagged } from "../data.ts";
import type { AnySchema } from "../schema/schemas.ts";
import { tag } from "../tags.ts";

/** `Rpc.Rpc l f`: a procedure as data, made into Effect's when served or called. */
export interface Procedure {
  readonly name: string;
  readonly payload: AnySchema | null;
  readonly success: AnySchema | null;
  readonly errors: ReadonlyArray<AnySchema>;
  readonly stream: boolean;
}

/** `Rpc.Group hs`. */
export interface Group {
  readonly procedures: ReadonlyArray<Procedure>;
}

/** `Rpc.Serialization`: how messages are written, as the layer of Effect's serialization. */
export type Serialization = Layer.Layer<RpcSerialization.RpcSerialization>;

/** `Rpc.ndjson`: JSON, one message a line, so a stream's elements come as they are sent. */
export const rpcNdjson: Serialization = RpcSerialization.layerNdjson;

/** `Rpc.json`: one JSON document; over HTTP a call's messages come together. */
export const rpcJson: Serialization = RpcSerialization.layerJson;

/** `Rpc.json_rpc`: JSON-RPC 2.0 documents. */
export const rpcJsonRpc: Serialization = /*#__PURE__*/ RpcSerialization.layerJsonRpc();

/** `Rpc.ndjson_rpc`: JSON-RPC 2.0, one message a line. */
export const rpcNdjsonRpc: Serialization = /*#__PURE__*/ RpcSerialization.layerNdJsonRpc();

/** `Rpc.binary`: Effect's schema-driven binary frames. */
export const rpcBinary: Serialization = /*#__PURE__*/ RpcSerialization.layerSchemaBinary();

/** `Rpc.ServeOptions`. */
export interface ServeOptions {
  readonly host: string;
  readonly port: number;
  readonly path: string;
  readonly serialization: Serialization;
}

/** `Rpc.CallOptions`. */
export interface CallOptions {
  readonly serialization: Serialization;
}

/** `Rpc.Server s`. */
export interface Server {
  readonly port: number;
  readonly scope: Scope.Closeable;
}

/** `Rpc.ServeError` and `Rpc.ClientError`. */
export type RpcError = Tagged<string, string>;

export const serveError = (message: string): RpcError => ({ _tag: tag("Rpc.ServeError.ServeError"), _0: message });
export const clientError = (message: string): RpcError => ({ _tag: tag("Rpc.ClientError.ClientError"), _0: message });

const with_ = (procedure: Procedure, change: Partial<Procedure>): Procedure => Object.freeze({ ...procedure, ...change });

/** `Rpc.make label`: a procedure without payload, success or errors. */
export const rpcMake = (name: string): Procedure =>
  Object.freeze({ name, payload: null, success: null, errors: [], stream: false });

export const rpcWithPayload = (codec: AnySchema, procedure: Procedure): Procedure => with_(procedure, { payload: codec });

export const rpcWithSuccess = (codec: AnySchema, procedure: Procedure): Procedure =>
  with_(procedure, { success: codec, stream: false });

export const rpcWithStream = (codec: AnySchema, procedure: Procedure): Procedure =>
  with_(procedure, { success: codec, stream: true });

/** `Rpc.error_case codec`: the codec is the case. */
export const rpcErrorCase = (codec: AnySchema): AnySchema => codec;

export const rpcWithErrors = (cases: ReadonlyArray<AnySchema>, procedure: Procedure): Procedure =>
  with_(procedure, { errors: [...procedure.errors, ...cases] });

/** `Rpc.group`: a group without procedures. */
export const rpcGroup: Group = /*#__PURE__*/ Object.freeze({ procedures: [] });

/** `Rpc.add`, after the dictionaries of its record constraints. */
export const rpcAdd = (_lacks: unknown, _union: unknown, procedure: Procedure, group: Group): Group =>
  Object.freeze({ procedures: [...group.procedures, procedure] });

/** Effect's `Rpc` of `procedure`. */
const effectRpc = (procedure: Procedure) => {
  const [first, ...rest] = procedure.errors;
  const error = first === undefined ? Schema.Never : rest.length === 0 ? first : Schema.Union([first, ...rest]);
  return Rpc.make(procedure.name, {
    payload: procedure.payload ?? Schema.Void,
    success: procedure.success ?? Schema.Void,
    error,
    stream: procedure.stream,
  });
};

export const effectGroup = (procedures: ReadonlyArray<Procedure>) => RpcGroup.make(...procedures.map(effectRpc));

