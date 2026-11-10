import type { Exercise } from "../../exercises/load.ts";
import type { Configuration } from "./State.fl";

export { attributes } from "../widgets/dom.ts";

export const lessonData = (): Omit<Exercise, "starter"> | undefined => {
  const text = document.getElementById("flick-lesson")?.textContent;
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the build validates and embeds this data
  return text ? JSON.parse(text) as Omit<Exercise, "starter"> : undefined;
};
export const lessonKey = (kind: string): string => `flick.lesson.${(document.getElementById("flick-playground")?.dataset.lesson ?? "").replaceAll("/", ".")}.${kind}`;
export const completeLesson = (): void => {
  try { localStorage.setItem(lessonKey("done"), "1"); } catch { /* local progress is optional */ }
};

/** Page-provided configuration; decoding and all locale behavior belong to Flick. */
export const configuration = (): Configuration => {
  const root = document.getElementById("flick-playground");
  if (root === null) throw new Error("The page has no flick-playground container");
  const embedded = document.getElementById("flick-playground-examples")?.textContent ?? "[]";
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the page writes this JSON from the gallery's index
  const examples = JSON.parse(embedded) as Configuration["examples"];
  let done = false;
  try { done = localStorage.getItem(lessonKey("done")) === "1"; } catch { /* local progress is optional */ }
  const exercise = lessonData();
  return {
    locale: root.dataset.locale ?? "",
    examples,
    lesson: exercise !== undefined,
    solution: exercise?.solution ?? "",
    lesson_title: exercise?.title ?? "",
    lesson_title_ru: exercise?.title_ru ?? "",
    lesson_task: exercise?.task ?? "",
    lesson_task_ru: exercise?.task_ru ?? "",
    lesson_hint_text: exercise?.hint ?? "",
    lesson_hint_text_ru: exercise?.hint_ru ?? "",
    done,
  };
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
