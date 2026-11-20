// The editor features of the language server, over the playground's own
// session: what each name is, the types the code leaves out, the signature of
// the call at the cursor, and the rewrites the checker offers.
import { StateEffect, StateField, type Extension, type Range } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  type PluginValue,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
  keymap,
} from "@codemirror/view";

import { request } from "./ide-client.ts";
import type { Action, Signature, Tokens } from "./ide-worker.ts";

/** The semantic-token marks, one class for each kind. */
const marks = new Map<string, Decoration>();
const mark = (type: string): Decoration => {
  let found = marks.get(type);
  if (found === undefined) {
    marks.set(type, (found = Decoration.mark({ class: `fp-ide-token fp-ide-${type}` })));
  }
  return found;
};

class Label extends WidgetType {
  constructor(readonly text: string, readonly className: string) {
    super();
  }
  eq(other: Label): boolean {
    return other.text === this.text && other.className === this.className;
  }
  toDOM(): HTMLElement {
    const span = document.createElement("span");
    span.className = this.className;
    span.textContent = this.text;
    return span;
  }
  ignoreEvent(): boolean {
    return true;
  }
}

const showTokens = StateEffect.define<{ text: string; tokens: Tokens }>();
const showSignature = StateEffect.define<{ text: string; at: number; help: Signature | null }>();

const inRange = (at: number, length: number): number => Math.max(0, Math.min(at, length));

/** The decorations of the tokens of `text`, or none when it moved on. */
const decorated = (doc: string, value: { text: string; tokens: Tokens }): DecorationSet => {
  if (value.text !== doc) return Decoration.none;
  const ranges: Array<Range<Decoration>> = [];
  for (const token of value.tokens.tokens) {
    const from = inRange(token.from, doc.length);
    const to = inRange(token.to, doc.length);
    if (to > from) ranges.push(mark(token.type).range(from, to));
  }
  return Decoration.set(ranges, true);
};

interface Shown {
  readonly text: string;
  readonly decorations: DecorationSet;
}

const tokens = StateField.define<Shown>({
  create: () => ({ text: "", decorations: Decoration.none }),
  update: (value, transaction) => {
    if (transaction.docChanged) return { text: "", decorations: Decoration.none };
    let next = value;
    for (const effect of transaction.effects) {
      if (effect.is(showTokens)) {
        next = { text: effect.value.text, decorations: decorated(transaction.state.doc.toString(), effect.value) };
      }
    }
    return next;
  },
  provide: (field) => EditorView.decorations.from(field, (value) => value.decorations),
});

const signature = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update: (value, transaction) => {
    if (transaction.docChanged) return Decoration.none;
    let next = value;
    for (const effect of transaction.effects) {
      if (effect.is(showSignature)) {
        const { text, at, help } = effect.value;
        next =
          text !== transaction.state.doc.toString() || help === null
            ? Decoration.none
            : Decoration.set([
                Decoration.widget({
                  widget: new Label(help.label, "fp-ide-signature"),
                  side: 1,
                }).range(inRange(at, transaction.state.doc.length)),
              ]);
      }
    }
    return next;
  },
  provide: (field) => EditorView.decorations.from(field),
});

/** The edits of `action`, over the text they were offered for. */
const rewrite = (view: EditorView, source: string, action: Action): void => {
  if (view.state.doc.toString() !== source) return;
  const changes = action.edits
    .map((edit) => ({ from: edit.from, to: edit.to, insert: edit.text }))
    .sort((left, right) => left.from - right.from);
  view.dispatch({ changes, selection: { anchor: changes[0]?.from ?? view.state.selection.main.head } });
  view.focus();
};

/** A small menu of the rewrites at `at`, over the editor. */
const menu = (view: EditorView, at: number, source: string, actions: ReadonlyArray<Action>): void => {
  document.querySelector(".fp-ide-menu")?.remove();
  const box = document.createElement("div");
  box.className = "fp-ide-menu";
  Object.assign(box.style, {
    position: "fixed",
    zIndex: "100",
    display: "flex",
    flexDirection: "column",
    minWidth: "12em",
    padding: "0.2em",
    border: "1px solid #8886",
    borderRadius: "4px",
    background: "var(--sl-color-bg, #fff)",
    boxShadow: "0 4px 12px #0003",
  } satisfies Partial<CSSStyleDeclaration>);
  for (const action of actions) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "fp-ide-action";
    button.textContent = action.title;
    button.style.textAlign = "left";
    button.style.padding = "0.3em 0.5em";
    button.addEventListener("click", () => {
      box.remove();
      rewrite(view, source, action);
    });
    box.append(button);
  }
  const where = view.coordsAtPos(at);
  box.style.left = `${(where?.left ?? 8).toString()}px`;
  box.style.top = `${((where?.bottom ?? 8) + 4).toString()}px`;
  document.body.append(box);
  const close = (event: Event): void => {
    if (event instanceof MouseEvent && box.contains(event.target as Node)) return;
    box.remove();
    window.removeEventListener("pointerdown", close);
  };
  window.addEventListener("pointerdown", close);
};

const offer = (view: EditorView): boolean => {
  const at = view.state.selection.main.head;
  const source = view.state.doc.toString();
  void request({ action: "actions", source, from: at, to: at })
    .then((result) => {
      const actions = (result ?? []) as ReadonlyArray<Action>;
      if (actions.length > 0) menu(view, at, source, actions);
    })
    .catch(() => undefined);
  return true;
};

class Editor implements PluginValue {
  private timer: number | undefined;
  private generation = 0;
  private destroyed = false;

  constructor(readonly view: EditorView) {
    this.schedule();
  }

  update(update: ViewUpdate): void {
    if (update.docChanged || update.selectionSet) this.schedule();
  }

  destroy(): void {
    this.destroyed = true;
    if (this.timer !== undefined) clearTimeout(this.timer);
    document.querySelector(".fp-ide-menu")?.remove();
  }

  private schedule(): void {
    if (this.timer !== undefined) clearTimeout(this.timer);
    // an edit under the cursor waits for the typing to pause
    this.timer = window.setTimeout(() => {
      this.timer = undefined;
      void this.load();
    }, 120);
  }

  private async load(): Promise<void> {
    const source = this.view.state.doc.toString();
    const at = this.view.state.selection.main.head;
    const before = at > 0 ? source.slice(at - 1, at) : "";
    const generation = ++this.generation;
    const [kinds, help] = await Promise.all([
      request({ action: "tokens", source }).catch(() => null),
      before === "(" || before === ","
        ? request({ action: "signature", source, at }).catch(() => null)
        : Promise.resolve(null),
    ]);
    if (this.destroyed || generation !== this.generation || this.view.state.doc.toString() !== source) return;
    if (kinds !== null) {
      this.view.dispatch({ effects: showTokens.of({ text: source, tokens: kinds as Tokens }) });
    }
    this.view.dispatch({ effects: showSignature.of({ text: source, at, help: help as Signature | null }) });
  }
}

const theme = EditorView.theme({
  "& .fp-ide-signature": {
    margin: "0 0.2em",
    padding: "0.1em 0.4em",
    border: "1px solid #8886",
    borderRadius: "4px",
    fontFamily: "var(--sl-font-mono, monospace)",
    fontSize: "0.85em",
    background: "var(--sl-color-bg, #fff)",
  },
});

/** The editor features of a session, mounted beside the highlighter. */
export const ide = (): Extension => [
  tokens,
  signature,
  theme,
  keymap.of([{ key: "Mod-.", run: offer }]),
  ViewPlugin.fromClass(Editor),
];
