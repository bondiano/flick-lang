import assert from "node:assert/strict";
import { createServer } from "node:http";
import { test } from "node:test";
import { brotliCompressSync, deflateSync, gzipSync } from "node:zlib";
import { HttpClient } from "./client.mjs";

const fixture = async (t, handle) => {
  const client = new HttpClient();
  const server = createServer(handle);
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(async () => {
    client.close();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  });
  return { client, url: `http://127.0.0.1:${server.address().port}` };
};

test("direct GETs retain fetch's headers and read a complete UTF-8 body across chunks", async (t) => {
  const sockets = new Set();
  const { client, url } = await fixture(t, (request, response) => {
    sockets.add(request.socket);
    assert.equal(request.method, "GET");
    assert.equal(request.headers.connection, "keep-alive");
    assert.equal(request.headers.accept, "*/*");
    assert.equal(request.headers["accept-language"], "*");
    assert.equal(request.headers["sec-fetch-mode"], "cors");
    assert.equal(request.headers["user-agent"], "node");
    assert.equal(request.headers["accept-encoding"], "gzip, deflate");
    const bytes = Buffer.from("\uFEFFitem привет 🌍");
    response.write(bytes.subarray(0, 9));
    setImmediate(() => response.end(bytes.subarray(9)));
  });
  assert.deepEqual(await client.get(`${url}/item/r1`), { status: 200, body: "item привет 🌍" });
  assert.deepEqual(await client.get(`${url}/item/r2`), { status: 200, body: "item привет 🌍" });
  assert.equal(sockets.size, 1, "completed requests reuse their keep-alive connection");
});

test("compressed bodies are decoded before validation", async (t) => {
  const encode = { gzip: gzipSync, deflate: deflateSync, br: brotliCompressSync };
  const { client, url } = await fixture(t, (request, response) => {
    const coding = request.url.slice(1);
    response.setHeader("Content-Encoding", coding);
    response.end(encode[coding](Buffer.from("item русский 🌍")));
  });
  for (const coding of Object.keys(encode)) {
    assert.deepEqual(await client.get(`${url}/${coding}`), { status: 200, body: "item русский 🌍" });
  }
});

test("redirects remain status responses and are not hidden extra requests", async (t) => {
  let requests = 0;
  const { client, url } = await fixture(t, (_request, response) => {
    requests++;
    response.writeHead(302, { location: "/elsewhere" });
    response.end("moved");
  });
  assert.deepEqual(await client.get(url), { status: 302, body: "moved" });
  assert.equal(requests, 1);
});

test("abort keeps its timeout reason while the body is still pending", async (t) => {
  let received;
  const started = new Promise((resolve) => { received = resolve; });
  const { client, url } = await fixture(t, (_request, response) => {
    response.write("incomplete");
    received();
  });
  const controller = new AbortController();
  const pending = client.get(url, { signal: controller.signal });
  await started;
  const reason = new DOMException("load timed out", "TimeoutError");
  controller.abort(reason);
  await assert.rejects(pending, (error) => error === reason);
});

test("a truncated response fails instead of accepting a partial body", async (t) => {
  const { client, url } = await fixture(t, (_request, response) => {
    response.writeHead(200, { "content-length": "100" });
    response.write("part");
    setImmediate(() => response.socket.destroy());
  });
  await assert.rejects(client.get(url));
});
