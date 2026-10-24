import type { HighlighterCore, LanguageRegistration, ThemeRegistration } from "shiki/core";

export type HighlightLanguage = "flick" | "javascript";
export type HighlightToken = {
  readonly text: string;
  readonly class_name: "" | "fp-token-comment" | "fp-token-keyword" | "fp-token-string" | "fp-token-invalid";
};

const theme: ThemeRegistration = {
  name: "flick-signal",
  type: "light",
  fg: "#101016",
  bg: "#fafafc",
  tokenColors: [
    { scope: ["comment", "punctuation.definition.comment"], settings: { foreground: "#686873" } },
    { scope: ["keyword", "storage", "entity.name.type", "support.type", "support.class"], settings: { foreground: "#584dd0" } },
    { scope: ["string", "constant", "support.constant"], settings: { foreground: "#347b57" } },
    { scope: "invalid", settings: { foreground: "#b43b4f" } },
  ],
};

const classes: Readonly<Record<string, HighlightToken["class_name"]>> = {
  "#686873": "fp-token-comment",
  "#584DD0": "fp-token-keyword",
  "#347B57": "fp-token-string",
  "#B43B4F": "fp-token-invalid",
};

let highlighter: HighlighterCore | undefined;
let loading: Promise<void> | undefined;

/** Optional decoration loads independently of the native editor and compiler. */
export const loadHighlighting = (): Promise<void> => {
  loading ??= (async () => {
    const [{ createHighlighterCore }, { createJavaScriptRegexEngine }, javascript, flick] = await Promise.all([
      import("shiki/core"),
      import("shiki/engine/javascript"),
      import("shiki/langs/javascript.mjs"),
      import("../../../editors/vscode/syntaxes/flick.tmLanguage.json"),
    ]);
    const { promise, resolve } = Promise.withResolvers<void>();
    if (typeof requestIdleCallback === "function") requestIdleCallback(() => resolve(), { timeout: 1000 });
    else setTimeout(resolve, 0);
    await promise;
    highlighter = await createHighlighterCore({
      engine: createJavaScriptRegexEngine(),
      themes: [theme],
      langs: [{ ...flick.default, name: "flick" } as LanguageRegistration, javascript.default],
    });
  })().catch((error: unknown) => {
    loading = undefined;
    throw error;
  });
  return loading;
};

/** Text and allowlisted classes only; Flick owns the token spans and their state. */
export const highlight = (source: string, language: HighlightLanguage): readonly HighlightToken[] => {
  if (highlighter === undefined) throw new Error("Syntax highlighting has not loaded");
  const lines = highlighter.codeToTokensBase(source, { lang: language, theme: "flick-signal" });
  const result: HighlightToken[] = [];
  let offset = 0;
  for (const line of lines) {
    for (const token of line) {
      const end = token.offset + token.content.length;
      if (token.offset < offset || end > source.length) throw new Error("Invalid syntax token offsets");
      // TextMate omits line endings; retain the original CRLF, blank lines and final newline.
      if (token.offset > offset) result.push({ text: source.slice(offset, token.offset), class_name: "" });
      result.push({ text: source.slice(token.offset, end), class_name: classes[token.color ?? ""] ?? "" });
      offset = end;
    }
  }
  if (offset < source.length) result.push({ text: source.slice(offset), class_name: "" });
  return result;
};

/** Native textarea geometry only: no source, input actions or widget state. */
export const bindEditorScroll = (root: HTMLElement): (() => void) => {
  let editor: HTMLTextAreaElement | null = null;
  let overlay: HTMLPreElement | null = null;
  const copyScroll = (): void => {
    if (editor === null || overlay === null) return;
    overlay.scrollTop = editor.scrollTop;
    overlay.scrollLeft = editor.scrollLeft;
  };
  const sync = (): void => {
    const next = root.querySelector<HTMLTextAreaElement>(".fp-editor textarea");
    if (next !== editor) {
      if (editor !== null) resize.unobserve(editor);
      editor = next;
      if (editor !== null) resize.observe(editor);
    }
    overlay = editor?.closest(".fp-editor")?.querySelector<HTMLPreElement>(".fp-source-highlight") ?? null;
    if (editor === null || overlay === null) return;
    // Match the native viewport, including space consumed by non-overlay scrollbars.
    const width = `${editor.clientWidth}px`;
    const height = `${editor.clientHeight}px`;
    if (overlay.style.width !== width) overlay.style.width = width;
    if (overlay.style.height !== height) overlay.style.height = height;
    copyScroll();
  };
  const resize = new ResizeObserver(sync);
  const mutations = new MutationObserver(sync);
  mutations.observe(root, { childList: true, characterData: true, subtree: true, attributes: true, attributeFilter: ["class"] });
  const onScroll = (event: Event): void => {
    if (event.target === editor) copyScroll();
  };
  root.addEventListener("scroll", onScroll, true);
  sync();
  return () => {
    root.removeEventListener("scroll", onScroll, true);
    resize.disconnect();
    mutations.disconnect();
  };
};
