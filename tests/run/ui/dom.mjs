// A document in jsdom for the UI e2e program, and a scripted user.
import { JSDOM } from "jsdom";
import { createRoot } from "react-dom/client";

let dom;
let root;

export const setup = () => {
  dom = new JSDOM('<!doctype html><html><body><div id="app"></div></body></html>');
  Object.assign(globalThis, { window: dom.window, document: dom.window.document });
  root = dom.window.document.getElementById("app");
  return undefined;
};

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
