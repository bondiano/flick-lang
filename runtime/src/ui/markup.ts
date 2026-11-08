// A `Ui` as HTML without React, and the JSON scripts of the atoms a server
// hands a client: nothing here needs a runtime.
import type { Ui } from "./react/nodes.ts";

/** Elements without content or end tag. */
const VOID = /*#__PURE__*/ new Set(["area", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"]);

const escapeText = (s: string): string => s.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
const escapeAttr = (s: string): string => escapeText(s).replaceAll('"', "&quot;");

/** The HTML name of a React prop. */
const attrName = (name: string): string => (name === "className" ? "class" : name === "defaultValue" ? "value" : name.toLowerCase());

/** A boolean is an enumerated value for these, as React writes it; for others its presence. */
const stringBoolean = (name: string): boolean => name.startsWith("aria-") || name.startsWith("data-") || name === "spellCheck";

export const html = (node: Ui, read: (atom: object) => unknown): string => {
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

/** A JSON script of `entries` for the root in `container`, with `attribute` naming it. */
export const script = (attribute: string, container: string, list: ReadonlyArray<{ key: string; value: unknown }>): string => {
  // `<` escaped, so no value closes the script
  const json = JSON.stringify(list).replaceAll("<", "\\u003c");
  const name = container.replaceAll("&", "&amp;").replaceAll('"', "&quot;").replaceAll("<", "&lt;");
  return `<script type="application/json" ${attribute}="${attribute === "id" ? "flick-state-" : ""}${name}">${json}</script>`;
};
