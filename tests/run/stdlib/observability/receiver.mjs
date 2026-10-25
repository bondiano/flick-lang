// A local OTLP collector for the observability run tests: it keeps the
// body of every request it gets, by path, ungzipped, and what its headers
// say of it.
import { createServer } from "node:http";
import { gunzipSync } from "node:zlib";

let server;
const bodies = [];
const metas = [];

/** Starts the collector on a free port and gives the port. */
export const start = () =>
  new Promise((resolve) => {
    server = createServer((request, response) => {
      const chunks = [];
      request.on("data", (chunk) => chunks.push(chunk));
      request.on("end", () => {
        const raw = Buffer.concat(chunks);
        const encoding = request.headers["content-encoding"] ?? "identity";
        const body = (encoding === "gzip" ? gunzipSync(raw) : raw).toString("latin1");
        bodies.push(`${request.url} ${body}`);
        metas.push(`${request.url} ${request.headers["content-type"]} ${encoding} ${request.headers["x-api-key"] ?? "-"}`);
        response.writeHead(200, { "content-type": "application/json" }).end("{}");
      });
    });
    server.listen(0, "127.0.0.1", () => resolve(server.address().port));
  });

/** Everything the collector got, one request per line. */
export const received = () => bodies.join("\n");

/** Each request's path, content type, content encoding and `x-api-key`, one a line. */
export const meta = () => metas.join("\n");

export const stop = () => new Promise((resolve) => server.close(() => resolve(undefined)));
