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
// The example of a page is checked as the fence says: `flick` must check,
// `flick fail` must report the page's own code, `flick keep` must check
// unfixed, and `flick ignore` is a fragment the checker cannot compile,
// its reason reviewed in `docs/error-examples.txt`.
const forDocs = (text, code) =>
  text.replace(/^```flick(?: (.*))?$/gm, (_line, rest = "") => {
    const words = rest.trim();
    if (words === "") return "```flick";
    if (words === "fail") return `\`\`\`flick fail ${code.slice("FLICK-".length)}`;
    if (/^fail [A-Z]\d{3}( [A-Z]\d{3})*$/.test(words)) {
      const named = words.slice("fail ".length).split(" ");
      if (!named.includes(code.slice("FLICK-".length))) throw new Error(`${code}: a \`fail\` fence names the codes of the page: ${words}`);
      return `\`\`\`flick ${words}`;
    }
    if (words === "keep" || words.startsWith("keep ")) return `\`\`\`flick ${words}`;
    if (words === "ignore") return "```flick ignore";
    throw new Error(`${code}: a \`flick\` fence must be \`flick\`, \`flick fail\`, \`flick keep\` or \`flick ignore\`, not \`${words}\``);
  });

export function expectedPages() {
  const sourceFiles = readdirSync(explanations).filter((name) => /^[A-Z]\d{3}\.md$/.test(name)).sort();
  const registered = entries.map(({ short }) => `${short}.md`).sort();
  if (JSON.stringify(sourceFiles) !== JSON.stringify(registered)) throw new Error("Explanation files and diagnostic registry differ");
  const translated = existsSync(translations) ? readdirSync(translations).filter((name) => /^[A-Z]\d{3}\.md$/.test(name)).sort() : [];
  if (JSON.stringify(translated) !== JSON.stringify(registered)) throw new Error("Russian explanations and diagnostic registry differ");
  const pages = new Map();
  const add = (rel, text) => pages.set(path.join(content, rel), text);
  const counted = { fail: 0, valid: 0, ignore: 0, none: 0 };
  for (const { short, code } of entries) {
    const source = readFileSync(path.join(explanations, `${short}.md`), "utf8");
    const translated = readFileSync(path.join(translations, `${short}.md`), "utf8");
    if (JSON.stringify(fences(source)) !== JSON.stringify(fences(translated)) ||
        [...inline(source)].some((token) => !inline(translated).has(token))) {
      throw new Error(`${code}: Russian explanation changed a code example or identifier`);
    }
    const examples = fences(source)
      .map((fence) => fence.slice(0, fence.indexOf("\n")))
      .filter((line) => line.startsWith("```flick"))
      .map((line) => line.slice("```flick".length).trim());
    if (!examples.length) counted.none += 1;
    for (const words of examples) {
      if (words === "fail" || words.startsWith("fail ")) counted.fail += 1;
      else if (words === "ignore" || words.startsWith("ignore ")) counted.ignore += 1;
      else counted.valid += 1;
    }
    const en = page(code, forDocs(source, code));
    const ru = page(code, forDocs(translated, code), hash(en));
    add(`reference/errors/${code.toLowerCase()}.md`, en);
    add(`ru/reference/errors/${code.toLowerCase()}.md`, ru);
  }
  const index = (ru) => areas.map((area, i) => {
    const rows = entries.filter((entry) => entry.area === area);
    return rows.length ? `## ${ru ? areaRu[i] : area}\n\n${rows.map(({ code }) => `- [${code}](./${code.toLowerCase()}/)`).join("\n")}` : "";
  }).filter(Boolean).join("\n\n");
  const total = entries.length;
  const shown = total - counted.none;
  const tally = `${shown} of the ${total} codes have an example: ${counted.fail} report the code of their page, ${counted.valid} are programs that check, and ${counted.ignore} are fragments the compiler cannot check, each with its reason in the repository (\`docs/error-examples.txt\`). The other ${counted.none} show a configuration or a shape instead.`;
  const tallyRu = `${shown} из ${total} кодов имеют пример: ${counted.fail} сообщают код своей страницы, ${counted.valid} — программы, которые проверяются, ${counted.ignore} — фрагменты, которые компилятор проверить не может, каждый с причиной в репозитории (\`docs/error-examples.txt\`). Остальные ${counted.none} показывают настройку или форму.`;
  const enIndex = page("Compiler errors", "Every compiler code has an explanation and an example. Plugin codes belong to their packages and are explained with `flick explain <package>/<code>`.\n\n" + tally + "\n\n" + index(false));
  add("reference/errors/index.md", enIndex);
  add("ru/reference/errors/index.md", page("Ошибки компилятора", "Для каждого кода ошибки компилятора есть объяснение и пример. Коды плагинов принадлежат своим пакетам; их можно объяснить командой `flick explain <пакет>/<код>`.\n\n" + tallyRu + "\n\n" + index(true), hash(enIndex)));
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
