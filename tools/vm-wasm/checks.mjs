// Cooperative stepping (AC 3), JS host callbacks, wall-time deadlines,
// capability gating, and `.fvm` byte-buffer round-trip checks for `flick-vm-wasm`.

import assert from "node:assert/strict";
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

export function findSingleFvm(distDir) {
  const files = readdirSync(distDir).filter((name) => name.endsWith(".fvm"));
  assert.equal(
    files.length,
    1,
    `expected exactly one .fvm file in ${distDir}, found: ${files.join(", ")}`,
  );
  return path.join(distDir, files[0]);
}

function buildFlickAppFvm(runChecked, flickBin, workDir, entryModule, files, env) {
  const projDir = path.join(workDir, "proj");
  const srcDir = path.join(projDir, "src");
  mkdirSync(srcDir, { recursive: true });
  writeFileSync(
    path.join(projDir, "flick.toml"),
    [
      "schema-version = 1",
      "",
      "[project]",
      'kind = "app"',
      `entry = "${entryModule}"`,
      'source-dir = "src"',
      "",
      "[build]",
      'out-dir = "dist"',
      'profile = "debug"',
      'target = "vm"',
      "",
    ].join("\n"),
  );
  for (const [relPath, contents] of Object.entries(files)) {
    const full = path.join(srcDir, relPath);
    mkdirSync(path.dirname(full), { recursive: true });
    writeFileSync(full, contents);
  }
  runChecked(flickBin, ["build", "--target", "vm", "--out-dir", "dist"], {
    cwd: projDir,
    env,
  });
  return readFileSync(findSingleFvm(path.join(projDir, "dist")));
}

export function verifyCooperativeSteppingAndAbandon(runChecked, vmWasm, flickBin, workDir, env) {
  const fvmBytes = buildFlickAppFvm(
    runChecked,
    flickBin,
    path.join(workDir, "coop"),
    "App.Coop",
    {
      "App/Coop.fl": [
        "module App.Coop exposing (main, infinite_with_finalizers, after_abandon)",
        "",
        "infinite_with_finalizers : Unit ->{Host.Console} Int",
        "let infinite_with_finalizers () = scope {",
        '  Scope.add_finalizer (task { Platform.stdout_line "fin-1" })',
        '  Scope.add_finalizer (task { Platform.stdout_line "fin-2" })',
        '  Platform.stdout_line "entered"',
        "  let mut n = 0",
        "  loop {",
        "    n = n + 1",
        "    if n < 0 then break n",
        "  }",
        "}",
        "",
        "after_abandon : Unit ->{Host.Console} Int",
        "let after_abandon () =",
        '  Platform.stdout_line "still alive"',
        "  42",
        "",
        "main : Unit ->{Host.Console} Unit",
        "let main () =",
        "  let _ = (after_abandon, infinite_with_finalizers)",
        "  ()",
        "",
      ].join("\n"),
    },
    env,
  );

  const liveChunks = [];
  const session = new vmWasm.VmSession(
    JSON.stringify({
      preset: "untrusted",
      fuel: 1_000_000,
      wallTimeMs: 5_000,
      heapBytes: 16 * 1024 * 1024,
      maxStackDepth: 512,
    }),
  );
  session.set_host_callbacks({
    writeStdout(chunk) {
      liveChunks.push(chunk);
    },
  });

  const loaded = JSON.parse(session.load_image(fvmBytes));
  assert.ok(loaded.loadedModules.includes("App.Coop"));

  // Start the infinite loop cooperatively.
  const started = JSON.parse(session.start_eval("App.Coop", "infinite_with_finalizers", true));
  assert.equal(started.ok, true, `start_eval failed: ${JSON.stringify(started)}`);
  assert.equal(session.has_active_eval(), true);

  // Step 5 bounded fuel slices and verify control returns to JS every time.
  for (let i = 0; i < 5; i++) {
    const step = JSON.parse(session.step(500));
    assert.equal(step.status, "yielded", `slice ${i} should yield, got ${JSON.stringify(step)}`);
    assert.ok(step.fuelUsed > 0, `slice ${i} should consume fuel`);
    assert.equal(session.has_active_eval(), true);
  }
  assert.ok(
    (session.active_stdout() ?? "").includes("entered\n"),
    `expected buffered stdout to contain 'entered', got ${JSON.stringify(session.active_stdout())}`,
  );
  assert.ok(
    liveChunks.join("").includes("entered\n"),
    `expected JS writeStdout callback to receive 'entered', got ${JSON.stringify(liveChunks)}`,
  );

  // Abandon the infinite loop: LIFO finalizers must run cleanly.
  const abandoned = JSON.parse(session.abandon());
  assert.equal(abandoned.kind, "interrupted");
  assert.equal(abandoned.stdout, "entered\nfin-2\nfin-1\n");
  assert.equal(session.has_active_eval(), false);

  // Run and abandon a second time (now that persistent `Host.live` and global
  // closures are already initialized) and verify zero heap growth.
  const heapAfterFirst = session.heap_live_bytes();
  const startedAgain = JSON.parse(
    session.start_eval("App.Coop", "infinite_with_finalizers", true),
  );
  assert.equal(startedAgain.ok, true);
  const stepAgain = JSON.parse(session.step(500));
  assert.equal(stepAgain.status, "yielded");
  const abandonedAgain = JSON.parse(session.abandon());
  assert.equal(abandonedAgain.kind, "interrupted");
  assert.equal(abandonedAgain.stdout, "entered\nfin-2\nfin-1\n");
  const heapAfterSecond = session.heap_live_bytes();
  assert.equal(
    heapAfterSecond,
    heapAfterFirst,
    `abandon() leaked heap bytes: afterFirst=${heapAfterFirst}, afterSecond=${heapAfterSecond}`,
  );

  // The same session must continue working cleanly for subsequent evaluations.
  const started2 = JSON.parse(session.start_eval("App.Coop", "after_abandon", true));
  assert.equal(started2.ok, true);
  let outcome = null;
  for (let i = 0; i < 100; i++) {
    const step = JSON.parse(session.step(5_000));
    if (step.status !== "yielded") {
      outcome = step;
      break;
    }
  }
  assert.ok(outcome !== null, "after_abandon must complete");
  assert.equal(outcome.status, "completed");
  assert.equal(outcome.outcome.kind, "value");
  assert.equal(outcome.outcome.rendered, "42");
  assert.equal(outcome.outcome.stdout, "still alive\n");

  session.shutdown();
  session.free();
  console.log(
    "vm-wasm cooperative stepping (AC 3): 5 yielded slices -> abandon() ran LIFO finalizers -> zero leaks -> follow-up eval returned 42",
  );
}

export function verifyHostCallbacksLimitsAndImages(runChecked, vmWasm, flickBin, workDir, env) {
  const fvmBytes = buildFlickAppFvm(
    runChecked,
    flickBin,
    path.join(workDir, "host"),
    "App.HostDemo",
    {
      "App/HostDemo.fl": [
        "module App.HostDemo exposing (main, host_probe, saved_answer, spin_forever)",
        "",
        "host_probe : Unit ->{Host.Clock, Host.Random, Host.Environment} String",
        "let host_probe () =",
        "  let now = Clock.current_time_millis ()",
        "  let draw = Random.next ()",
        '  let greeting = Option.with_default "missing" (Platform.env "GREETING")',
        '  show now ++ "|" ++ show draw ++ "|" ++ greeting',
        "",
        "saved_answer : Int",
        "let saved_answer = 21 + 21",
        "",
        "spin_forever : Unit ->{} Int",
        "let spin_forever () =",
        "  let mut n = 0",
        "  loop {",
        "    n = n + 1",
        "    if n < 0 then break n",
        "  }",
        "",
        "main : Unit ->{Host.Clock, Host.Random, Host.Environment} Unit",
        "let main () =",
        "  let _ = (saved_answer, spin_forever, host_probe)",
        "  ()",
        "",
      ].join("\n"),
    },
    env,
  );

  // 1. Default untrusted session denies clock/random/env and net fetch.
  const deniedSession = new vmWasm.VmSession(undefined);
  assert.throws(
    () => deniedSession.load_image(fvmBytes),
    /capability/i,
    "untrusted session must deny loading program requiring clock/random/env",
  );
  assert.throws(
    () => deniedSession.host_fetch(JSON.stringify({ url: "https://example.test/data" })),
    /capability/i,
    "untrusted session must deny host_fetch without net capability",
  );
  deniedSession.shutdown();
  deniedSession.free();

  // 2. Granted capabilities invoke JS host callbacks for clock, random, env, fetch.
  const hostSession = new vmWasm.VmSession(
    JSON.stringify({
      preset: "untrusted",
      capabilities: ["clock", "random", "env", "net"],
      hostMode: "live",
      fuel: 1_000_000,
      wallTimeMs: 5_000,
    }),
  );
  hostSession.set_host_callbacks({
    currentTimeMillis() {
      return 1_700_000_000_123;
    },
    randomNext() {
      return 0.25;
    },
    envVar(key) {
      return key === "GREETING" ? "hi-from-js" : undefined;
    },
    fetch(req) {
      return {
        status: 200,
        headers: [["content-type", "text/plain"]],
        bodyText: `${req.method}:${req.url}:ok`,
      };
    },
  });
  hostSession.load_image(fvmBytes);

  const probeStart = JSON.parse(hostSession.start_eval("App.HostDemo", "host_probe", true));
  assert.equal(probeStart.ok, true);
  let probeStep = JSON.parse(hostSession.step(100_000));
  while (probeStep.status === "yielded") {
    probeStep = JSON.parse(hostSession.step(100_000));
  }
  assert.equal(probeStep.status, "completed");
  assert.equal(probeStep.outcome.kind, "value");
  assert.ok(
    probeStep.outcome.rendered.includes("1700000000123") &&
      probeStep.outcome.rendered.includes("0.25") &&
      probeStep.outcome.rendered.includes("hi-from-js"),
    `expected JS host callback values in outcome, got: ${probeStep.outcome.rendered}`,
  );

  // Verify host_fetch routes through JS callback when `net` capability is granted.
  const fetchResp = JSON.parse(
    hostSession.host_fetch(
      JSON.stringify({ url: "https://example.test/ping", method: "POST", bodyText: "hi" }),
    ),
  );
  assert.equal(fetchResp.status, 200);
  assert.equal(fetchResp.bodyText, "POST:https://example.test/ping:ok");

  // 3. Evaluate `saved_answer`, save session to Uint8Array .fvm, and reload in a fresh session.
  const ansStart = JSON.parse(hostSession.start_eval("App.HostDemo", "saved_answer", false));
  assert.equal(ansStart.ok, true);
  let ansStep = JSON.parse(hostSession.step(100_000));
  while (ansStep.status === "yielded") {
    ansStep = JSON.parse(hostSession.step(100_000));
  }
  assert.equal(ansStep.status, "completed");
  assert.equal(ansStep.outcome.rendered, "42");

  const savedBytes = hostSession.save_image("App.HostDemo", "main");
  assert.ok(savedBytes instanceof Uint8Array, "save_image must return Uint8Array");
  assert.equal(Buffer.from(savedBytes.slice(0, 4)).toString("ascii"), "FLKM");

  const restoredSession = new vmWasm.VmSession(
    JSON.stringify({
      preset: "untrusted",
      capabilities: ["clock", "random", "env"],
      fuel: 1_000_000,
    }),
  );
  const restoredReport = JSON.parse(restoredSession.load_image(savedBytes));
  assert.ok(restoredReport.restoredValues >= 1, "reloaded image should restore initialized global");
  const restoredStart = JSON.parse(
    restoredSession.start_eval("App.HostDemo", "saved_answer", false),
  );
  assert.equal(restoredStart.ok, true);
  const restoredStep = JSON.parse(restoredSession.step(10_000));
  assert.equal(restoredStep.status, "completed");
  assert.equal(restoredStep.outcome.rendered, "42");
  restoredSession.shutdown();
  restoredSession.free();

  // 4. Verify wallTimeMs deadline backed by JS performance.now() terminates an infinite loop.
  hostSession.set_config(
    JSON.stringify({
      preset: "untrusted",
      capabilities: ["clock", "random", "env"],
      fuel: 1_000_000_000,
      wallTimeMs: 25,
    }),
  );
  const spinStart = JSON.parse(hostSession.start_eval("App.HostDemo", "spin_forever", true));
  assert.equal(spinStart.ok, true);
  let spinOutcome = null;
  for (let i = 0; i < 500; i++) {
    const step = JSON.parse(hostSession.step(100_000));
    if (step.status !== "yielded") {
      spinOutcome = step;
      break;
    }
  }
  assert.ok(spinOutcome !== null, "wallTimeMs deadline must fire on infinite loop");
  assert.equal(spinOutcome.status, "limitExceeded");
  assert.equal(spinOutcome.limit, "wall_time");

  hostSession.shutdown();
  hostSession.free();
  console.log(
    "vm-wasm host callbacks, wall_time (performance.now), capability gating, and .fvm byte-buffer round-trip: passed",
  );
}
