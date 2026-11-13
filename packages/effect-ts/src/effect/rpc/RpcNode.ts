// Flick's RPC servers on Node: a separate entry point, so a program that only
// calls procedures, in a browser say, loads none of them.
export * from "../../rpc/server.ts";

export { rpcServe } from "../../rpc/handler.ts";
