// A second session for the editor features: the compiler worker already owns
// the check and the emit, and a decoration must not queue behind them.
import init, { Session } from "./wasm/flick_wasm.js";

import type { Request, Reply, Result } from "./ide-client.ts";

let session: Session | undefined;
let ready: Promise<unknown> | undefined;
let text = "";

const load = async (): Promise<Session> => {
  ready ??= init();
  await ready;
  session ??= new Session();
  return session;
};

onmessage = async (event: MessageEvent<Request>) => {
  const request = event.data;
  try {
    const current = await load();
    // `update` does nothing when the text has not changed; its cache survives
    if (request.source !== text) {
      current.update(request.source);
      text = request.source;
    }
    let result: Result;
    switch (request.action) {
      case "tokens":
        result = JSON.parse(current.semantic_tokens()) as Result;
        break;
      case "hints":
        result = JSON.parse(current.inlay_hints()) as Result;
        break;
      case "signature":
        result = JSON.parse(current.signature_help(request.at)) as Result;
        break;
      case "actions":
        result = JSON.parse(current.code_actions(request.from, request.to)) as Result;
        break;
    }
    const reply: Reply = { id: request.id, result };
    postMessage(reply);
  } catch (error) {
    const reply: Reply = { id: request.id, error: error instanceof Error ? error.message : String(error) };
    postMessage(reply);
  }
};
