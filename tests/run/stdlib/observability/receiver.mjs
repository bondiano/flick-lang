// A local OTLP collector for the observability run test: it keeps the body
// of every request it gets, by path.
import { createServer } from "node:http";

let server;
const bodies = [];

/** Starts the collector on a free port and gives the port. */
export const start = () =>
  new Promise((resolve) => {
    server = createServer((request, response) => {
      let body = "";
      request.on("data", (chunk) => (body += chunk));
      request.on("end", () => {
        bodies.push(`${request.url} ${body}`);
        response.writeHead(200, { "content-type": "application/json" }).end("{}");
      });
    });
    server.listen(0, "127.0.0.1", () => resolve(server.address().port));
  });

/** Everything the collector got, one request per line. */
export const received = () => bodies.join("\n");

export const stop = () => new Promise((resolve) => server.close(() => resolve(undefined)));
