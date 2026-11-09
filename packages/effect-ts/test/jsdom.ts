// A jsdom document as the global `window`, installed when this module loads:
// import it before React, which decides what DOM events it supports when it
// is first imported.
import { JSDOM } from "jsdom";

export const dom = new JSDOM("<!doctype html><html><body></body></html>");
Object.assign(globalThis, { window: dom.window, document: dom.window.document });
