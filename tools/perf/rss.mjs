// Child-process RSS in MiB. Linux supplies a lifetime high-water mark;
// macOS supplies current RSS, so callers sample it over the measured work.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

export const parseLinuxRss = (text) => ({
  current: Number(/^VmRSS:\s+(\d+)/m.exec(text)?.[1]) / 1024,
  peak: Number(/^VmHWM:\s+(\d+)/m.exec(text)?.[1]) / 1024,
});

export const readRss = (pid) => {
  if (!Number.isSafeInteger(Number(pid)) || Number(pid) <= 0) throw new Error("RSS needs a positive process id");
  if (process.platform === "linux") return parseLinuxRss(readFileSync(`/proc/${pid}/status`, "utf8"));
  if (process.platform === "darwin") {
    const current = Number(execFileSync("ps", ["-o", "rss=", "-p", String(pid)], { encoding: "utf8" }).trim()) / 1024;
    return { current, peak: current };
  }
  throw new Error(`RSS measurement is unsupported on ${process.platform}`);
};

export const sampleRss = (pid) => {
  let peak = 0;
  const sample = () => {
    const value = readRss(pid);
    peak = Math.max(peak, value.current, value.peak);
    return value.current;
  };
  sample();
  const timer = setInterval(sample, 250);
  timer.unref();
  return {
    stop: () => {
      clearInterval(timer);
      sample();
      return peak;
    },
  };
};
