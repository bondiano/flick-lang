// Writes runtime-go/flickrt/testdata/glob.txt: what Effect's
// `FileSystem.glob` (internal/glob.ts over NodeFileSystem) gives for each
// case of a corpus, run in a fixture tree. The first line is `tree`, a tab
// and the tree as JSON; every other line is a case as JSON (`pattern` and
// the `root`, `exclude` and `dot` options), a tab, then the paths as JSON
// in Effect's order or `error: <reason> <method> <syscall> <path>`.
// `<abs>` stands for the fixture's absolute path. TestFileSystemGlobAsEffect
// rebuilds the tree and compares the go runtime's port with these lines.
// Run from the repository root: `node tools/go-glob/gen.mjs`.
import { chmodSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { Effect, Exit, FileSystem } from "../../runtime/node_modules/effect/dist/index.js";
import { layer } from "../../runtime/node_modules/@effect/platform-node/dist/NodeFileSystem.js";

const out = new URL("../../runtime-go/flickrt/testdata/glob.txt", import.meta.url);

// a path is a file, ["d", path, mode?] a directory (mode set last),
// ["l", path, target] a symbolic link
const names = [
  "a.txt", "b.txt", "c.md", "ab", "abc", "abcd", "a1", "a2", "a10", "9lives", "_u", "-dash", "x.y.z", ".dot",
  "Upper.TXT", "space name.txt", "file[1].txt", "brace{x}.txt", "comma,name.txt", "@(a|b).txt", "!a.txt", "#tag",
  "é.txt", "日本.txt", "😀.txt", "\uFF01.txt", "ω",
];
const tree = [
  "a.txt", "b.txt", "c.md", ".hidden", ".hidden.txt", "notdir.txt",
  ...names.map((p) => `names/${p}`),
  "sub/one.txt", "sub/two.md", "sub/.dot.txt", "sub/a.txt", "sub/deep/three.txt",
  "sub/deep/.cache/x.txt", "sub/deep/deeper/four.txt",
  ["l", "sub/up", ".."],
  ".git/config", ".git/objects/o.txt", "dir.d/f.txt",
  ["d", "empty"],
  "node/a.txt", "node/b.txt", "node/hard.txt", "node/tree/one.txt", "node/tree/sub/deep.txt",
  "node/copied/one.txt", "node/copied/sub/deep.txt",
  "plain/a.txt", "plain/b.txt", "plain/d.txt", "plain/hard.txt",
  ["l", "plain/soft.txt", "a.txt"],
  ["l", "linkdir", "sub"],
  ["l", "linkfile.txt", "a.txt"],
  ["l", "broken.txt", "missing.txt"],
  ["l", "loop", "loop"],
  "locked/inner.txt",
  "noread/f.txt",
  ["d", "locked", 0o000],
  ["d", "noread", 0o311],
];

// run with `names` as the root
const named = [
  "*", "?.txt", "??.txt", "a?", "a*", "a*b*", "*c*", "a*c", "*b*c*d", "*.*.*", "space*",
  "a[0-9]", "a[0-9][0-9]", "a[^0-9]*", "[[:digit:]]*", "a[[:digit:]]", "a[[:alpha:]]*", "[[:upper:]]*",
  "[[:punct:]]*", "[[:bogus:]]*", "[[:alpha:]-z]*", "[]]*", "[!]a-z]*", "[a-]*", "[-a]*", "[z-a]*", "[\\]]*",
  "[[:alnum:][:punct:]][[:alnum:][:punct:]]", "[a-c][!a-z]*", "[!a-z]*", "[.]*", "[[:space:]]*", "*[[:space:]]*",
  "[[:xdigit:]][[:lower:]]*", "[[:word:]][[:word:]]", "[a-[:digit:]]*", "[[:digit:]-a]*", "[[:digit]*", "[[.]*",
  "file\\[1\\].txt", "file[[]1].txt", "file[1].txt", "file[1*", "file[*", "[*", "*]*", "[a/b]*", "file[1]*",
  "brace{x}.txt", "brace\\{x\\}.txt", "brace*", "{a,b}.txt", "{a,b,c}.*", "{a,{b,c}}.txt", "{a..c}.txt",
  "a.{txt,md}", "{*.md,a?}", "{a,b", "a,b}", "{a}.txt", "{}", "{,}", "{,a}.txt", "{a,}b*", "comma,name.txt",
  "{comma\\,name,a}.txt", "comma{\\,,}name.txt", "[{,}]*", "{[a,b]}*", "{a,{b}}.txt", "{a{b,c}}", "{{a,b}.txt",
  "{a,b}}.txt", "{a,b}{c,.}*", "@(a|b).txt", "?(a).txt", "!(a).txt", "!a.txt", "+(a|b).txt", "*(a|b).txt", "#*",
  "!*", "@*", "é.txt", "😀*", "[😀].txt", "[é-ê].txt", "*\uFF01*", "日*", "?", "ω", "[ω]", "\\*", "a\\",
  "\\a.txt", "a\\.txt", "[a]\\.txt", ".*",
  // past 10,000 alternatives braces stay literal
  `{a,b}.txt${"{,}".repeat(13)}`, `{a,b}.txt${"{,}".repeat(12)}`, "\\.*", ".?ot", "[.]dot", "*dot", "{.dot,a.txt}",
];

const patterns = [
  "", "*", "*.txt", "**", "*/", "**/", "**a",
  "sub/**", "sub/**/", "sub/**/*.txt", "sub/*/", "sub/*", "**/deep/**", "**/*.md",
  "**/sub/**/**/*.txt", "**/one.txt", "*/one.txt", "*/a.txt",
  ".*", ".*/**", "**/.*", "**/.git/**", ".git/*", "sub/.*", "sub/deep/.cache/*", "**/.cache/*",
  "{sub,node}/*.txt", "{sub/one,node/a}.txt", "{.hidden,a.txt}", "{a,b}/{c,d}", "{**,sub}/*.md",
  ".", "./", "./*.txt", "./sub/../a.txt", "sub/../a.txt", "sub/..", "..", "**/..", "**/.", "sub/**/..",
  "sub/.", "sub/./one.txt", "sub//one.txt", "./**/one.txt", "/", "//server/share", "C:/x",
  "<abs>/a.txt", "<abs>/*.txt", "<abs>/sub/**/*.txt", "<abs>/sub/", "<abs>/{a,b}.txt", "{<abs>/a,b}.txt",
  "a.txt", "a.txt/", "sub/", "sub", "nope", "nope/*", "nope/**", "a.txt/*", "a.txt/x", "notdir.txt/**",
  "linkdir", "linkdir/", "linkdir/*", "linkdir/**", "linkdir/**/*.txt", "*/deep/*.txt", "**/three.txt",
  "linkfile.txt", "linkfile.txt/", "broken.txt", "broken*", "broken.txt/", "loop", "loop/", "loop*", "loop/*",
  "sub/up/a.txt", "sub/up/*.md", "sub/up/sub/up/a.txt", "sub/*/a.txt", "sub/up/sub/*",
  "locked/*", "locked/inner.txt", "locked", "locked/", "*/inner.txt", "**/inner.txt", "locked/**",
  "noread/f.txt", "noread/*", "*/f.txt", "**/f.txt", "noread/**", "noread/x/**",
  "empty/", "empty/*", "empty/**", "dir.d/*", "*.d/", "sub\\/one.txt",
];

const cases = [
  ...named.map((pattern) => ({ pattern, root: "names" })),
  ...patterns.map((pattern) => ({ pattern })),
  ...[".*", "**/.*", "*/", "**/..", ".git/*", ".git/**"].map((pattern) => ({ pattern, dot: true })),
  ...["*t", "?dot*", "[.]*", "*dot"].map((pattern) => ({ pattern, root: "names", dot: true })),
  ...["*", "**", "../*.txt", "deep/**", "./*", "..", "up/*.md"].map((pattern) => ({ pattern, root: "sub" })),
  ...["*", "**", "**/*.txt", "**/**/*.txt"].map((pattern) => ({ pattern, root: "sub", dot: true })),
  { pattern: "**/**/*.txt", root: "sub" },
  { pattern: "*", root: "sub/" },
  { pattern: "**/*.txt", root: "./sub" },
  { pattern: "*", root: "nope" },
  { pattern: "*", root: "a.txt" },
  { pattern: "*", root: "locked" },
  { pattern: "inner.txt", root: "locked" },
  { pattern: "*", root: "noread" },
  { pattern: "f.txt", root: "noread" },
  { pattern: "**", root: "linkdir" },
  { pattern: "<abs>/a.txt", root: "sub" },
  { pattern: "*", root: "<abs>/sub" },
  { pattern: "**/*.txt", root: "node/tree" },
  { pattern: "{a,b}.txt", root: "node" },
  { pattern: "@(a|b).txt", root: "node" },
  { pattern: "**/*.txt", root: "node/tree", exclude: ["sub/**"] },
  { pattern: "*.txt", root: "plain", exclude: ["b.txt"] },
  { pattern: "**/*.txt", root: "sub", exclude: ["deep"] },
  { pattern: "**/*.txt", root: "sub", exclude: ["deep/"] },
  { pattern: "**/*.txt", root: "node", exclude: ["**/sub"] },
  { pattern: "**", root: "sub", exclude: ["*.md", "**/*.md"] },
  { pattern: "**", exclude: ["**/.git/**", "names", "node", "sub"], dot: true },
  { pattern: "**", root: "sub", exclude: [".*"], dot: true },
  { pattern: "sub/**", exclude: ["sub/**"] },
  { pattern: "sub/**", exclude: ["sub/*/"] },
  { pattern: "*.txt", exclude: ["{a,b}.txt"] },
  { pattern: "*", root: "sub", exclude: ["."] },
  { pattern: "*", root: "sub", exclude: ["nope"] },
  { pattern: "*.txt", exclude: ["a.txt/"] },
  { pattern: "*", exclude: ["linkdir/", "*.*"] },
  { pattern: "**/*.txt", exclude: ["*/deep/**", "node", "names"] },
  { pattern: "*", exclude: ["*"] },
  { pattern: "*", root: "sub", exclude: [""] },
  { pattern: "*/", exclude: ["**/"] },
  { pattern: "**/*.txt", root: "sub", exclude: ["./deep"] },
  { pattern: "**/*.txt", exclude: ["locked", "noread", "names", "node", "sub"] },
  { pattern: "<abs>/sub/*", exclude: ["<abs>/sub/one.txt"] },
  { pattern: "<abs>/sub/*", exclude: ["sub/one.txt"] },
  { pattern: "*", root: "sub", exclude: ["deep"] },
  { pattern: "locked/*", exclude: ["locked"] },
];

const fixture = mkdtempSync(join(tmpdir(), "flick-glob-"));
const run = (c) => {
  const abs = (text) => text.replaceAll("<abs>", fixture);
  const options = { root: c.root === undefined ? undefined : abs(c.root), exclude: c.exclude?.map(abs), dot: c.dot };
  return Effect.runPromiseExit(
    Effect.provide(Effect.flatMap(FileSystem.FileSystem, (fs) => fs.glob(abs(c.pattern), options)), layer),
  );
};
const show = (exit) => {
  if (Exit.isSuccess(exit)) return JSON.stringify(exit.value);
  const error = exit.cause.reasons.find((reason) => reason._tag === "Fail")?.error;
  if (error?.reason === undefined) throw new Error(`unexpected failure: ${exit.cause}`);
  const { _tag, method, syscall, pathOrDescriptor } = error.reason;
  return `error: ${_tag} ${method} ${syscall} ${pathOrDescriptor}`;
};

const lines = [`tree\t${JSON.stringify(tree)}`];
const cwd = process.cwd();
try {
  for (const entry of tree) {
    const [kind, path, extra] = typeof entry === "string" ? ["f", entry] : entry;
    const at = join(fixture, path);
    if (kind === "d" && extra !== undefined) chmodSync(at, extra);
    else if (kind === "d") mkdirSync(at, { recursive: true });
    else {
      mkdirSync(dirname(at), { recursive: true });
      if (kind === "f") writeFileSync(at, path);
      else symlinkSync(extra, at);
    }
  }
  process.chdir(fixture);
  for (const c of cases) {
    lines.push(`${JSON.stringify(c)}\t${show(await run(c)).replaceAll(fixture, "<abs>")}`);
  }
} finally {
  process.chdir(cwd);
  for (const [kind, path, mode] of tree.filter(Array.isArray)) if (kind === "d" && mode !== undefined) chmodSync(join(fixture, path), 0o700);
  rmSync(fixture, { recursive: true, force: true });
}
writeFileSync(out, `${lines.join("\n")}\n`);
console.log(`${cases.length} cases`);
