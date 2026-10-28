#!/usr/bin/env node
// The process TypeScript starts for `.fl` files: `flick content-mapper`,
// from `FLICK_BIN` or the PATH, speaking on this process's stdio.
import { spawn } from "node:child_process";

const flick = process.env.FLICK_BIN ?? "flick";
const mapper = spawn(flick, ["content-mapper"], { stdio: "inherit" });
mapper.on("error", (error) => {
  console.error(`cannot start ${flick} content-mapper: ${error.message}`);
  process.exit(1);
});
mapper.on("exit", (code, signal) => process.exit(signal === null ? (code ?? 1) : 1));
