//! `flick ffi import` gives an opaque type the form TypeScript names it
//! by, so the declarations of the generated bindings take the values a
//! TypeScript caller has: an instance of the package's class, a `Date`, an
//! `Intl.DateTimeFormatOptions`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const DECLARATIONS: &str = "export declare class Clock {
  private constructor();
  readonly zone: string;
}
export interface Options {
  readonly locale: string;
}
declare class Inner {
  private constructor();
}
export declare function configured(options: Options): Clock;
export declare function shown(clock: Clock, options: Intl.DateTimeFormatOptions): string;
export declare function started(clock: Clock): Date;
export declare function hidden(): Inner;
// with this, only what says `export` is exported
export {};
";

const PACKAGE: &str = r#"{ "name": "typed-handles", "version": "1.0.0", "type": "module",
  "exports": { ".": { "types": "./index.d.ts", "default": "./index.js" } } }
"#;

const CONSUMER: &str = r#"import type { Effect } from "@flick-lang/effect-ts";
import type { Clock } from "typed-handles";
import { configured, shown, started } from "./dist/Ffi/TypedHandles/Generated.js";

declare const clock: Clock;
const made: Effect.Effect<Clock, never, never> = configured({ locale: "en" });
const text: Effect.Effect<string, never, never> = shown(clock)({ dateStyle: "short" });
const at: Effect.Effect<Date, never, never> = started(clock);
// @ts-expect-error a number is no clock
shown(1);
export { at, made, text };
"#;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn flick(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env(
            "FLICK_DTS_IMPORTER",
            root().join("tools/dts-importer/index.mjs"),
        )
        .env("NO_COLOR", "1")
        .output()
        .expect("flick runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
#[ignore = "needs Node and `pnpm install`"]
fn an_imported_opaque_type_is_the_type_typescript_has() {
    let dir = root().join("target/tmp/ffi_forms/app");
    let _ = fs::remove_dir_all(&dir);
    let installed = dir.join("node_modules/typed-handles");
    fs::create_dir_all(&installed).expect("package dir");
    fs::create_dir_all(dir.join("src/Ffi")).expect("ffi dir");
    for (file, contents) in [
        ("package.json", PACKAGE),
        ("index.d.ts", DECLARATIONS),
        ("index.js", "export {};\n"),
    ] {
        fs::write(installed.join(file), contents).expect("write");
    }
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[build]\neffect = \"effect\"\n",
    )
    .expect("write");
    fs::write(
        dir.join("package.json"),
        "{ \"name\": \"forms-consumer\", \"type\": \"module\" }\n",
    )
    .expect("write");

    let import = flick(
        &dir,
        &["ffi", "import", "typed-handles", "--out", "src/Ffi"],
    );
    assert!(import.status.success(), "{}", text(&import.stderr));
    let generated = fs::read_to_string(dir.join("src/Ffi/TypedHandles/Generated.fl"))
        .expect("the generated module");
    for form in [
        // an export of the package, a global, and a member of a namespace
        "@ts \"$Clock\"\n@ts import \"typed-handles\" \"Clock\"\ntype Clock\n",
        "@ts \"$Options\"\n@ts import \"typed-handles\" \"Options\"\ntype Options\n",
        "@ts \"Date\"\ntype Date\n",
        "@ts \"Intl.DateTimeFormatOptions\"\ntype DateTimeFormatOptions\n",
        // nothing outside the declarations names it: it stays a brand
        "opaque to Flick.\ntype Inner\n",
    ] {
        assert!(generated.contains(form), "{form}\n{generated}");
    }

    let build = flick(&dir, &["build", "--emit-dts"]);
    assert!(build.status.success(), "{}", text(&build.stderr));
    fs::write(dir.join("consumer.ts"), CONSUMER).expect("write");
    let repo = root();
    fs::write(
        dir.join("tsconfig.json"),
        serde_json::json!({
            "compilerOptions": {
                "strict": true, "noEmit": true, "module": "nodenext",
                "moduleResolution": "nodenext", "target": "es2022",
                "lib": ["es2022", "dom", "esnext.disposable"], "types": ["node"],
                "typeRoots": [repo.join("runtime/node_modules/@types")],
            },
            "files": ["consumer.ts"],
        })
        .to_string(),
    )
    .expect("write");
    let tsc = Command::new(repo.join("node_modules/.bin/tsc"))
        .arg("-p")
        .arg(&dir)
        .output()
        .expect("tsc");
    assert!(tsc.status.success(), "{}", text(&tsc.stdout));
}
