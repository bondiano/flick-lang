//! What TypeScript's language service shows of the declarations: the
//! alias name and the `///` text on hover.

use std::fs;
use std::path::Path;
use std::process::Command;

use super::flick_in;

/// What TypeScript's language service shows on hover over the last
/// occurrence of each word: `node hover.mjs <typescript package.json> <file> <word>…`.
const HOVER: &str = r#"import { createRequire } from "node:module";
import path from "node:path";

const [typescript, file, ...words] = process.argv.slice(2);
const ts = createRequire(typescript)("typescript");
const text = ts.sys.readFile(file);
const options = {
  strict: true, noEmit: true, target: ts.ScriptTarget.ES2022,
  module: ts.ModuleKind.NodeNext, moduleResolution: ts.ModuleResolutionKind.NodeNext,
};
const service = ts.createLanguageService({
  getScriptFileNames: () => [file],
  getScriptVersion: () => "0",
  getScriptSnapshot: (f) => {
    const t = ts.sys.readFile(f);
    return t === undefined ? undefined : ts.ScriptSnapshot.fromString(t);
  },
  getCurrentDirectory: () => path.dirname(file),
  getCompilationSettings: () => options,
  getDefaultLibFileName: ts.getDefaultLibFilePath,
  fileExists: ts.sys.fileExists,
  readFile: ts.sys.readFile,
  readDirectory: ts.sys.readDirectory,
  directoryExists: ts.sys.directoryExists,
  getDirectories: ts.sys.getDirectories,
});
const hover = (word) => {
  const info = service.getQuickInfoAtPosition(file, text.lastIndexOf(word));
  return [ts.displayPartsToString(info?.displayParts), ts.displayPartsToString(info?.documentation)];
};
console.log(JSON.stringify(Object.fromEntries(words.map((w) => [w, hover(w)]))));
"#;

#[test]
#[ignore = "needs Node and pnpm install"]
fn typescript_shows_the_alias_and_its_docs_on_hover() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("dts_hover");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/Lib")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    fs::write(dir.join("package.json"), "{\"type\":\"module\"}\n").expect("write");
    fs::write(
        dir.join("src/Lib/Docs.fl"),
        "module Lib.Docs exposing (Point, Shape(..), area)\n\n\
         /// A point on the plane.\ntype alias Point = { x : Float; y : Float }\n\n\
         type Shape =\n  /// A circle by its radius.\n  | Circle Float\n  | Square Float\n\n\
         /// The area of a shape, in **square** units.\n\
         area : Shape -> Float\n\
         let area shape =\n  match shape with\n  | Circle r -> r * r\n  | Square side -> side * side\n",
    )
    .expect("write");
    let build = flick_in(&dir, &["build", "--emit-dts"]);
    assert!(build.status.success(), "{build:?}");
    fs::write(
        dir.join("consumer.ts"),
        "import { area, Circle } from './dist/Lib/Docs.js';\n\
         import type { Point } from './dist/Lib/Docs.js';\n\
         export const corner: Point = { x: 1, y: 2 };\n\
         export const measured = area(Circle(1));\n",
    )
    .expect("write");
    fs::write(dir.join("hover.mjs"), HOVER).expect("write");
    // the classic language service API, which the `.d.ts` importer depends on
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let node = Command::new("node")
        .arg(dir.join("hover.mjs"))
        .arg(repo.join("tools/dts-importer/package.json"))
        .arg(dir.join("consumer.ts"))
        .args(["corner", "Point", "area", "Circle"])
        .output()
        .expect("node");
    assert!(node.status.success(), "{node:?}");
    let shown: serde_json::Value = serde_json::from_slice(&node.stdout).expect("JSON");
    let expected = serde_json::json!({
        "corner": ["const corner: Point", ""],
        "Point": ["(alias) interface Point\nimport Point", "A point on the plane."],
        "area": [
            "(alias) area(arg: Shape): number\nimport area",
            "The area of a shape, in **square** units.",
        ],
        "Circle": [
            "(alias) Circle(arg: number): Shape\nimport Circle",
            "A circle by its radius.",
        ],
    });
    assert_eq!(shown, expected);
}
