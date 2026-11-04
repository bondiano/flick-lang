// A streamed page in jsdom for the stream e2e program: chunks arrive as a
// parser would insert them, running microtasks before each script.
import { JSDOM } from "jsdom";

const SEPARATOR = "<!--chunk-->";
let dom;
let rest = [];
let runs = 0;
const errors = [];

let pending = 0;

/** The user's name, after a while; counts how often a task asked for it. */
export const later = () => {
  runs += 1;
  pending += 1;
  return new Promise((resolve) =>
    setTimeout(() => {
      pending -= 1;
      resolve("ada");
    }, 10),
  );
};

export const taskRuns = () => runs;

const append = async (container, html) => {
  const template = dom.window.document.createElement("template");
  template.innerHTML = html;
  for (const node of Array.from(template.content.childNodes)) {
    if (node instanceof dom.window.HTMLScriptElement) {
      // oxlint-disable-next-line eslint/no-await-in-loop -- each script waits for the microtasks before it
      await Promise.resolve();
      const script = dom.window.document.createElement("script");
      for (const { name, value } of Array.from(node.attributes)) script.setAttribute(name, value);
      script.text = node.text;
      container.append(script);
    } else container.append(node);
  }
};

/** A document still loading that has received the chunks up to the shell's fallback. */
export const streamShell = async (joined) => {
  dom = new JSDOM("<!doctype html><html><body></body></html>", { runScripts: "dangerously", pretendToBeVisual: true });
  Object.defineProperty(dom.window.document, "readyState", { configurable: true, get: () => "loading" });
  Object.assign(globalThis, { window: dom.window, document: dom.window.document });
  // the server's and the client's React share this process, which a page never does
  console.error = (...args) => {
    const message = args.join(" ");
    if (!message.includes("multiple renderers concurrently")) errors.push(message);
  };
  const chunks = joined.split(SEPARATOR);
  const shell = chunks.findIndex((chunk) => chunk.includes("waiting")) + 1;
  rest = chunks.slice(shell);
  await append(dom.window.document.body, chunks.slice(0, shell).join(""));
  return undefined;
};

/** The chunks after the shell; React reveals a boundary at most every 300 ms. */
export const streamRest = async () => {
  await append(dom.window.document.body, rest.join(""));
  await quiet(700);
  return undefined;
};

const tick = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** 20ms, then until no check is running and the page stays the same for three ticks: a busy machine renders late. */
export const settle = () => quiet(20);

const page = () => dom?.window.document.body?.innerHTML ?? "";

const quiet = async (ms) => {
  await tick(ms);
  for (let last = page(), still = 0; still < 3; ) {
    await tick(10);
    const now = page();
    still = pending === 0 && now === last ? still + 1 : 0;
    last = now;
  }
};

export const text = () => dom.window.document.getElementById("app")?.textContent ?? "";

export const complaints = () => errors.join("\n");

export const close = () => {
  dom.window.close();
  return undefined;
};
