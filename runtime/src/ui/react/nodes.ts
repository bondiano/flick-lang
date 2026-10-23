// `Ui r e` values: inert descriptions of elements. Building one runs no
// handler and touches no DOM; the bridge turns them into React elements.
import { Option } from "effect";
import type { ComponentType } from "react";

import type { Handler } from "./policies.ts";

/** What an event prop of an element gives its handler. */
export interface Binding {
  readonly handler: Handler;
  /** An inert copy of what the handler may see of the event, taken at once. */
  readonly snapshot: (event: DomEvent) => unknown;
}

/** The part of a React synthetic event the bridge reads. */
export interface DomEvent {
  readonly target: unknown;
  readonly button?: number;
  readonly altKey?: boolean;
  readonly ctrlKey?: boolean;
  readonly shiftKey?: boolean;
  readonly metaKey?: boolean;
  preventDefault(): void;
}

export type Ui =
  | { readonly kind: "text"; readonly value: string }
  | { readonly kind: "fragment"; readonly children: ReadonlyArray<Ui> }
  | { readonly kind: "keyed"; readonly key: string; readonly node: Ui }
  | { readonly kind: "component"; readonly component: ComponentType<object>; readonly props: object }
  | {
      readonly kind: "element";
      readonly tag: string;
      readonly attrs: Readonly<Record<string, string | boolean>>;
      readonly events: Readonly<Record<string, Binding>>;
      readonly children: ReadonlyArray<Ui>;
      /** Trusted HTML, inserted as it is. */
      readonly html?: string;
    };

/** `Tab r e`. */
export interface Tab {
  readonly label: string;
  readonly node: Ui;
}

const element = (
  tag: string,
  attrs: Record<string, string | boolean>,
  events: Record<string, Binding>,
  children: ReadonlyArray<Ui>,
): Ui => Object.freeze({ kind: "element", tag, attrs: Object.freeze(attrs), events: Object.freeze(events), children });

const bool = (value: unknown): boolean => value === true;

const clickSnapshot = (event: DomEvent): unknown =>
  Object.freeze({
    button: event.button ?? 0,
    alt: bool(event.altKey),
    ctrl: bool(event.ctrlKey),
    shift: bool(event.shiftKey),
    meta: bool(event.metaKey),
  });

const inputSnapshot = (event: DomEvent): unknown => {
  const value: unknown = typeof event.target === "object" && event.target !== null ? Reflect.get(event.target, "value") : "";
  return Object.freeze({ value: typeof value === "string" ? value : "" });
};

export const uiText = (value: string): Ui => Object.freeze({ kind: "text", value });
export const uiFragment = (children: ReadonlyArray<Ui>): Ui => Object.freeze({ kind: "fragment", children });
export const uiOptional = (node: Option.Option<Ui>): Ui => (Option.isSome(node) ? node.value : uiFragment([]));
export const uiComponent = (component: ComponentType<object>, props: object): Ui =>
  Object.freeze({ kind: "component", component, props });
export const uiKeyed = (key: string, node: Ui): Ui => Object.freeze({ kind: "keyed", key, node });

interface ClassProps {
  readonly class_name: string;
}

export const uiDiv = (props: ClassProps, children: ReadonlyArray<Ui>): Ui =>
  element("div", { className: props.class_name }, {}, children);
export const uiSpan = (props: ClassProps, children: ReadonlyArray<Ui>): Ui =>
  element("span", { className: props.class_name }, {}, children);
export const uiParagraph = (children: ReadonlyArray<Ui>): Ui => element("p", {}, {}, children);
export const uiHeading = (children: ReadonlyArray<Ui>): Ui => element("h1", {}, {}, children);
export const uiList = (children: ReadonlyArray<Ui>): Ui => element("ul", {}, {}, children);
export const uiItem = (children: ReadonlyArray<Ui>): Ui => element("li", {}, {}, children);

interface ButtonProps {
  readonly disabled: boolean;
  readonly on_click: Handler;
}

export const uiButton = (props: ButtonProps, children: ReadonlyArray<Ui>): Ui =>
  element(
    "button",
    { type: "button", disabled: props.disabled },
    { onClick: { handler: props.on_click, snapshot: clickSnapshot } },
    children,
  );

interface InputProps {
  readonly value: string;
  readonly placeholder: string;
  readonly on_input: Handler;
}

/** `value` is the initial text: the input keeps what is typed while tasks run. */
export const uiInput = (props: InputProps): Ui =>
  element(
    "input",
    { defaultValue: props.value, placeholder: props.placeholder },
    { onChange: { handler: props.on_input, snapshot: inputSnapshot } },
    [],
  );

/** A text area; like `input`, `value` is the initial text. */
export const uiTextarea = (props: InputProps): Ui =>
  element(
    "textarea",
    { defaultValue: props.value, placeholder: props.placeholder, spellCheck: false },
    { onChange: { handler: props.on_input, snapshot: inputSnapshot } },
    [],
  );

export const uiPre = (props: ClassProps, children: ReadonlyArray<Ui>): Ui =>
  element("pre", { className: props.class_name }, {}, children);

export const uiLink = (props: { readonly href: string }, children: ReadonlyArray<Ui>): Ui =>
  element("a", { href: props.href }, {}, children);

export const uiHtml = (html: string): Ui => Object.freeze({ kind: "element", tag: "div", attrs: {}, events: {}, children: [], html });

interface SelectProps {
  readonly items: ReadonlyArray<unknown>;
  readonly selected: Option.Option<unknown>;
  readonly key: (item: unknown) => string;
  readonly render_item: (item: unknown) => Ui;
  readonly on_change: Handler;
}

/** A listbox: choosing an item runs `on_change` with it. */
export const uiSelect = (props: SelectProps): Ui => {
  const chosen = Option.map(props.selected, props.key);
  const options = props.items.map((item) => {
    const key = props.key(item);
    const selected = Option.isSome(chosen) && chosen.value === key;
    const option = element(
      "li",
      { role: "option", "aria-selected": selected },
      { onClick: { handler: props.on_change, snapshot: () => item } },
      [props.render_item(item)],
    );
    return uiKeyed(key, option);
  });
  return element("ul", { role: "listbox" }, {}, options);
};

interface PanelSlots {
  readonly header: Ui;
  readonly body: Ui;
  readonly footer: Option.Option<Ui>;
}

export const uiPanel = (slots: PanelSlots): Ui =>
  element("section", {}, {}, [
    element("header", {}, {}, [slots.header]),
    element("main", {}, {}, [slots.body]),
    uiOptional(Option.map(slots.footer, (footer) => element("footer", {}, {}, [footer]))),
  ]);

export const uiTab = (label: string, node: Ui): Tab => Object.freeze({ label, node });

export const uiTabs = (props: { readonly selected: number }, tabs: ReadonlyArray<Tab>): Ui => {
  const labels = tabs.map((tab, i) =>
    uiKeyed(String(i), element("li", { role: "tab", "aria-selected": i === props.selected }, {}, [uiText(tab.label)])),
  );
  const shown = tabs[props.selected];
  return element("div", {}, {}, [
    element("ul", { role: "tablist" }, {}, labels),
    element("div", { role: "tabpanel" }, {}, shown === undefined ? [] : [shown.node]),
  ]);
};

const SAFE_SCHEMES = new Set(["http", "https", "mailto"]);

/** `Ui.safe_url`: a relative URL, or one with a safe scheme. */
export const uiSafeUrl = (url: string): Option.Option<string> => {
  // browsers drop tabs and line breaks inside a URL: `java\tscript:` is a scheme
  // oxlint-disable-next-line no-control-regex -- the control characters are what is refused
  if (/[\u0000- \u007f]/.test(url)) return Option.none();
  const scheme = /^([a-zA-Z][a-zA-Z0-9+.-]*):/.exec(url)?.[1];
  return scheme === undefined || SAFE_SCHEMES.has(scheme.toLowerCase()) ? Option.some(url) : Option.none();
};

export const uiTrustedHtml = (html: string): string => html;

/** `Ui.container id`: the element of the document, or null. */
export const uiContainer = (id: string): Element | null => globalThis.document?.getElementById(id) ?? null;
