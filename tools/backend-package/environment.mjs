// Inputs shared by the release build and its regression tests.
import { realpathSync } from "node:fs";
import { delimiter, dirname, join } from "node:path";

const canonical = (path) => {
  try { return realpathSync(path); } catch { return path; }
};

export const nativeEnvironment = ({ root, cargo, rustc, environment }) => {
  const env = {
    ...environment,
    PATH: `${dirname(cargo)}${delimiter}${environment.PATH ?? ""}`,
    RUSTC: rustc,
    RUSTC_WRAPPER: "",
    RUSTC_WORKSPACE_WRAPPER: "",
    CARGO_TARGET_DIR: join(root, "target/backend-tools"),
    CARGO_ENCODED_RUSTFLAGS: "",
  };
  delete env.RUSTFLAGS;
  delete env.CARGO_BUILD_TARGET;
  return env;
};

export const guestEnvironment = ({ root, cargoHome, cargo, rustc, wrapper, rustcVersion, environment }) => {
  const commit = /^commit-hash: ([a-f0-9]{40})$/m.exec(rustcVersion)?.[1];
  if (!commit) throw new Error("the pinned rustc must report its full commit hash");
  if (!wrapper) throw new Error("the guest metadata wrapper must be built first");
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
    ...nativeEnvironment({ root, cargo, rustc, environment }),
    RUSTC_WRAPPER: wrapper,
    CARGO_TARGET_DIR: join(root, "target/backend-guest"),
    // Cargo does not invalidate artifacts when only RUSTC_WRAPPER changes.
    // Bump this cfg with the wrapper's identity schema to rebuild old guests.
    CARGO_ENCODED_RUSTFLAGS: ["--cfg=flick_guest_metadata_v1", ...[...paths].map(([from, to]) => `--remap-path-prefix=${from}=${to}`)].join("\x1f"),
    FLICK_GUEST_ROOT: canonical(root),
    FLICK_GUEST_CARGO_HOME: canonical(cargoHome),
    FLICK_GUEST_COMPILER_COMMIT: commit,
  };
  return env;
};
