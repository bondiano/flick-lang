export { attributes } from "../widgets/dom.ts";

/** The examples of the gallery, as the page embeds them. */
interface Example {
  readonly id: string;
  readonly group: string;
  readonly group_ru: string;
  readonly name: string;
  readonly name_ru: string;
  readonly source: string;
}

/** Page-provided configuration; decoding and all locale behavior belong to Flick. */
export const configuration = (): { locale: string; examples: ReadonlyArray<Example> } => {
  const root = document.getElementById("flick-playground");
  if (root === null) throw new Error("The page has no flick-playground container");
  const embedded = document.getElementById("flick-playground-examples")?.textContent ?? "[]";
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the page writes this JSON from the gallery's index
  const examples = JSON.parse(embedded) as ReadonlyArray<Example>;
  return { locale: root.dataset.locale ?? "", examples };
};

/** The tab a link names in its query, such as the old Transpile page's `?tab=js`. */
export const tabInAddress = (): string => new URLSearchParams(location.search).get("tab") ?? "";

/** Read native text entered before hydration; Flick adopts it into its model. */
export const sourceValue = (): string => {
  const editor = document.querySelector<HTMLTextAreaElement>("#flick-playground textarea");
  if (editor === null) throw new Error("The playground source editor is missing");
  return editor.value;
};

/** Queue a host event so a mounted Flick root can render its changed Ref. */
export const notify = (id: string): void => {
  queueMicrotask(() => document.getElementById(id)?.click());
};
