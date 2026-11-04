// A document in jsdom for the form e2e program: a user who types into
// inputs by id and reads what assistive technology would.
import { JSDOM } from "jsdom";
import { flushSync } from "react-dom";

let dom;

export const setup = () => {
  dom = new JSDOM('<!doctype html><html><body><div id="app"></div></body></html>', { pretendToBeVisual: true });
  Object.assign(globalThis, { window: dom.window, document: dom.window.document });
  // react-dom loaded before this document existed, so it takes the old IE input-event path on focus
  Object.assign(dom.window.HTMLElement.prototype, { attachEvent: () => undefined, detachEvent: () => undefined });
  return undefined;
};

const element = (id) => {
  const found = dom.window.document.getElementById(id);
  if (found === null) throw new Error(`no element ${id}`);
  return found;
};

/** Types `value` as a browser does: the DOM value changes, then `input` fires. */
export const type = (id, value) => {
  const field = element(id);
  // that path watches the focused input only
  field.focus();
  // the prototype's setter: React tracks what its own instance setter is given
  Reflect.set(dom.window.HTMLInputElement.prototype, "value", value, field);
  // react-dom loaded before this document, so it notices a change on a key's release, not on `input`
  flushSync(() => {
    field.dispatchEvent(new dom.window.Event("input", { bubbles: true }));
    field.dispatchEvent(new dom.window.KeyboardEvent("keyup", { bubbles: true }));
  });
  return undefined;
};

export const click = (label) => {
  const button = [...dom.window.document.querySelectorAll("button")].find((b) => b.textContent === label);
  if (button === undefined) throw new Error(`no button ${label}`);
  button.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true, cancelable: true }));
  return undefined;
};

/** The element's text, then its `aria-invalid` and `aria-describedby`. */
export const describe = (id) => {
  const input = element(id);
  const described = input.getAttribute("aria-describedby") ?? "";
  const error = described === "" ? "" : (dom.window.document.getElementById(described)?.textContent ?? "");
  return `${id}: invalid=${input.getAttribute("aria-invalid")}, label=${input.getAttribute("aria-label")}, error="${error}"`;
};

export const focused = () => dom.window.document.activeElement?.id ?? "";

export const log = (line) => {
  console.log(line);
  return undefined;
};

let pending = 0;

/** Whether `name` is taken; a short name takes longer, so an uninterrupted check would answer last. */
export const taken = (name) => {
  pending += 1;
  return new Promise((resolve) =>
    setTimeout(
      () => {
        pending -= 1;
        resolve(name === "root");
      },
      name.length <= 3 ? 60 : 10,
    ),
  );
};

const tick = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** 80ms, then until no check is running and the page stays the same for three ticks: a busy machine renders late. */
export const settle = () => quiet(80);

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
