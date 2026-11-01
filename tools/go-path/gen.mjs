// Writes runtime-go/flickrt/testdata/path_win32{,_more}.txt: what Node's
// `path.win32`, and `fileURLToPath`/`pathToFileURL` with `windows`, give
// for a corpus of Windows paths. TestPathWin32AsNode compares the go
// runtime's win32 paths with these, one JSON line each: the operation,
// its arguments, and the result (`null` for a thrown error). Inputs that
// resolve against the working directory are left out: it differs here.
// Run from the repository root: `node tools/go-path/gen.mjs`.
import { writeFileSync } from "node:fs";
import { win32 } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// a path's parts in one file, the rest in the other, each a size Git reviews
const out = "runtime-go/flickrt/testdata/path_win32.txt";
const more = "runtime-go/flickrt/testdata/path_win32_more.txt";

const paths = [
  "", ".", "..", "\\", "/", "a", "a\\b", "a/b", "a\\b\\", "a/b/", "\\a", "/a", "\\\\", "//",
  "C:", "C:\\", "C:/", "c:a", "C:a\\b", "C:\\a", "C:/a/b", "C:\\a\\b\\", "C:\\a\\..\\..\\b",
  "C:\\a\\.\\b", "C:\\\\a\\\\b", "C:..\\a", "a\\..\\..\\b", "..\\..\\a", ".\\a", "a\\.",
  "\\\\server", "\\\\server\\", "\\\\server\\share", "\\\\server\\share\\", "//server/share/x/y",
  "\\\\server\\share\\a\\..\\b", "\\\\?\\C:\\a", "\\\\.\\PHYSICALDRIVE0", "\\\\.\\pipe\\x",
  "\\\\?\\COM1:", "\\\\?\\UNC\\server\\share\\a", "\\\\\\server\\share",
  "CON", "CON:", "con:x", "COM1:", "LPT¹:", "NUL:\\a", "AUX:x\\y", "a:b", "a\\b:c", "a:\\b", "x:", "1:\\a",
  "file.txt", "file.tar.gz", ".bashrc", "..txt", "a.", "C:\\a\\b.txt", "C:\\a\\.b", "C:\\a\\b.",
  "C:\\dir.x\\file", "\\\\server\\share\\f.txt", "a\\b\\..\\c.d", "über\\ñ.txt", "C:\\ünï\\é", "a\\😀\\b",
];
const absolute = paths.filter((p) => win32.isAbsolute(p) && !/^[\\/](?![\\/])/.test(p));
// pairs of these, as every pair of `absolute` would be a large file
const some = ["C:\\", "C:\\a", "C:/a/b/", "c:\\A\\c", "D:\\a", "\\\\server\\share", "//server/share/x/y", "\\\\?\\C:\\a"];

const cwd = process.cwd().replaceAll("/", "\\");
const lines = [];
const record = (op, args, f) => {
  let result;
  try {
    result = f();
  } catch {
    result = null;
  }
  lines.push(JSON.stringify([op, args, result]));
};

for (const p of paths) {
  record("normalize", [p], () => win32.normalize(p));
  record("isAbsolute", [p], () => win32.isAbsolute(p));
  record("dirname", [p], () => win32.dirname(p));
  record("basename", [p], () => win32.basename(p));
  for (const s of [".txt", "😀\\b"]) record("basename", [p, s], () => win32.basename(p, s));
  record("extname", [p], () => win32.extname(p));
}
const parts = lines.splice(0);
for (const p of paths) {
  record("parse", [p], () => win32.parse(p));
  record("format", [p], () => win32.format(win32.parse(p)));
}
const joins = [
  [], [""], ["a", "b"], ["a", "", "b"], ["\\\\server", "share"], ["//server", "share", "x"], ["\\", "\\a"],
  ["\\\\\\a", "b"], ["C:", "a"], ["C:\\", "..", "b"], ["a", "CON:", "b"], ["a/b", "NUL:x"], ["..", "a:b"],
  ["", "", ""], ["a\\", "\\b\\"], ["/", "//x"],
];
for (const args of joins) record("join", args, () => win32.join(...args));
const formats = [
  { root: "C:\\", dir: "", base: "", name: "f", ext: "txt" },
  { root: "", dir: "a\\b", base: "", name: "f", ext: ".txt" },
  { root: "C:\\", dir: "C:\\", base: "x", name: "", ext: "" },
  { root: "", dir: "", base: "", name: "", ext: "" },
];
for (const f of formats) record("format", [f], () => win32.format(f));
for (const a of absolute) {
  record("resolve", [a], () => win32.resolve(a));
  record("toNamespacedPath", [a], () => win32.toNamespacedPath(a));
  // a root keeps its `\\`, which Node off Windows takes for no separator and adds a `/`
  if (!win32.resolve(a).endsWith("\\")) record("toFileUrl", [a], () => pathToFileURL(a, { windows: true }).href);
}
for (const a of some) {
  for (const b of ["..\\y", "D:z", "C:w", "\\\\h\\s\\t"]) {
    // another drive's relative path reads the working directory
    if (!win32.resolve(a, b).includes(cwd)) record("resolve", [a, b], () => win32.resolve(a, b));
  }
  for (const b of some) record("relative", [a, b], () => win32.relative(a, b));
}
for (const extra of [["C:\\Foo\\Bar", "c:\\foo\\baz"], ["C:\\İ\\a", "C:\\i̇\\b"], ["\\\\Srv\\S\\a", "\\\\srv\\s"]])
  record("relative", extra, () => win32.relative(...extra));
const urls = [
  "file:///C:/a/b", "file:///C:/a%20b/%C3%BC", "file:///c:/", "file://server/share/x", "file:///a/b",
  "file:///C:/a%2Fb", "file:///C:/a%5cb", "file:///C:/a%25b", "http://x/C:/a", "file:///C:/a?q#h",
];
for (const u of urls) record("fromFileUrl", [u], () => fileURLToPath(u, { windows: true }));
for (const p of ["C:\\a b\\#?%\\", "C:\\ü\\x", "C:\\a\\", "\\\\server\\share\\a b", "\\\\?\\UNC\\srv\\sh\\x", "\\\\server", "C:\\a\tb\nc"])
  record("toFileUrl", [p], () => pathToFileURL(p, { windows: true }).href);

writeFileSync(out, parts.join("\n") + "\n");
writeFileSync(more, lines.join("\n") + "\n");
console.log(`${parts.length + lines.length} lines in ${out} and ${more}`);
