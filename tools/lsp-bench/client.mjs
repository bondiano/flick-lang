// A language server over stdio for the scripts here: framed messages out,
// replies and notifications in, matched by the caller.
import { spawn } from "node:child_process";

export const connect = (binary) => {
  const server = spawn(binary, ["lsp", "--stdio"], { stdio: ["pipe", "pipe", "ignore"] });
  let buffer = Buffer.alloc(0);
  const waiting = [];
  server.stdout.on("data", (chunk) => {
    buffer = Buffer.concat([buffer, chunk]);
    for (;;) {
      const end = buffer.indexOf("\r\n\r\n");
      if (end < 0) return;
      const length = Number(/content-length: *(\d+)/i.exec(buffer.subarray(0, end).toString())[1]);
      if (buffer.length < end + 4 + length) return;
      const message = JSON.parse(buffer.subarray(end + 4, end + 4 + length).toString());
      buffer = buffer.subarray(end + 4 + length);
      const i = waiting.findIndex((w) => w.matches(message));
      if (i >= 0) waiting.splice(i, 1)[0].resolve(message);
    }
  });
  const send = (message) => {
    const body = JSON.stringify({ jsonrpc: "2.0", ...message });
    server.stdin.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
  };
  const expect = (matches) => new Promise((resolve) => waiting.push({ matches, resolve }));
  let id = 0;
  const request = (method, params) => {
    const current = ++id;
    const reply = expect((m) => m.id === current);
    send({ id: current, method, params });
    return reply;
  };
  const diagnostics = (uri) => expect((m) => m.method === "textDocument/publishDiagnostics" && m.params.uri === uri);
  return { server, send, expect, request, diagnostics };
};
