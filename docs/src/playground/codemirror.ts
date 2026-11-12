// The editor of the playground, a chunk of its own that `editor.ts` loads after the page.
import { autocompletion, completionKeymap, type CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { type Diagnostic as Mark, lintKeymap, setDiagnostics } from "@codemirror/lint";
import { Annotation, EditorState, RangeSetBuilder } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, hoverTooltip, keymap, lineNumbers, ViewPlugin, type ViewUpdate } from "@codemirror/view";

import { complete, hover } from "./compiler.ts";
import type { Diagnostic } from "./compiler.worker.ts";
import { highlight } from "./highlight.ts";
import { renderMarkdown } from "./markdown.ts";

/** A change Flick made itself, which it does not need to hear back about. */
const external = Annotation.define<boolean>();

const marks = new Map<string, Decoration>();

/** The source painted with the classes of the docs' highlighter; plain when it has not loaded. */
const paint = (text: string): DecorationSet => {
  const builder = new RangeSetBuilder<Decoration>();
  let at = 0;
  try {
    // shortcut: the whole module on each edit, a few ms for a playground's size; paint the viewport for long files
    for (const token of highlight(text, "flick")) {
      const end = at + token.text.length;
      if (token.class_name !== "") {
        let mark = marks.get(token.class_name);
        if (mark === undefined) marks.set(token.class_name, (mark = Decoration.mark({ class: token.class_name })));
        builder.add(at, end, mark);
      }
      at = end;
    }
  } catch {
    // the highlighter failed to load; the playground says so beside the editor
  }
  return builder.finish();
};

class Colors {
  decorations: DecorationSet;
  constructor(view: EditorView) {
    this.decorations = paint(view.state.doc.toString());
  }
  update(update: ViewUpdate): void {
    if (update.docChanged) this.decorations = paint(update.state.doc.toString());
  }
}

const colors = ViewPlugin.fromClass(Colors, { decorations: (plugin) => plugin.decorations });

/** The type under the pointer, as the language server shows it. */
const types = hoverTooltip(async (view, at) => {
  const source = view.state.doc.toString();
  const shown = await hover(source, at).catch(() => null);
  if (shown === null || view.state.doc.toString() !== source) return null;
  return {
    pos: shown.from,
    end: shown.to,
    above: true,
    create: () => ({ dom: renderMarkdown(shown.text, view.dom.ownerDocument) }),
  };
});

const KINDS: Readonly<Record<string, string>> = {
  local: "variable",
  value: "function",
  constructor: "enum",
  type: "type",
  module: "namespace",
  field: "property",
  operation: "method",
  keyword: "keyword",
};

const WORD = /[\p{L}\p{N}_']+/u;

const completions = async (context: CompletionContext): Promise<CompletionResult | null> => {
  const word = context.matchBefore(WORD);
  const from = word?.from ?? context.pos;
  const dotted = context.state.sliceDoc(from - 1, from) === ".";
  if (word === null && !dotted && !context.explicit) return null;
  const controller = new AbortController();
  context.addEventListener("abort", () => controller.abort(), { onDocChange: true });
  // as the language server does: after a `.` the member is cut out, so that the module parses, and the dot is the place
  const doc = context.state.doc;
  const [text, at] = dotted ? [doc.sliceString(0, from - 1) + doc.sliceString(context.pos), from - 1] : [doc.toString(), from];
  const items = await complete(text, at, dotted, controller.signal).catch(() => null);
  if (items === null || items.length === 0) return null;
  return {
    from,
    options: items.map((item) => ({ label: item.label, type: KINDS[item.kind], detail: item.detail ?? undefined })),
    validFor: /^[\p{L}\p{N}_']*$/u,
  };
};

const SEVERITIES: Readonly<Record<string, Mark["severity"]>> = { error: "error", warning: "warning" };

/** CodeMirror in `parent` over `text`; `changed` hears of each edit the reader makes. */
const pressCheckOrRun = (): boolean => {
  const button = document.getElementById("fp-check") ?? document.getElementById("fp-run");
  if (!(button instanceof HTMLButtonElement) || button.disabled) return false;
  button.click();
  return true;
};

export const create = (parent: HTMLElement, text: string, label: string, describedBy: string, changed: () => void): EditorView =>
  new EditorView({
    parent,
    state: EditorState.create({
      doc: text,
      extensions: [
        lineNumbers(),
        history(),
        colors,
        types,
        autocompletion({ override: [completions] }),
        // Ctrl+Enter checks an exercise's answer, or runs the program; Flick owns both buttons
        keymap.of([{ key: "Mod-Enter", run: pressCheckOrRun }, ...completionKeymap, ...lintKeymap, ...defaultKeymap, ...historyKeymap, indentWithTab]),
        EditorView.contentAttributes.of({
          "aria-label": label,
          "aria-describedby": describedBy,
          autocapitalize: "off",
          autocorrect: "off",
          spellcheck: "false",
        }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged && !update.transactions.some((t) => t.annotation(external) === true)) changed();
        }),
      ],
    }),
  });

/** Puts `text` in place of the whole source, as one step of the history. */
export const replace = (view: EditorView, text: string): void => {
  if (view.state.doc.toString() === text) return;
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text }, annotations: external.of(true) });
};

/** Underlines the `diagnostics` of `source` with their fixes, unless the text has moved on since. */
export const mark = (view: EditorView, source: string, diagnostics: ReadonlyArray<Diagnostic>): void => {
  if (view.state.doc.toString() !== source) return;
  const length = view.state.doc.length;
  const shown = diagnostics
    .filter((d) => d.start >= 0)
    .map((d): Mark => ({
      from: Math.min(d.start, length),
      to: Math.min(Math.max(d.end, d.start), length),
      severity: SEVERITIES[d.severity] ?? "info",
      source: d.code,
      message: d.message,
      renderMessage: (view) => renderMarkdown(d.message, view.dom.ownerDocument),
      actions: d.fixes.map((fix) => ({
        name: fix.message,
        // a fix's offsets are those of `source`, the text it was found in
        apply: (view: EditorView) => {
          if (view.state.doc.toString() !== source) return;
          view.dispatch({ changes: fix.edits.map((edit) => ({ from: edit.start, to: edit.end, insert: edit.insert })) });
        },
      })),
    }));
  view.dispatch(setDiagnostics(view.state, shown));
};
