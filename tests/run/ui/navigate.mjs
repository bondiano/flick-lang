// A page at a URL in jsdom for the navigation e2e program: clicks on links
// and submits of forms, telling whether the browser's default ran.
import { JSDOM } from "jsdom";

let dom;

export const open = (url) => {
  dom = new JSDOM('<!doctype html><html><body><div id="app"></div></body></html>', { url });
  Object.assign(globalThis, { window: dom.window, document: dom.window.document });
  return undefined;
};

const node = (selector) => {
  const found = dom.window.document.querySelector(selector);
  if (found === null) throw new Error(`nothing matches ${selector}`);
  return found;
};

/** Clicks the element; with `ctrl` as a click for a new tab. "prevented" when the app took it. */
export const click = (selector, ctrl) => {
  const event = new dom.window.MouseEvent("click", { bubbles: true, cancelable: true, button: 0, ctrlKey: ctrl });
  let decided;
  // after the app's listener: note its decision, then keep jsdom, which cannot open a tab, in place
  const after = (e) => {
    decided = e.defaultPrevented ? "prevented" : "followed";
    e.preventDefault();
  };
  dom.window.addEventListener("click", after, { once: true });
  node(selector).dispatchEvent(event);
  return decided;
};

/** Submits the form as Enter in its input would. */
export const submit = (selector) => {
  const event = new dom.window.Event("submit", { bubbles: true, cancelable: true });
  return node(selector).dispatchEvent(event) ? "reloads" : "stays";
};

export const path = () => dom.window.location.pathname;

export const href = (selector) => node(selector).getAttribute("href") ?? "";

export const settle = () => new Promise((resolve) => setTimeout(resolve, 50));

export const text = () => dom.window.document.getElementById("app")?.textContent ?? "";

export const close = () => {
  dom.window.close();
  return undefined;
};
