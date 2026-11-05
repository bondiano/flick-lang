import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { hash, problems, stamp } from "../scripts/parity.mjs";

const docs = fileURLToPath(new URL("../src/content/docs/", import.meta.url));

test("every translation of the site records the hash of its original", () => {
  assert.deepEqual(problems(docs), []);
});

test("each kind of problem is reported, and a stamp settles a translation", () => {
  const root = mkdtempSync(path.join(tmpdir(), "flick-parity-"));
  try {
    const write = (file: string, text: string) => {
      mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
      writeFileSync(path.join(root, file), text);
    };
    const page = (title: string, extra = "") => `---\ntitle: ${title}\n${extra}---\n\nText of ${title}.\n`;
    const of = (file: string) => `source_hash: ${hash(readFileSync(path.join(root, file)))}\n`;
    write("synced.md", page("Synced"));
    write("ru/synced.md", page("Синхронно", of("synced.md")));
    write("ru/spec/chapter.md", page("Глава"));
    write("spec/chapter.md", page("Chapter", of("ru/spec/chapter.md")));
    write("alone.md", page("Alone"));
    write("ru/stray.md", page("Без пары"));
    write("unstamped.md", page("Unstamped"));
    write("ru/unstamped.md", page("Без отметки"));
    write("stamped.md", page("Stamped", "source_hash: 0123456789abcdef\n"));
    write("ru/stamped.md", page("С отметкой", of("stamped.md")));
    write("stale.md", page("Stale"));
    write("ru/stale.md", page("Устарело", "source_hash: 0123456789abcdef\n"));
    const found = problems(root);
    assert.deepEqual(
      found.map((line) => line.split(": ")[0]),
      ["alone.md", "ru/stale.md", "stamped.md", "ru/stray.md", "ru/unstamped.md"],
    );
    assert.match(found[1] ?? "", /changed since it was translated.*--stamp \S*ru\/stale\.md$/);
    assert.match(found[2] ?? "", /an original records no source_hash/);
    assert.match(found[4] ?? "", /no source_hash of unstamped\.md/);

    stamp(root, path.join(root, "ru/stale.md"));
    stamp(root, path.join(root, "ru/unstamped.md"));
    assert.equal(readFileSync(path.join(root, "ru/unstamped.md"), "utf8"), page("Без отметки", of("unstamped.md")));
    assert.deepEqual(
      problems(root).map((line) => line.split(": ")[0]),
      ["alone.md", "stamped.md", "ru/stray.md"],
    );
    assert.throws(() => stamp(root, path.join(root, "synced.md")), /not a translation/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
