// Templates: a `Ui` as HTML without React, for mail and other markup no
// client hydrates. A separate entry point that imports neither React nor
// `react-dom/server`.
import * as Atom from "../effect/reactivity/Atom.ts";
import * as AtomRegistry from "../effect/reactivity/AtomRegistry.ts";

import type { Ui } from "./react/nodes.ts";

/** Elements without content or end tag. */
const VOID = /*#__PURE__*/ new Set(["area", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"]);

const escapeText = (s: string): string => s.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
const escapeAttr = (s: string): string => escapeText(s).replaceAll('"', "&quot;");

/** The HTML name of a React prop. */
const attrName = (name: string): string => (name === "className" ? "class" : name === "defaultValue" ? "value" : name.toLowerCase());

/** A boolean is an enumerated value for these, as React writes it; for others its presence. */
const stringBoolean = (name: string): boolean => name.startsWith("aria-") || name.startsWith("data-") || name === "spellCheck";

const html = (node: Ui, read: (atom: Atom.Atom<unknown>) => unknown): string => {
  if (node.kind === "text") return escapeText(node.value);
  if (node.kind === "fragment") return node.children.map((child) => html(child, read)).join("");
  if (node.kind === "keyed") return html(node.node, read);
  if (node.kind === "component") {
    if (node.static !== undefined) return html(node.static(read), read);
    const name = node.component.displayName ?? node.component.name;
    throw new TypeError(`the foreign component \`${name}\` renders only with React: use \`Ui.Server.render\``);
  }
  let attrs = "";
  let content = node.html ?? node.children.map((child) => html(child, read)).join("");
  for (const [name, value] of Object.entries(node.attrs)) {
    // a text area's value is its content
    if (node.tag === "textarea" && (name === "value" || name === "defaultValue")) content = escapeText(String(value));
    else if (typeof value !== "boolean" || stringBoolean(name)) attrs += ` ${attrName(name)}="${escapeAttr(String(value))}"`;
    else if (value) attrs += ` ${attrName(name)}`;
  }
  return VOID.has(node.tag) ? `<${node.tag}${attrs}>` : `<${node.tag}${attrs}>${content}</${node.tag}>`;
};

/**
 * `Ui.Server.to_html`: the HTML of `ui`. Text and attribute values are
 * escaped; views read atoms as a server does, from a registry of this call
 * alone, so the same `ui` always gives the same HTML.
 */
export const uiToHtml = (ui: Ui): string => {
  const registry = AtomRegistry.make();
  try {
    return html(ui, (atom) => Atom.getServerValue(atom, registry));
  } finally {
    registry.dispose();
  }
};
