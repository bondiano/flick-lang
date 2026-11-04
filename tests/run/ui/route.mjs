// A page at a URL for the route e2e program: the server's chunks arrive at
// once, then the client navigates with the history.
import { JSDOM } from "jsdom";

let dom;
let runs = 0;
let pending = 0;
const errors = [];

/** The name of user `id`, after a while; counts how often a loader asked. */
export const name = (id) => {
  runs += 1;
  pending += 1;
  return new Promise((resolve) =>
    setTimeout(() => {
      pending -= 1;
      resolve(`user ${id}`);
    }, 10),
  );
};

export const loads = () => runs;

/** A document at `url` holding the server's chunks, scripts run in order. */
export const open = async (url, joined) => {
  dom?.window.close();
  dom = new JSDOM(`<!doctype html><html><body></body></html>`, { url, runScripts: "dangerously", pretendToBeVisual: true });
  Object.assign(globalThis, { window: dom.window, document: dom.window.document });
  // the server's and the client's React share this process, which a page never does
  console.error = (...args) => {
    const message = args.join(" ");
    if (!message.includes("multiple renderers concurrently")) errors.push(message);
  };
  const template = dom.window.document.createElement("template");
  template.innerHTML = joined;
  for (const node of Array.from(template.content.childNodes)) {
    if (node instanceof dom.window.HTMLScriptElement) {
      const script = dom.window.document.createElement("script");
      for (const { name: attribute, value } of Array.from(node.attributes)) script.setAttribute(attribute, value);
      script.text = node.text;
      dom.window.document.body.append(script);
    } else dom.window.document.body.append(node);
  }
  return undefined;
};

export const back = async () => {
  dom.window.history.back();
  // jsdom fires `popstate` asynchronously
  await new Promise((resolve) => setTimeout(resolve, 50));
  return undefined;
};

export const path = () => dom.window.location.pathname;

const tick = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** 50ms, then until no name is loading and the page stays the same: a busy machine renders late. */
export const settle = async () => {
  await tick(50);
  for (let last = text(), quiet = 0; quiet < 3; ) {
    await tick(10);
    const now = text();
    quiet = pending === 0 && now === last ? quiet + 1 : 0;
    last = now;
  }
};

export const text = () => dom.window.document.getElementById("app")?.textContent ?? "";

export const complaints = () => errors.join("\n");

export const close = () => {
  dom.window.close();
  return undefined;
};
