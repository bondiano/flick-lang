// A document in jsdom for the UI e2e program, and a scripted user.
import { JSDOM } from "jsdom";
import { createRoot } from "react-dom/client";

let dom;
let root;

export const setup = () => {
  dom = new JSDOM('<!doctype html><html><body><div id="app"></div></body></html>');
  Object.assign(globalThis, { window: dom.window, document: dom.window.document });
  // react-dom loaded before this document existed, so it takes the old IE
  // input-event path on focus, which calls these
  Object.assign(dom.window.HTMLElement.prototype, { attachEvent: () => undefined, detachEvent: () => undefined });
  root = dom.window.document.getElementById("app");
  return undefined;
};

let errors = [];

/** A page a server rendered: `markup` in the container, `state` after it; React's complaints are kept. */
export const serve = (markup, state) => serveAt("http://localhost/", markup, state);

/** `serve` at the URL `url`. */
export const serveAt = (url, markup, state) => {
  dom = new JSDOM(`<!doctype html><html><body><div id="app" data-flick-ssr>${markup}</div>${state}</body></html>`, { url });
  Object.assign(globalThis, { window: dom.window, document: dom.window.document });
  root = dom.window.document.getElementById("app");
  console.error = (...args) => errors.push(args.join(" "));
  return undefined;
};

/** What React reported since `serve`, such as a hydration mismatch. */
export const complaints = () => errors.join("\n");

/** Whether the page still holds the state a server wrote. */
export const stateLeft = () => dom.window.document.getElementById("flick-state") !== null;

export const click = (label) => {
  const button = [...root.querySelectorAll("button")].find((b) => b.textContent === label);
  if (button === undefined) throw new Error(`no button ${label}`);
  button.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true, cancelable: true }));
};

export const choose = (label) => {
  const option = [...root.querySelectorAll('[role="option"]')].find((o) => o.textContent === label);
  if (option === undefined) throw new Error(`no option ${label}`);
  option.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true, cancelable: true }));
};

let pending;

/** A write that ignores interruption and finishes only on `release`. */
export const writeOnRelease = (line) =>
  new Promise((resolve) => {
    pending = () => {
      process.stdout.write(`${line}\n`);
      resolve();
    };
  });

export const release = () => {
  pending?.();
  pending = undefined;
};

export const settle = () => new Promise((resolve) => setTimeout(resolve, 20));

export const text = () => root.textContent;

export const markup = () => root.innerHTML;

/** A React component of the page, which Flick sees as an opaque `Ui.Component`. */
export const Badge = (props) => {
  return `badge: ${props.label}`;
};

/** A factory, not a root: `Ui.mount_with` mounts it when the task starts. */
export const renderer = () => {
  const mounted = createRoot(root);
  return {
    render: (node) => mounted.render(node),
    unmount: () => mounted.unmount(),
  };
};

/** The tag of the element that has the keyboard focus. */
export const focused = () => dom.window.document.activeElement?.tagName.toLowerCase() ?? "none";
