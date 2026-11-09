// The foreign surface of the editor: Flick owns the source, CodeMirror shows and edits it.
import type { EditorView } from "@codemirror/view";

import type { Diagnostic } from "./compiler.worker.ts";
import { notify } from "./dom.ts";
import { loadHighlighting } from "./highlight.ts";

let editor: typeof import("./codemirror.ts") | undefined;
let view: EditorView | undefined;

/** Loads CodeMirror and the highlighter; the native field stays when this fails. */
export const loadEditor = async (): Promise<void> => {
  // the playground reports a highlighter that does not load; the editor works without it
  const [loaded] = await Promise.all([import("./codemirror.ts"), loadHighlighting().catch(() => undefined)]);
  editor = loaded;
};

/** Opens CodeMirror on `text` in `#fp-cm`; each edit clicks the hidden `#fp-edit`. */
export const mountEditor = (text: string, label: string): void => {
  const parent = document.getElementById("fp-cm");
  if (parent === null || editor === undefined) throw new Error("The editor has not loaded");
  view?.destroy();
  view = editor.create(parent, text, label, "fp-source-hint", () => notify("fp-edit"));
};

export const editorText = (): string => view?.state.doc.toString() ?? "";

export const setEditorText = (text: string): void => {
  if (view !== undefined) editor?.replace(view, text);
  else {
    const field = document.querySelector<HTMLTextAreaElement>("#flick-playground textarea");
    if (field !== null) field.value = text;
  }
};

export const markDiagnostics = (source: string, diagnostics: ReadonlyArray<Diagnostic>): void => {
  if (view !== undefined) editor?.mark(view, source, diagnostics);
};

/** Focuses the editor; false when it is not open. */
export const focusEditor = (): boolean => {
  view?.focus();
  return view !== undefined;
};
