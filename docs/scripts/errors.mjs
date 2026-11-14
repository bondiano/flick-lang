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
// The public index follows `Area::ALL` in `flick-core/src/diagnostics/codes/code.rs`.
// `Area::Plugin` stays out of that array: a plugin code is spelled under its
// package and has no heading here. An area with no registered code is omitted
// from both localized indexes.
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
  const blocks = counted.fail + counted.valid + counted.ignore;
  const tally = `${shown} of the ${total} codes have an example, ${blocks} blocks in all: ${counted.fail} report the code of their page, ${counted.valid} are programs that check, and ${counted.ignore} are fragments the compiler cannot check, each with its reason in the repository (\`docs/error-examples.txt\`). The other ${counted.none} show a configuration or a shape instead.`;
  const tallyRu = `${shown} из ${total} кодов имеют пример, всего ${blocks} блоков: ${counted.fail} сообщают код своей страницы, ${counted.valid} — программы, которые проверяются, ${counted.ignore} — фрагменты, которые компилятор проверить не может, каждый с причиной в репозитории (\`docs/error-examples.txt\`). Остальные ${counted.none} показывают настройку или форму.`;
  const enIndex = page("Compiler errors", "Every compiler code has an explanation and an example. Plugin codes belong to their packages and are explained with `flick explain <package>/<code>`.\n\n" + tally + "\n\n" + index(false));
  add("reference/errors/index.md", enIndex);
  add("ru/reference/errors/index.md", page("Ошибки компилятора", "Для каждого кода ошибки компилятора есть объяснение и пример. Коды плагинов принадлежат своим пакетам; их можно объяснить командой `flick explain <пакет>/<код>`.\n\n" + tallyRu + "\n\n" + index(true), hash(enIndex)));
  return pages;
}

export function problems() {
  return [...expectedPages()].filter(([file, expected]) => !existsSync(file) || readFileSync(file, "utf8") !== expected).map(([file]) => path.relative(root, file));
}

// Names in `Area::ALL`, in source order. `Area::Plugin` is not one of them.
export function compilerAreas(source) {
  const found = source.match(/pub const ALL:\s*\[Area;\s*\d+\]\s*=\s*\[([\s\S]*?)\];/);
  if (!found) throw new Error("Area::ALL is not in the diagnostic source");
  const names = [...found[1].matchAll(/Area::([A-Za-z0-9_]+)/g)].map((match) => match[1]);
  if (!names.length) throw new Error("Area::ALL has no areas");
  return names;
}

const headingsOf = (markdown) => [...markdown.matchAll(/^## (.+)$/gm)].map((match) => match[1]);

const duplicates = (list) => {
  const seen = new Set();
  const repeated = [];
  for (const name of list) {
    if (seen.has(name)) repeated.push(name);
    seen.add(name);
  }
  return repeated;
};

// Drift between the compiler's areas, this script's headings and both
// generated indexes. An area with no code is left out of the indexes.
export function areaInventoryProblems({ compiler, areas: listed, areaRu: listedRu, headingsEn, headingsRu, codedAreas }) {
  const problems = [];
  for (const name of duplicates(compiler)) problems.push(`duplicate diagnostic area ${name}`);
  for (const name of duplicates(listed)) problems.push(`duplicate diagnostic area ${name}`);
  for (const name of duplicates(listedRu)) problems.push(`duplicate Russian diagnostic heading ${name}`);
  const compilerSet = new Set(compiler);
  const listedSet = new Set(listed);
  for (const name of compiler) if (!listedSet.has(name)) problems.push(`missing diagnostic area ${name}`);
  for (const name of listed) if (!compilerSet.has(name)) problems.push(`unknown diagnostic area ${name}`);
  const unique = new Set(compiler).size === compiler.length && new Set(listed).size === listed.length;
  const sameMembers = unique && compiler.length === listed.length && compiler.every((name) => listedSet.has(name));
  if (sameMembers && compiler.some((name, index) => listed[index] !== name)) {
    problems.push("diagnostic areas are out of compiler order");
  }
  if (listedRu.length !== listed.length) {
    problems.push(`Russian headings are ${listedRu.length} for ${listed.length} diagnostic areas`);
  }
  const expectEn = [];
  const expectRu = [];
  listed.forEach((name, index) => {
    if (!codedAreas.has(name)) return;
    expectEn.push(name);
    if (index < listedRu.length) expectRu.push(listedRu[index]);
  });
  if (JSON.stringify(headingsEn) !== JSON.stringify(expectEn)) {
    problems.push(`English index headings are ${JSON.stringify(headingsEn)}, not the areas that have codes ${JSON.stringify(expectEn)}`);
  }
  if (JSON.stringify(headingsRu) !== JSON.stringify(expectRu)) {
    problems.push(`Russian index headings are ${JSON.stringify(headingsRu)}, not the areas that have codes ${JSON.stringify(expectRu)}`);
  }
  return problems;
}

// The current compiler source, script lists and generated en/ru indexes.
export function diagnosticAreaProblems() {
  const source = readFileSync(path.join(root, "flick-core/src/diagnostics/codes/code.rs"), "utf8");
  const pages = expectedPages();
  const en = pages.get(path.join(content, "reference/errors/index.md"));
  const ru = pages.get(path.join(content, "ru/reference/errors/index.md"));
  if (!en || !ru) throw new Error("localized error indexes were not generated");
  return areaInventoryProblems({
    compiler: compilerAreas(source),
    areas,
    areaRu,
    headingsEn: headingsOf(en),
    headingsRu: headingsOf(ru),
    codedAreas: new Set(entries.map((entry) => entry.area)),
  });
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
