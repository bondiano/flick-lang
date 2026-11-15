// The distributed guest uses canonical source paths on every build machine.
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { guestEnvironment } from "./environment.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const channel = /^channel = "([^"]+)"/m.exec(readFileSync(join(root, "rust-toolchain.toml"), "utf8"))?.[1];
if (!channel) throw new Error("rust-toolchain.toml must pin a compiler channel");
const cargoHome = resolve(process.env.CARGO_HOME ?? join(homedir(), ".cargo"));
const tool = (name) => {
  const result = spawnSync("rustup", ["which", "--toolchain", channel, name], { cwd: root, encoding: "utf8" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(result.stderr || `cannot locate pinned ${name}`);
  return result.stdout.trim();
};
const cargo = tool("cargo");
const rustc = tool("rustc");
const version = spawnSync(rustc, ["-vV"], { cwd: root, encoding: "utf8" });
if (version.error) throw version.error;
if (version.status !== 0) throw new Error(version.stderr || "cannot inspect the pinned rustc");
const env = guestEnvironment({ root, cargoHome, cargo, rustc, rustcVersion: version.stdout, environment: process.env });
for (const [command, extra] of [["build", ["--release"]], ["clippy", ["--", "-D", "warnings"]]]) {
  const result = spawnSync(cargo, [command, "-p", "flick-backend-js", "--features", "guest", "--target", "wasm32-unknown-unknown", ...extra], { cwd: root, env, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
