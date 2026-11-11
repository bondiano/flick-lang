import assert from "node:assert/strict";
import { test } from "node:test";
import { parseLinuxRss, readRss, sampleRss } from "./rss.mjs";

test("Linux distinguishes current RSS from the lifetime peak", () => {
  assert.deepEqual(parseLinuxRss("VmRSS: 2048 kB\nVmHWM: 4096 kB\n"), { current: 2, peak: 4 });
  assert.ok(Number.isNaN(parseLinuxRss("").current));
});

test("the current process has a measured resident set", () => {
  assert.ok(readRss(process.pid).current > 0);
  const sampler = sampleRss(process.pid);
  assert.ok(sampler.stop() > 0);
  assert.throws(() => readRss("1;echo bad"), /process id/);
});
