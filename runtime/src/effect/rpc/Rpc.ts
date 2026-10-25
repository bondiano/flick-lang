// Effect's `effect/rpc` for the standard library, and Flick's typed RPCs:
// procedures and groups as data, called with a client of the same
// procedures. Serving them is `./RpcNode.ts`, which needs Node.
export * from "effect/rpc/Rpc";
export * from "../../rpc/model.ts";
export * from "../../rpc/client.ts";
export * from "../../rpc/local.ts";
