// CodeMirror's DOM adapter for compiler Markdown. Never interpret documentation as HTML.
import { fromMarkdown } from "mdast-util-from-markdown";
import type { Nodes } from "mdast";
import { highlight } from "./highlight.ts";

/** Render compiler documentation with the editor's existing syntax colours. */
export const renderMarkdown = (markdown: string, document: Document): HTMLElement => {
  const tree = fromMarkdown(markdown);
  const definitions = new Map(tree.children.filter(node => node.type === "definition").map(node => [node.identifier, node]));
  const safeUrl = (url: string): boolean => {
    try { return ["http:", "https:", "mailto:"].includes(new URL(url, "https://flick.invalid/").protocol); }
    catch { return false; }
  };
  const render = (node: Nodes): Node => {
    if (node.type === "text" || node.type === "html") return document.createTextNode(node.value);
    if (node.type === "image" || node.type === "imageReference") return document.createTextNode(node.alt ?? "");
    if (node.type === "definition") return document.createTextNode("");
    if (node.type === "code" || node.type === "inlineCode") {
      const code = document.createElement("code");
      const language = node.type === "inlineCode" ? "" : node.lang;
      if (language === "flick" || language === "javascript" || language === "js") {
        try {
          for (const token of highlight(node.value, language === "flick" ? "flick" : "javascript")) {
            const span = document.createElement("span");
            if (token.class_name) span.className = token.class_name;
            span.textContent = token.text;
            code.append(span);
          }
        } catch {
          code.textContent = node.value;
        }
      } else code.textContent = node.value;
      if (node.type === "inlineCode") return code;
      const pre = document.createElement("pre");
      pre.append(code);
      return pre;
    }
    const tag = node.type === "paragraph" ? "p"
      : node.type === "strong" ? "strong"
      : node.type === "emphasis" ? "em"
      : node.type === "heading" ? `h${node.depth}`
      : node.type === "list" ? (node.ordered ? "ol" : "ul")
      : node.type === "listItem" ? "li"
      : node.type === "blockquote" ? "blockquote"
      : node.type === "break" ? "br"
      : node.type === "thematicBreak" ? "hr"
      : node.type === "link" || node.type === "linkReference" ? "a" : "div";
    const element = document.createElement(tag);
    if (node.type === "list" && node.ordered && node.start !== null && node.start !== undefined) element.setAttribute("start", String(node.start));
    if (node.type === "link" || node.type === "linkReference") {
      const link = node.type === "link" ? node : definitions.get(node.identifier);
      if (link !== undefined && safeUrl(link.url)) element.setAttribute("href", link.url);
      if (link?.title) element.setAttribute("title", link.title);
    }
    if ("children" in node) for (const child of node.children) element.append(render(child));
    return element;
  };
  const result = document.createElement("div");
  result.className = "fp-hover";
  for (const node of tree.children) result.append(render(node));
  return result;
};
