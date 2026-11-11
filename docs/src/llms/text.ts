/** The documentation as text for language models (llmstxt.org): what Flick is, a link to each page, and the whole tour. */
import { readFileSync } from "node:fs";
import { loadExercise } from "../../exercises/load.ts";
import { chapters, type Lesson } from "../../tour/load.ts";

export interface Page {
  readonly slug: string;
  readonly title: string;
  readonly description?: string;
  /** The MDX source of the page. */
  readonly body?: string;
}
interface Item {
  readonly slug?: string;
  readonly label?: string;
  readonly items?: ReadonlyArray<Item | string>;
}

const summary = `# Flick

> Flick is a language for whole apps, where every function does what it says.

A function's signature says what it needs and how it can fail, and the compiler holds its body to it.
Flick compiles to JavaScript and TypeScript, and to Go. It is alpha: host calls such as \`Console.log\`,
the clock and randomness are not named in signatures yet.`;

/** The slugs of a section of the sidebar, in its order. */
const slugs = (items: ReadonlyArray<Item | string>): ReadonlyArray<string> =>
  items.flatMap((item) => (typeof item === "string" ? [] : item.slug !== undefined ? [item.slug] : slugs(item.items ?? [])));

/** `llms.txt`: a section of links for each section of the sidebar; `url` turns a slug into an address. */
export const index = (sections: ReadonlyArray<Item>, pages: ReadonlyMap<string, Page>, url: (path: string) => string): string => {
  const parts = sections.map((section) => {
    const links = slugs(section.items ?? []).flatMap((slug) => {
      const page = pages.get(slug);
      if (page === undefined) return [];
      return [`- [${page.title}](${url(`${slug}/`)})${page.description ? `: ${page.description}` : ""}`];
    });
    return `## ${section.label ?? ""}\n\n${links.join("\n")}`;
  });
  return `${summary}\n\n${parts.join("\n\n")}\n\n## Optional\n\n- [The whole tour](${url("llms-full.txt")}): every lesson of the tour, its text and its program\n`;
};

/** The text of a lesson's page without its imports and the tags of its component. */
const prose = (body: string): string =>
  body
    .split("\n")
    .filter((line) => !/^import .* from ".*";$/.test(line) && !/^<\/?TourLesson\b.*>$/.test(line))
    .join("\n")
    .trim();

/** `llms-full.txt`: every lesson in the order of the tour, its text and then its program. */
export const tour = (lessons: ReadonlyArray<Lesson>, pages: ReadonlyMap<string, Page>): string => {
  const parts = chapters(lessons).map((chapter) => {
    const shown = lessons.filter((lesson) => lesson.chapter.id === chapter.id).map((lesson) => {
      const page = pages.get(`tour/${lesson.id}`);
      if (page === undefined) throw new Error(`The tour lesson ${lesson.id} has no page`);
      const program = lesson.file === undefined ? loadExercise(lesson.exercise ?? "").starter : readFileSync(lesson.file, "utf8");
      const kind = lesson.exercise === undefined ? "The program" : "The exercise starts from";
      return `### ${page.title}\n\n${prose(page.body ?? "")}\n\n${kind}:\n\n\`\`\`flick\n${program.trimEnd()}\n\`\`\``;
    });
    return `## ${chapter.en}\n\n${shown.join("\n\n")}`;
  });
  return `# The tour of Flick\n\nEvery lesson of the tour, in order: its text, then its program.\n\n${parts.join("\n\n")}\n`;
};
