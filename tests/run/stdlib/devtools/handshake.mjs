// A server that keeps the `x-token` header of each WebSocket handshake it
// gets, then drops the connection: enough to see what a client sends.
import { createServer } from "node:http";

let server;
const tokens = [];

export const start = () =>
  new Promise((resolve) => {
    server = createServer();
    server.on("upgrade", (request, socket) => {
      tokens.push(request.headers["x-token"] ?? "-");
      socket.destroy();
    });
    server.listen(0, "127.0.0.1", () => resolve(server.address().port));
  });

/** The token of the first handshake, waiting for one a while. */
export const token = async () => {
  for (let i = 0; i < 100 && tokens.length === 0; i += 1) await new Promise((resolve) => setTimeout(resolve, 20));
  return tokens[0] ?? "none";
};

export const stop = () => new Promise((resolve) => server.close(() => resolve(undefined)));
