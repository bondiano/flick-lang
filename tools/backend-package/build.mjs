// The distributed guest uses canonical source paths on every build machine.
import { spawnSync } from "node:child_process";
import { readFileSync, realpathSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const channel = /^channel = "([^"]+)"/m.exec(readFileSync(join(root, "rust-toolchain.toml"), "utf8"))?.[1];
if (!channel) throw new Error("rust-toolchain.toml must pin a compiler channel");
const cargoHome = resolve(process.env.CARGO_HOME ?? join(homedir(), ".cargo"));
const canonical = (path) => {
  try { return realpathSync(path); } catch { return path; }
};
const paths = new Map([
  [root, "/flick"], [canonical(root), "/flick"],
  [cargoHome, "/cargo"], [canonical(cargoHome), "/cargo"],
  ["/kache/.cargo", "/cargo"],
]);
const tool = (name) => {
  const result = spawnSync("rustup", ["which", "--toolchain", channel, name], { cwd: root, encoding: "utf8" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(result.stderr || `cannot locate pinned ${name}`);
  return result.stdout.trim();
};
const cargo = tool("cargo");
const env = {
  ...process.env,
  PATH: `${dirname(cargo)}${delimiter}${process.env.PATH ?? ""}`,
  RUSTC: tool("rustc"),
  // Compiler-cache wrappers can rewrite crate identities and source paths.
  // Keep this artifact's cache separate from wrapped development builds.
  RUSTC_WRAPPER: "",
  RUSTC_WORKSPACE_WRAPPER: "",
  CARGO_TARGET_DIR: join(root, "target/backend-guest"),
  CARGO_ENCODED_RUSTFLAGS: [...paths].map(([from, to]) => `--remap-path-prefix=${from}=${to}`).join("\x1f"),
};
delete env.RUSTFLAGS;
for (const [command, extra] of [["build", ["--release"]], ["clippy", ["--", "-D", "warnings"]]]) {
  const result = spawnSync(cargo, [command, "-p", "flick-backend-js", "--features", "guest", "--target", "wasm32-unknown-unknown", ...extra], { cwd: root, env, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
