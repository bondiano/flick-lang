// `Server.handler`: the implementations of server functions served as one
// RPC group, a POST per call in NDJSON, on a route of `Http.Router`.
import { rpcNdjson } from "../../rpc/model.ts";
import { rpcHandler } from "../../rpc/server.ts";

import type { ServerFunction } from "./Functions.ts";

/** `Server.handler functions`, owned by the innermost scope. */
export const serverHandler = (functions: ReadonlyArray<ServerFunction>) =>
  rpcHandler(
    rpcNdjson,
    { procedures: functions.map((f) => f.procedure) },
    Object.fromEntries(functions.map((f) => [f.procedure.name, f.run])),
  );
