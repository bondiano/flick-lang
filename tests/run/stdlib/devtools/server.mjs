// A DevTools server on a free local port for the devtools run test.
import { devToolsListen } from "@flick-lang/runtime/effect/devtools/DevToolsServer";

let server;

export const start = async () => {
  server = await devToolsListen();
  return server.port;
};

/** Whether the server got a span named `name`, waiting for it a while. */
export const got = async (name) => {
  for (let i = 0; i < 100 && !server.spans.includes(name); i += 1) await new Promise((resolve) => setTimeout(resolve, 20));
  return server.spans.includes(name);
};

export const stop = async () => {
  await server.close();
  return undefined;
};
