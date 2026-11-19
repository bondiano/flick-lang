// The snapshots of `Test.to.match_snapshot`, whatever runtime runs the tests.
//
// `flick test` writes `snapshots.json` beside the runner, an object whose keys
// are the module names the runner gives `runTests` and whose values are the
// paths of their snapshot files, relative to the manifest, and passes it as
// `--snapshots <path>`. A module's file is `__snapshots__/<File>.snap` beside
// its source: UTF-8 text with LF line ends, where an entry is a header line
// `<full test name> » <snapshot name>`, then each line of the snapshot after
// two spaces (an empty line stays empty), and entries are apart by a blank
// line, sorted by their header's UTF-8 bytes. No run writes it without
// `--update-snapshots`. The Go runtime reads and writes the same files.

/** What a test's snapshot holds for a text: the same, nothing, or another text. */
export type Stored = Readonly<{ kind: "same" }> | Readonly<{ kind: "missing" }> | Readonly<{ kind: "other"; text: string }>;

/** The snapshots of one test: compares `text` with its snapshot `name`. */
export type SnapshotCase = (name: string, text: string) => Stored;

/** What a run did to its snapshot files, for the report. */
export interface SnapshotSummary {
  readonly written: number;
  readonly removed: number;
  /** The entries no test checks any more and that stay in their files, as `file: header`. */
  readonly obsolete: ReadonlyArray<string>;
}

const SAME: Stored = Object.freeze({ kind: "same" });
const MISSING: Stored = Object.freeze({ kind: "missing" });

const fs = () => process.getBuiltinModule("node:fs");
const path = () => process.getBuiltinModule("node:path");

/** The entries of a snapshot file, by header. */
export const parseSnapshots = (text: string): Map<string, string> => {
  const entries = new Map<string, string>();
  let header: string | undefined;
  let body: Array<string> = [];
  const end = (): void => {
    if (header === undefined) return;
    // the blank line after an entry ends it
    if (body.at(-1) === "") body.pop();
    entries.set(header, body.join("\n"));
  };
  const lines = text.replaceAll("\r\n", "\n");
  for (const line of (lines.endsWith("\n") ? lines.slice(0, -1) : lines).split("\n")) {
    if (line === "" || line.startsWith("  ")) {
      body.push(line.slice(2));
      continue;
    }
    end();
    header = line;
    body = [];
  }
  end();
  return entries;
};

const byBytes = (a: string, b: string): number => Buffer.compare(Buffer.from(a), Buffer.from(b));

/** The text of a snapshot file with `entries`. */
export const renderSnapshots = (entries: ReadonlyMap<string, string>): string =>
  [...entries.keys()]
    .sort(byBytes)
    .map((header) => `${header}\n${(entries.get(header) ?? "").split("\n").map((line) => (line === "" ? "" : `  ${line}`)).join("\n")}\n`)
    .join("\n");

interface SnapshotFile {
  readonly path: string;
  entries: Map<string, string> | undefined;
  /** The text each header was checked with in this run. */
  readonly checked: Map<string, string>;
  written: number;
}

const entriesOf = (file: SnapshotFile): Map<string, string> => {
  if (file.entries !== undefined) return file.entries;
  let text = "";
  try {
    text = fs().readFileSync(file.path, "utf8");
  } catch (error) {
    if (Reflect.get(Object(error), "code") !== "ENOENT") throw error;
  }
  file.entries = parseSnapshots(text);
  return file.entries;
};

/** Whether `header` is that of a snapshot of one of `tests`, by their full names. */
const ofOneOf = (header: string, tests: ReadonlySet<string>): boolean => {
  for (let at = header.indexOf(" » "); at >= 0; at = header.indexOf(" » ", at + 1)) {
    if (tests.has(header.slice(0, at))) return true;
  }
  return false;
};

/** The snapshot files of a run, by module, from the manifest at `manifest`. */
export class Snapshots {
  readonly #files = new Map<string, SnapshotFile>();
  readonly #update: boolean;

  constructor(manifest: string, update: boolean) {
    const paths: unknown = JSON.parse(fs().readFileSync(manifest, "utf8"));
    const dir = path().dirname(manifest);
    for (const [module, file] of Object.entries(Object(paths))) {
      if (typeof file !== "string") continue;
      this.#files.set(module, { path: path().resolve(dir, file), entries: undefined, checked: new Map(), written: 0 });
    }
    this.#update = update;
  }

  /** The snapshots of the test `test` of `module`, if the manifest has its file. */
  of(module: string, test: string): SnapshotCase | undefined {
    const file = this.#files.get(module);
    if (file === undefined) return undefined;
    return (name, text) => {
      if (test.includes("\n") || name.includes("\n")) throw new Error(`a snapshot name, and the name of its test, cannot hold a line break: ${JSON.stringify(name)}`);
      const header = `${test} » ${name}`;
      // a name checked again in a run is compared with the text it was checked with first
      const seen = file.checked.get(header);
      if (seen !== undefined) return seen === text ? SAME : { kind: "other", text: seen };
      file.checked.set(header, text);
      const entries = entriesOf(file);
      const stored = entries.get(header);
      if (stored === text) return SAME;
      if (!this.#update) return stored === undefined ? MISSING : { kind: "other", text: stored };
      entries.set(header, text);
      file.written++;
      return SAME;
    };
  }

  /**
   * After the run: the entries no test checks any more, those of a test the
   * module no longer has or one that passed without checking them; with
   * `--update-snapshots` and no `--filter` they are removed, and each file
   * that changed is written, or removed when it is left empty. `unfinished`
   * are the full names of the tests that did not pass in the run, whose
   * entries are never obsolete.
   */
  finish(unfinished: ReadonlySet<string>, filtered: boolean): SnapshotSummary {
    let written = 0;
    let removed = 0;
    const obsolete: Array<string> = [];
    for (const [, file] of [...this.#files].sort(([a], [b]) => byBytes(a, b))) {
      const entries = entriesOf(file);
      const shown = path().relative(process.cwd(), file.path).split(path().sep).join("/");
      let changed = file.written > 0;
      for (const header of [...entries.keys()].sort(byBytes)) {
        if (file.checked.has(header) || ofOneOf(header, unfinished)) continue;
        if (this.#update && !filtered) {
          entries.delete(header);
          removed++;
          changed = true;
        } else obsolete.push(`${shown}: ${header}`);
      }
      written += file.written;
      if (!this.#update || !changed) continue;
      if (entries.size === 0) fs().rmSync(file.path, { force: true });
      else {
        fs().mkdirSync(path().dirname(file.path), { recursive: true });
        fs().writeFileSync(file.path, renderSnapshots(entries));
      }
    }
    return { written, removed, obsolete };
  }
}
