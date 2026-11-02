// `Ui r e` values: inert descriptions of elements. Building one runs no
// handler and touches no DOM; the bridge turns them into React elements.
import { Effect, Option } from "effect";
import type * as Atom from "../../effect/reactivity/Atom.ts";
import type { ComponentType } from "react";

import { WriteService } from "../../effect/reactivity/Atom.ts";
import { routePrint, type UrlCodec } from "../../route.ts";
import { constructorName } from "../../tags.ts";
import { type Handler, eventLatest, eventPreventDefault } from "./policies.ts";

/** What an event prop of an element gives its handler. */
export interface Binding {
  readonly handler: Handler;
  /** An inert copy of what the handler may see of the event, taken at once. */
  readonly snapshot: (event: DomEvent) => unknown;
}

/** The part of a React synthetic event the bridge reads. */
export interface DomEvent {
  readonly type: string;
  readonly target: unknown;
  readonly button?: number;
  readonly altKey?: boolean;
  readonly ctrlKey?: boolean;
  readonly shiftKey?: boolean;
  readonly metaKey?: boolean;
  preventDefault(): void;
}

export type Attrs = Readonly<Record<string, string | boolean | number>>;

export type Ui =
  | { readonly kind: "text"; readonly value: string }
  | { readonly kind: "fragment"; readonly children: ReadonlyArray<Ui> }
  | { readonly kind: "keyed"; readonly key: string; readonly node: Ui }
  | {
      readonly kind: "component";
      readonly component: ComponentType<object>;
      readonly props: object;
      /** How a component of the runtime (a view, a field) renders without React, for `Ui.Server.to_html`. */
      readonly static?: Static;
      /** A component rendering one element (a field): the same one whose element's attributes go through `add`, for `Ui.with_host`. */
      readonly hosted?: (add: (attrs: Attrs) => Attrs) => Ui;
    }
  | {
      readonly kind: "element";
      readonly tag: string;
      readonly attrs: Attrs;
      readonly events: Readonly<Record<string, Binding>>;
      readonly children: ReadonlyArray<Ui>;
      /** Trusted HTML, inserted as it is. */
      readonly html?: string;
      /** `Ui.input_at element …` and the like: the element whose node this is. */
      readonly handle?: ElementHandle;
    };

/** `Ui.Element t`: the node of an element once React mounts it. */
export interface ElementHandle {
  node: unknown;
  /** The ref React gives the node, the same across renders. */
  readonly ref: (node: unknown) => void;
}

/** `Ui.handle`: a fresh element, bound to no node yet. */
export const uiElementHandle = (_unit: undefined): ElementHandle => {
  const handle: ElementHandle = {
    node: null,
    ref: (node) => {
      handle.node = node;
    },
  };
  return handle;
};

const at = (handle: ElementHandle, node: Ui): Ui => (node.kind === "element" ? Object.freeze({ ...node, handle }) : node);

/** A component's `Ui` for the props it was given, reading atoms through `read`. */
export type Static = (read: (atom: Atom.Atom<unknown>) => unknown) => Ui;

/** `Tab r e`. */
export interface Tab {
  readonly label: string;
  readonly node: Ui;
}

export const element = (
  tag: string,
  attrs: Record<string, string | boolean | number>,
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

export const inputSnapshot = (event: DomEvent): unknown => {
  const value: unknown = typeof event.target === "object" && event.target !== null ? Reflect.get(event.target, "value") : "";
  return Object.freeze({ value: typeof value === "string" ? value : "" });
};

export const uiText = (value: string): Ui => Object.freeze({ kind: "text", value });
export const uiFragment = (children: ReadonlyArray<Ui>): Ui => Object.freeze({ kind: "fragment", children });
export const uiOptional = (node: Option.Option<Ui>): Ui => (Option.isSome(node) ? node.value : uiFragment([]));
export const uiComponent = (component: ComponentType<object>, props: object, toUi?: Static): Ui =>
  Object.freeze(toUi === undefined ? { kind: "component", component, props } : { kind: "component", component, props, static: toUi });
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

export const uiNav = (props: ClassProps, children: ReadonlyArray<Ui>): Ui =>
  element("nav", { className: props.class_name }, {}, children);
export const uiFooter = (props: ClassProps, children: ReadonlyArray<Ui>): Ui =>
  element("footer", { className: props.class_name }, {}, children);
export const uiFieldset = (props: ClassProps, children: ReadonlyArray<Ui>): Ui =>
  element("fieldset", { className: props.class_name }, {}, children);
/** The `i` element of an icon font: its classes name the icon. */
export const uiIcon = (props: ClassProps): Ui => element("i", { className: props.class_name }, {}, []);
export const uiHr: Ui = /*#__PURE__*/ element("hr", {}, {}, []);

export const uiImage = (props: { readonly src: string; readonly alt: string; readonly class_name: string }): Ui =>
  element("img", { src: props.src, alt: props.alt, className: props.class_name }, {}, []);

/** A form: a submit runs `on_submit` and never reloads the page. */
export const uiForm = (props: { readonly on_submit: Handler }, children: ReadonlyArray<Ui>): Ui =>
  element(
    "form",
    {},
    { onSubmit: { handler: Object.freeze({ ...props.on_submit, prevent: () => true }), snapshot: () => undefined } },
    children,
  );

/** The button that submits the form around it. */
export const uiSubmit = (props: { readonly disabled: boolean }, children: ReadonlyArray<Ui>): Ui =>
  element("button", { type: "submit", disabled: props.disabled }, {}, children);

interface RouteLinkProps {
  readonly route: Atom.Writable<unknown>;
  readonly url: UrlCodec;
  readonly to: Readonly<Record<string, unknown>>;
}

/** A left click without a modifier key: what a browser follows in the same tab. */
const plainClick = (snapshot: unknown): boolean =>
  typeof snapshot === "object" &&
  snapshot !== null &&
  Reflect.get(snapshot, "button") === 0 &&
  !["alt", "ctrl", "shift", "meta"].some((key) => Reflect.get(snapshot, key) === true);

/**
 * A link inside the app: a plain click writes `to` to the route state, which
 * pushes its URL without a reload; another click, and markup without a
 * client, follow the real `href`.
 */
export const uiRouteLink = (props: RouteLinkProps, children: ReadonlyArray<Ui>): Ui => {
  const navigate = Effect.flatMap(Effect.service(WriteService), (write) => write.set("Ui.route_link")(props.route)(props.to));
  // a handler's task runs with the root's `Atom.Write`, as one of Flick's does
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the root provides the service
  const run = navigate as Effect.Effect<void>;
  const handler = eventPreventDefault(
    plainClick,
    eventLatest((snapshot) => (plainClick(snapshot) ? run : Effect.void)),
  );
  return element("a", { href: routePrint(props.url, props.to) }, { onClick: { handler, snapshot: clickSnapshot } }, children);
};

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

const SAFE_SCHEMES = /*#__PURE__*/ new Set(["http", "https", "mailto"]);

/** `Ui.safe_url`: a relative URL, or one with a safe scheme. */
export const uiSafeUrl = (url: string): Option.Option<string> => {
  // browsers drop tabs and line breaks inside a URL: `java\tscript:` is a scheme
  // oxlint-disable-next-line no-control-regex -- the control characters are what is refused
  if (/[\u0000- \u007f]/.test(url)) return Option.none();
  const scheme = /^([a-zA-Z][a-zA-Z0-9+.-]*):/.exec(url)?.[1];
  return scheme === undefined || SAFE_SCHEMES.has(scheme.toLowerCase()) ? Option.some(url) : Option.none();
};

export const uiTrustedHtml = (html: string): string => html;

interface Host {
  readonly id: Option.Option<string>;
  readonly class_name: Option.Option<string>;
  readonly role: Option.Option<string>;
  readonly aria_label: Option.Option<string>;
  readonly aria_describedby: Option.Option<string>;
  readonly aria_hidden: Option.Option<boolean>;
  readonly aria_invalid: Option.Option<boolean>;
  readonly aria_pressed: Option.Option<boolean>;
  readonly aria_live: Option.Option<{ readonly _tag: string }>;
  readonly tab_index: Option.Option<number>;
  readonly data: ReadonlyArray<readonly [string, string]>;
  readonly auto_capitalize: Option.Option<boolean>;
  readonly auto_correct: Option.Option<boolean>;
  readonly wrap: Option.Option<boolean>;
  readonly name: Option.Option<string>;
  readonly input_type: Option.Option<{ readonly _tag: string }>;
}

/** The `type` of an input of each `Ui.InputType`. */
const INPUT_TYPES: Readonly<Record<string, string>> = { TextInput: "text", PasswordInput: "password", EmailInput: "email" };

const onOff = (on: boolean): string => (on ? "on" : "off");

/** `attrs` with the host attributes `host` sets. */
const hostAttrs = (host: Host, base: Attrs): Attrs => {
  const attrs: Record<string, string | boolean | number> = { ...base };
  const put = <A>(name: string, value: Option.Option<A>, write: (a: A) => string | number): void => {
    if (Option.isSome(value)) attrs[name] = write(value.value);
  };
  put("id", host.id, String);
  put("className", host.class_name, String);
  put("role", host.role, String);
  put("aria-label", host.aria_label, String);
  put("aria-describedby", host.aria_describedby, String);
  put("aria-hidden", host.aria_hidden, String);
  put("aria-invalid", host.aria_invalid, String);
  put("aria-pressed", host.aria_pressed, String);
  put("aria-live", host.aria_live, (live) => constructorName(live._tag).toLowerCase());
  put("tabIndex", host.tab_index, (n) => n);
  put("autoCapitalize", host.auto_capitalize, onOff);
  put("autoCorrect", host.auto_correct, onOff);
  put("wrap", host.wrap, (wrap) => (wrap ? "soft" : "off"));
  put("name", host.name, String);
  put("type", host.input_type, (kind) => INPUT_TYPES[constructorName(kind._tag)] ?? "text");
  for (const [name, value] of host.data) {
    if (!/^[a-z][a-z0-9-]*$/.test(name)) throw new RangeError(`\`data-${name}\` is not an attribute name: use lowercase letters, digits and hyphens`);
    attrs[`data-${name}`] = value;
  }
  return Object.freeze(attrs);
};

/** `Ui.with_host`: the element, or the field, with these host attributes. */
export const uiWithHost = (host: Host, node: Ui): Ui => {
  if (node.kind === "component" && node.hosted !== undefined) return node.hosted((attrs) => hostAttrs(host, attrs));
  if (node.kind !== "element") throw new TypeError("host attributes go on an element, not on a " + node.kind);
  return Object.freeze({ ...node, attrs: hostAttrs(host, node.attrs) });
};

/** `Ui.heading_at`: a heading of the level `H1` … `H6`. */
export const uiHeadingAt = (level: { readonly _tag: string }, children: ReadonlyArray<Ui>): Ui =>
  element(constructorName(level._tag).toLowerCase(), {}, {}, children);

/** `Ui.input_at`. */
export const uiInputAt = (handle: ElementHandle, props: InputProps): Ui => at(handle, uiInput(props));

/** `Ui.textarea_at`. */
export const uiTextareaAt = (handle: ElementHandle, props: InputProps): Ui => at(handle, uiTextarea(props));

/** `Ui.button_at`. */
export const uiButtonAt = (handle: ElementHandle, props: ButtonProps, children: ReadonlyArray<Ui>): Ui =>
  at(handle, uiButton(props, children));

/** `Ui.div_at`. */
export const uiDivAt = (handle: ElementHandle, props: ClassProps, children: ReadonlyArray<Ui>): Ui => at(handle, uiDiv(props, children));
