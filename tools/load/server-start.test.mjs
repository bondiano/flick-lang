import assert from "node:assert/strict";
import { test } from "node:test";
import { createServer } from "node:net";
import { startServer } from "./server-start.mjs";

// The budget of startup, which includes starting the child: only the test of
// the timeout itself gets a short one, which a slow start could outlast.
const options = async (timeoutMs) => {
  const server = createServer();
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const port = server.address().port;
  await new Promise((resolve) => server.close(resolve));
  return { url: `http://127.0.0.1:${port}/`, timeoutMs };
};

test("startup reports a child exit instead of polling another process", async () => {
  await assert.rejects(startServer([process.execPath, "-e", "process.exit(7)"], await options()), /exited.*7/);
});

test("startup reports an executable that could not spawn", async () => {
  await assert.rejects(startServer(["/missing-flick-benchmark-executable"], await options()), /ENOENT/);
});

test("startup times out and reaps a process that never listens", async () => {
  await assert.rejects(startServer([process.execPath, "-e", "setInterval(()=>{},1000)"], await options(200)), /abort|timeout/i);
});
