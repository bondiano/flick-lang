// One module of the site does what its page says, with the real compiler: the gallery's examples and the tour's lessons.
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { cpSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
export const repo = path.join(here, "../../..");
export const flick = process.env.FLICK_BIN ?? path.join(repo, "target/debug/flick");

/** The distinct codes of the errors in `flick check --diagnostics json` output. */
export const errorCodes = (stderr: string): ReadonlyArray<string> => [
  ...new Set(
    stderr
      .split("\n")
      .filter((line) => line.startsWith("{"))
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiler writes one JSON event a line
      .map((line) => JSON.parse(line) as { code: string; severity: string })
      .filter((event) => event.severity === "error")
      .map((event) => event.code),
  ),
];

/**
 * Checks `file` as `Main.fl` in `target/tmp/<name>`: `fail CODE` reports
 * exactly that code; `run` and `ui` check clean and build in both profiles,
 * and a `run` prints `stdout`.
 */
export const verifyExample = (name: string, file: string, expect: string, stdout?: string): void => {
  const dir = path.join(repo, "target/tmp", name);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  cpSync(file, path.join(dir, "Main.fl"));
  if (expect.startsWith("fail ")) {
    const checked = spawnSync(flick, ["check", "Main.fl", "--diagnostics", "json"], { cwd: dir, encoding: "utf8" });
    assert.notEqual(checked.status, 0, name);
    assert.deepEqual(errorCodes(checked.stderr), [expect.slice("fail ".length)], checked.stderr);
    return;
  }
  execFileSync(flick, ["check", "Main.fl", "--warnings-as-errors"], { cwd: dir });
  for (const profile of ["debug", "release"]) {
    // as the playground builds it: for Flick's own runtime
    execFileSync(flick, ["build", "Main.fl", "--rt", "--profile", profile, "--out-dir", profile], { cwd: dir });
    if (expect !== "run") continue;
    const out = execFileSync("node", [path.join(here, "../run-linked.ts"), path.join(dir, profile), "flick-main.js"], {
      cwd: path.join(here, ".."),
      encoding: "utf8",
    });
    assert.equal(out, readFileSync(stdout ?? "", "utf8"), `${name}, ${profile}`);
  }
};
