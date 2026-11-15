import assert from "node:assert/strict";
import { delimiter, join } from "node:path";
import test from "node:test";
import { guestEnvironment } from "./environment.mjs";

const commit = "59807616e1fa2540724bfbac14d7976d7e4a3860";
const root = join(process.cwd(), "checkout");
const cargoHome = join(process.cwd(), "registry");
const input = {
  root, cargoHome,
  cargo: join(process.cwd(), "toolchain/bin/cargo"),
  rustc: join(process.cwd(), "toolchain/bin/rustc"),
  rustcVersion: `rustc 1.95.0\ncommit-hash: ${commit}\nhost: aarch64-apple-darwin\n`,
  environment: {
    PATH: "original-path", RUSTFLAGS: "--cfg inherited",
    RUSTC_WRAPPER: "cache-wrapper", RUSTC_WORKSPACE_WRAPPER: "workspace-wrapper",
    CARGO_TARGET_DIR: "shared-target", CARGO_ENCODED_RUSTFLAGS: "inherited-flags",
  },
};

test("guest preserves virtual standard-library paths when rust-src is installed", () => {
  const env = guestEnvironment(input);
  // Rust's imported-source decoder skips local rust-src discovery only when
  // a remap destination is this exact compiler-versioned virtual directory.
  const virtual = `/rustc/${commit}`;
  assert.ok(env.CARGO_ENCODED_RUSTFLAGS.split("\x1f").includes(`--remap-path-prefix=${virtual}=${virtual}`));
});

test("guest isolates wrappers, output and flags from development builds", () => {
  const env = guestEnvironment(input);
  assert.equal(env.RUSTC, input.rustc);
  assert.equal(env.PATH, `${join(process.cwd(), "toolchain/bin")}${delimiter}original-path`);
  assert.equal(env.RUSTC_WRAPPER, "");
  assert.equal(env.RUSTC_WORKSPACE_WRAPPER, "");
  assert.equal(env.CARGO_TARGET_DIR, join(root, "target/backend-guest"));
  assert.equal(env.RUSTFLAGS, undefined);
  assert.ok(!env.CARGO_ENCODED_RUSTFLAGS.includes("inherited-flags"));
});

test("guest rejects missing compiler identity rather than publishing local paths", () => {
  assert.throws(() => guestEnvironment({ ...input, rustcVersion: "rustc 1.95.0" }), /full commit hash/);
});
