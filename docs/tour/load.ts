/** Build/CI-only reading of the tour's index; the order of the lessons lives in `index.json` alone. */
import { readFileSync } from "node:fs";
import path from "node:path";

export interface Chapter {
  readonly id: string;
  readonly en: string;
  readonly ru: string;
}
/** A lesson shows one program: its own file, a file of the gallery, or the starter of an exercise. */
export interface Lesson {
  readonly id: string;
  readonly chapter: Chapter;
  /** `run`, `ui`, or `fail` and the one code its errors have; empty for an exercise. */
  readonly expect: string;
  /** The program the lesson opens with; absent for an exercise, which brings its own. */
  readonly file?: string;
  /** What a `run` lesson prints, in both profiles. */
  readonly stdout?: string;
  readonly exercise?: string;
}

interface Entry {
  readonly id: string;
  readonly expect?: string;
  readonly example?: string;
  readonly exercise?: string;
}
interface Index {
  readonly chapters: ReadonlyArray<Chapter & { readonly lessons: ReadonlyArray<Entry> }>;
}

const read = <T>(file: string): T =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `test/tour.test.ts` checks the index against the files
  JSON.parse(readFileSync(file, "utf8")) as T;

/** Every lesson in the order of the tour. */
export const lessons = (): ReadonlyArray<Lesson> => {
  // From the site's root, as `exercises/load.ts` reads its files: prerendering moves this module.
  const site = process.cwd();
  const gallery = path.join(site, "src/playground/examples");
  const examples = new Map(
    read<{ groups: ReadonlyArray<{ examples: ReadonlyArray<{ id: string; expect: string }> }> }>(path.join(gallery, "index.json"))
      .groups.flatMap((group) => group.examples)
      .map((entry) => [entry.id, entry.expect]),
  );
  return read<Index>(path.join(site, "tour/index.json")).chapters.flatMap(({ lessons: entries, ...chapter }) =>
    entries.map((entry): Lesson => {
      if (entry.exercise !== undefined) return { id: entry.id, chapter, expect: "", exercise: entry.exercise };
      if (entry.example !== undefined && !examples.has(entry.example)) throw new Error(`tour lesson ${entry.id}: no gallery example ${entry.example}`);
      const [file, expect] = entry.example === undefined
        ? [path.join(site, "tour", chapter.id, `${entry.id}.fl`), entry.expect ?? ""]
        : [path.join(gallery, `${entry.example}.fl`), examples.get(entry.example) ?? ""];
      if (expect === "") throw new Error(`tour lesson ${entry.id}: no expectation`);
      return { id: entry.id, chapter, expect, file, ...(expect === "run" ? { stdout: file.replace(/\.fl$/, ".stdout") } : {}) };
    }),
  );
};

/** The lesson of a tour page, by its slug under `tour/`. */
export const lesson = (id: string): Lesson => {
  const found = lessons().find((entry) => entry.id === id);
  if (found === undefined) throw new Error(`No tour lesson ${id}`);
  return found;
};
