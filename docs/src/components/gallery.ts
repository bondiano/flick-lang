// The playground's gallery, read from `src/playground/examples/` when the page is built.
import { readFileSync } from "node:fs";
import path from "node:path";

interface Entry {
  readonly id: string;
  readonly en: string;
  readonly ru: string;
}

interface Group extends Entry {
  readonly examples: ReadonlyArray<Entry>;
}

export interface Example {
  readonly id: string;
  readonly group: string;
  readonly group_ru: string;
  readonly name: string;
  readonly name_ru: string;
  readonly source: string;
}

// a prerendered module does not run where it was written, so the files are found from the site's root
const directory = path.join(process.cwd(), "src/playground/examples");

/** Every example, in the order of the index, with its group's names. */
export const examples = (): Array<Example> => {
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the index is checked by `test/examples.test.ts`
  const index = JSON.parse(readFileSync(path.join(directory, "index.json"), "utf8")) as { groups: ReadonlyArray<Group> };
  return index.groups.flatMap((group) =>
    group.examples.map((entry) => ({
      id: entry.id,
      group: group.en,
      group_ru: group.ru,
      name: entry.en,
      name_ru: entry.ru,
      source: readFileSync(path.join(directory, `${entry.id}.fl`), "utf8"),
    })),
  );
};
