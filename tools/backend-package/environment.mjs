// Inputs shared by the release build and its regression tests.
import { realpathSync } from "node:fs";
import { delimiter, dirname, join } from "node:path";

const canonical = (path) => {
  try { return realpathSync(path); } catch { return path; }
};

export const guestEnvironment = ({ root, cargoHome, cargo, rustc, rustcVersion, environment }) => {
  const commit = /^commit-hash: ([a-f0-9]{40})$/m.exec(rustcVersion)?.[1];
  if (!commit) throw new Error("the pinned rustc must report its full commit hash");
  const virtual = `/rustc/${commit}`;
  const paths = new Map([
    [root, "/flick"], [canonical(root), "/flick"],
    [cargoHome, "/cargo"], [canonical(cargoHome), "/cargo"],
    ["/kache/.cargo", "/cargo"],
    // Rust 1.95 otherwise resolves imported /rustc paths to the local
    // rust-src component. This destination opts out even when it is installed.
    [virtual, virtual],
  ]);
  const env = {
    ...environment,
    PATH: `${dirname(cargo)}${delimiter}${environment.PATH ?? ""}`,
    RUSTC: rustc,
    RUSTC_WRAPPER: "",
    RUSTC_WORKSPACE_WRAPPER: "",
    CARGO_TARGET_DIR: join(root, "target/backend-guest"),
    CARGO_ENCODED_RUSTFLAGS: [...paths].map(([from, to]) => `--remap-path-prefix=${from}=${to}`).join("\x1f"),
  };
  delete env.RUSTFLAGS;
  return env;
};
