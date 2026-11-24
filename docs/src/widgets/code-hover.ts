// Browser and compiler interop only, as the playground's `hoverTooltip`: a Flick panel of the page
// shows what the compiler says about the name under the pointer. Nothing loads before the first ask.
import { hover } from "../playground/compiler.ts";

type Hover = NonNullable<Awaited<ReturnType<typeof hover>>>;
interface Asked { readonly panel: HTMLElement; readonly at: number }

const REST = 300; // CodeMirror's default before a hover
const GRACE = 300; // time to move from the word onto the tooltip

/** The DOM position of the offset `at` in the text of `root`. */
const position = (root: Node, at: number): [Node, number] => {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let left = at;
  for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
    const length = node.textContent?.length ?? 0;
    if (left <= length) return [node, left];
    left -= length;
  }
  return [root, root.childNodes.length];
};

const rangeOf = (code: Element, from: number, to: number): Range => {
  const range = document.createRange();
  range.setStart(...position(code, from));
  range.setEnd(...position(code, to));
  return range;
};

const caretAt = (x: number, y: number): [Node, number] | undefined => {
  const caret = document.caretPositionFromPoint?.(x, y);
  if (caret) return [caret.offsetNode, caret.offset];
  // older WebKit has only the range form
  const range = document.caretRangeFromPoint?.(x, y);
  return range ? [range.startContainer, range.startOffset] : undefined;
};

/** The offset in the panel's code under the point, or nothing between the lines. */
const asked = (x: number, y: number): Asked | undefined => {
  const caret = caretAt(x, y);
  const element = caret && (caret[0] instanceof Element ? caret[0] : caret[0].parentElement);
  const panel = element?.closest<HTMLElement>(".code-panel[data-sample]");
  const code = panel?.querySelector("code");
  if (!caret || !panel || !code?.contains(caret[0])) return undefined;
  const before = document.createRange();
  before.setStart(code, 0);
  before.setEnd(...caret);
  return { panel, at: before.toString().length };
};

const within = (range: Range, x: number, y: number): boolean =>
  [...range.getClientRects()].some((box) => x >= box.left && x <= box.right && y >= box.top && y <= box.bottom);

export const watch = (): void => {
  const data = document.getElementById("flick-samples")?.textContent;
  if (!data) return;
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the page writes it from `sampleSources`
  const sources = JSON.parse(data) as Record<string, string>;
  const tip = document.createElement("div");
  tip.className = "code-hover";
  tip.id = "flick-code-hover";
  tip.setAttribute("role", "tooltip");
  tip.hidden = true;
  document.body.append(tip);
  for (const hint of document.querySelectorAll<HTMLElement>(".hover-hint")) hint.hidden = false;

  let shown: Range | undefined;
  let resting: ReturnType<typeof setTimeout> | undefined;
  let leaving: ReturnType<typeof setTimeout> | undefined;
  let asking: AbortController | undefined;

  const hide = (): void => {
    asking?.abort();
    clearTimeout(leaving);
    tip.hidden = true;
    shown = undefined;
  };

  const show = async (x: number, y: number): Promise<void> => {
    const point = asked(x, y);
    if (!point) return;
    const source = sources[point.panel.dataset.sample ?? ""];
    const base = Number(point.panel.dataset.offset);
    if (source === undefined || Number.isNaN(base)) return;
    asking?.abort();
    const controller = new AbortController();
    asking = controller;
    const [answer, { renderMarkdown }] = await Promise.all([
      hover(source, base + point.at, controller.signal).catch((): Hover | null => null),
      import("../playground/markdown.ts"),
      import("../playground/highlight.ts").then(({ loadHighlighting }) => loadHighlighting()).catch(() => undefined),
    ]);
    const code = point.panel.querySelector("code");
    if (controller.signal.aborted || !answer || !code) return;
    const range = rangeOf(code, answer.from - base, answer.to - base);
    if (answer.from < base || !within(range, x, y)) return;
    tip.replaceChildren(renderMarkdown(answer.text, document));
    tip.style.left = "0px";
    tip.hidden = false;
    shown = range;
    const box = range.getBoundingClientRect();
    const width = tip.offsetWidth;
    const above = box.top - tip.offsetHeight - 6;
    tip.style.left = `${scrollX + Math.max(8, Math.min(box.left, innerWidth - width - 8))}px`;
    tip.style.top = `${scrollY + (above >= 8 ? above : box.bottom + 6)}px`;
  };

  const onTip = (target: EventTarget | null): boolean => target instanceof Node && tip.contains(target);
  const onPanel = (target: EventTarget | null): boolean => target instanceof Element && target.closest(".code-panel[data-sample]") !== null;

  document.addEventListener("pointermove", (event) => {
    if (event.pointerType !== "mouse") return;
    if (onTip(event.target) || (shown !== undefined && within(shown, event.clientX, event.clientY))) {
      clearTimeout(leaving);
      return;
    }
    clearTimeout(resting);
    if (tip.hidden) asking?.abort(); // moved on before the answer came
    else {
      clearTimeout(leaving);
      leaving = setTimeout(hide, GRACE);
    }
    if (onPanel(event.target)) resting = setTimeout(() => void show(event.clientX, event.clientY), REST);
  });
  // A touch or a pen has no hover: a tap asks at once, and a tap anywhere else hides.
  document.addEventListener("pointerup", (event) => {
    if (event.pointerType === "mouse" || onTip(event.target)) return;
    hide();
    if (onPanel(event.target)) void show(event.clientX, event.clientY);
  });
  document.addEventListener("keydown", (event) => { if (event.key === "Escape") hide(); });
  addEventListener("resize", hide);
};
