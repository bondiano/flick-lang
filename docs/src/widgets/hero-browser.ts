import { compile } from "../playground/compiler.ts";

// Compiler and map interop; what to check and what the answer means belong to Hero.fl.
export const check = async (source: string, signal: AbortSignal) => (await compile(source, signal)).debug.diagnostics;

export const showBreak = (broken: boolean): void => {
  const map = document.getElementById("flick-map");
  map?.toggleAttribute("data-broken", broken);
  // on a phone the diagram sits below the button, and the break is what the press was for
  if (broken) map?.scrollIntoView({ block: "nearest", behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
};
