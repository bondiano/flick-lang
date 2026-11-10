import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createServer } from "node:http";
import { test } from "node:test";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

const exec = promisify(execFile);
const script = fileURLToPath(new URL("./index.mjs", import.meta.url));

test("the direct HTTP load keeps ids unique across workers and checks every body marker", async () => {
  const ids = new Set();
  const server = createServer((request, response) => {
    const id = request.url.slice("/item/".length);
    assert.ok(!ids.has(id), `repeated id ${id}`);
    ids.add(id);
    response.end(`item ${id}; finished ${id}`);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  try {
    const { stdout } = await exec(process.execPath, [script,
      "--url", `http://127.0.0.1:${server.address().port}/item/{id}`,
      "--client", "http", "--expect", "item {id}|finished {id}",
      "--workers", "2", "--levels", "1,4", "--warmup", "0",
      "--run", "0.05", "--runs", "1", "--soak", "0.02", "--idle", "0",
    ]);
    assert.match(stdout, /client http/);
    assert.match(stdout, /errors: \{"status":0,"body":0,"transport":0,"timeout":0\}/);
    assert.ok(ids.size > 0);
  } finally {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
});

test("the fetch default and direct HTTP client classify failed responses identically", async () => {
  for (const client of [undefined, "http"]) {
    for (const failure of ["status", "body", "transport", "timeout"]) {
      const server = createServer((request, response) => {
        const id = request.url.slice("/item/".length);
        if (failure === "transport") return request.socket.destroy();
        if (failure === "timeout") { response.write("pending"); return; }
        response.statusCode = failure === "status" ? 503 : 200;
        // One marker is right: checking only the first must still fail.
        response.end(`item ${id}`);
      });
      await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
      try {
        const args = [script, "--url", `http://127.0.0.1:${server.address().port}/item/{id}`,
          "--expect", "item {id}|finished {id}", "--levels", "1", "--warmup", "0",
          "--run", "0.01", "--runs", "1", "--soak", "0.01", "--idle", "0", "--timeout", failure === "timeout" ? "0.02" : "5"];
        if (client) args.push("--client", client);
        await assert.rejects(exec(process.execPath, args), (error) => {
          assert.equal(error.code, 1, error.stderr);
          assert.match(error.stdout, new RegExp(`client ${client ?? "fetch"}`));
          const totals = JSON.parse(/errors: (.+)/.exec(error.stdout)[1]);
          assert.ok(totals[failure] > 0, error.stdout);
          for (const [kind, count] of Object.entries(totals)) if (kind !== failure) assert.equal(count, 0, error.stdout);
          return true;
        });
      } finally {
        server.closeAllConnections();
        await new Promise((resolve) => server.close(resolve));
      }
    }
  }
});

test("an unknown client is a usage error before loading the server", async () => {
  await assert.rejects(exec(process.execPath, [script, "--url", "http://127.0.0.1/item/{id}", "--client", "unknown"]), (error) => {
    assert.equal(error.code, 2);
    assert.match(error.stderr, /--client must be fetch or http/);
    return true;
  });
});
