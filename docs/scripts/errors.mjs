// The compiler registry is the source of truth for the public error reference.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const explanations = path.join(root, "flick-core/src/diagnostics/explain");
const translations = path.join(root, "docs/src/error-translations");
const content = path.join(root, "docs/src/content/docs");
const registry = readFileSync(path.join(root, "flick-core/src/diagnostics/codes.rs"), "utf8");
const entries = [...registry.matchAll(/^\s*([A-Z]\d{3}) = (\w+) \d+, "([^"]+)";/gm)]
  .map(([, short, area, title]) => ({ short, code: `FLICK-${short}`, area, title }));
const areas = ["Syntax", "Resolve", "Types", "Effects", "Classes", "Regions", "Ffi", "Schema", "Sql", "Ui", "Cli", "Project"];
const areaRu = ["Синтаксис", "Имена и модули", "Типы", "Эффекты", "Классы", "Регионы", "Внешние вызовы", "Схемы", "SQL", "Интерфейс", "Командная строка", "Проект"];
const hash = (text) => createHash("sha256").update(text).digest("hex").slice(0, 16);
const page = (title, body, sourceHash) => `---\ntitle: ${JSON.stringify(title)}\n${sourceHash ? `source_hash: ${JSON.stringify(sourceHash)}\n` : ""}---\n\n${body.trim()}\n`;
const fences = (text) => text.match(/```[\s\S]*?```/g) ?? [];
const inline = (text) => new Set((text.replace(/```[\s\S]*?```/g, "").match(/`[^`\n]+`/g) ?? []));
// Diagnostic examples are fragments of a failing program, not standalone doctests.
const forDocs = (text) => text.replace(/^```flick\s*$/gm, "```flick ignore");

export function expectedPages() {
  const sourceFiles = readdirSync(explanations).filter((name) => /^[A-Z]\d{3}\.md$/.test(name)).sort();
  const registered = entries.map(({ short }) => `${short}.md`).sort();
  if (JSON.stringify(sourceFiles) !== JSON.stringify(registered)) throw new Error("Explanation files and diagnostic registry differ");
  const translated = existsSync(translations) ? readdirSync(translations).filter((name) => /^[A-Z]\d{3}\.md$/.test(name)).sort() : [];
  if (JSON.stringify(translated) !== JSON.stringify(registered)) throw new Error("Russian explanations and diagnostic registry differ");
  const pages = new Map();
  const add = (rel, text) => pages.set(path.join(content, rel), text);
  for (const { short, code } of entries) {
    const source = readFileSync(path.join(explanations, `${short}.md`), "utf8");
    const translated = readFileSync(path.join(translations, `${short}.md`), "utf8");
    if (JSON.stringify(fences(source)) !== JSON.stringify(fences(translated)) ||
        [...inline(source)].some((token) => !inline(translated).has(token))) {
      throw new Error(`${code}: Russian explanation changed a code example or identifier`);
    }
    const en = page(code, forDocs(source));
    const ru = page(code, forDocs(translated), hash(en));
    add(`reference/errors/${code.toLowerCase()}.md`, en);
    add(`ru/reference/errors/${code.toLowerCase()}.md`, ru);
  }
  const index = (ru) => areas.map((area, i) => {
    const rows = entries.filter((entry) => entry.area === area);
    return rows.length ? `## ${ru ? areaRu[i] : area}\n\n${rows.map(({ code }) => `- [${code}](./${code.toLowerCase()}/)`).join("\n")}` : "";
  }).filter(Boolean).join("\n\n");
  const enIndex = page("Compiler errors", "Every compiler code has an explanation and an example. Plugin codes belong to their packages and are explained with `flick explain <package>/<code>`.\n\n" + index(false));
  add("reference/errors/index.md", enIndex);
  add("ru/reference/errors/index.md", page("Ошибки компилятора", "Для каждого кода ошибки компилятора есть объяснение и пример. Коды плагинов принадлежат своим пакетам; их можно объяснить командой `flick explain <пакет>/<код>`.\n\n" + index(true), hash(enIndex)));
  return pages;
}

export function problems() {
  return [...expectedPages()].filter(([file, expected]) => !existsSync(file) || readFileSync(file, "utf8") !== expected).map(([file]) => path.relative(root, file));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const pages = expectedPages();
  if (process.argv.includes("--write")) {
    for (const [file, text] of pages) {
      mkdirSync(path.dirname(file), { recursive: true });
      writeFileSync(file, text);
    }
  } else {
    const stale = problems();
    if (stale.length) {
      console.error(`Regenerate error pages with node docs/scripts/errors.mjs --write:\n${stale.join("\n")}`);
      process.exitCode = 1;
    }
  }
}
