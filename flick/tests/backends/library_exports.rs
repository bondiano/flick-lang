//! End-to-end tests for `@export "name"`, `[target.<name>] kind = "library"`,
//! JS library artifact formats (`esm`, `cjs`, `iife`), `index.d.ts` header
//! projection, and WIT export/header passing to the `test/literal` component (#670).

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use super::common::{flick_in, project};
use super::pinned;

const LIB_HELPER: &str = "\
module App.Helper exposing (bump)

bump : Int -> Int
let bump n = n + 1
";

const LIB_MATH: &str = "\
module App.Math exposing (add)

import App.Helper as Helper

/// Adds two integers plus one via helper.
@export \"addNumbers\"
add : Int -> Int -> Int
let add a b = Helper.bump (a + b)

/// Private function still exported under its host name.
@export \"doubleValue\"
double_val : Int -> Int
let double_val x = x * 2

internal_only : Int -> Int
let internal_only x = x + 100
";

fn run_node(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("node")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("node executes");
    assert!(
        out.status.success(),
        "node {args:?} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

#[test]
#[ignore = "requires just wasm-backends"]
#[allow(clippy::too_many_lines)]
fn js_library_builds_and_runs_esm_cjs_and_iife_in_node_with_headers() {
    for profile in ["debug", "release"] {
        // 1. ESM library (`format = "esm"`, default)
        let esm_toml = "\
schema-version = 1
[project]
kind = \"lib\"
[target.js]
kind = \"library\"
format = \"esm\"
";
        let esm_root = project(
            &format!("lib_js_esm_{profile}"),
            &[
                ("flick.toml", esm_toml),
                ("src/App/Helper.fl", LIB_HELPER),
                ("src/App/Math.fl", LIB_MATH),
            ],
        );
        let built = flick_in(&esm_root, &["build", "--profile", profile, "--offline"]);
        assert!(built.status.success(), "esm {profile}: {built:?}");
        let dist = esm_root.join("dist");
        assert!(dist.join("index.js").is_file());
        assert!(!dist.join("flick-main.js").exists());
        let dts = fs::read_to_string(dist.join("index.d.ts")).expect("index.d.ts");
        assert!(
            dts.contains(
                "export declare const addNumbers: (arg: number) => (arg: number) => number;"
            ),
            "{dts}"
        );
        assert!(
            dts.contains("export declare const doubleValue: (arg: number) => number;"),
            "{dts}"
        );
        assert!(dts.contains("Adds two integers plus one"), "{dts}");
        assert!(!dts.contains("internal_only"), "{dts}");
        let pkg: Value =
            serde_json::from_str(&fs::read_to_string(dist.join("package.json")).expect("pkg"))
                .expect("json");
        assert_eq!(pkg["type"], "module");
        assert_eq!(pkg["main"], "./index.js");
        assert_eq!(pkg["types"], "./index.d.ts");
        let esm_out = run_node(
            &dist,
            &[
                "--input-type=module",
                "-e",
                "import * as lib from './index.js'; console.log(JSON.stringify({ keys: Object.keys(lib).sort(), sum: lib.addNumbers(20)(22), dbl: lib.doubleValue(21) }));",
            ],
        );
        let esm_val: Value = serde_json::from_str(esm_out.trim()).expect("esm json");
        assert_eq!(
            esm_val["keys"],
            serde_json::json!(["addNumbers", "doubleValue"])
        );
        assert_eq!(esm_val["sum"], 43);
        assert_eq!(esm_val["dbl"], 42);

        // 2. CommonJS library (`format = "cjs"`)
        let cjs_toml = "\
schema-version = 1
[project]
kind = \"lib\"
[target.js]
kind = \"library\"
format = \"cjs\"
";
        let cjs_root = project(
            &format!("lib_js_cjs_{profile}"),
            &[
                ("flick.toml", cjs_toml),
                ("src/App/Helper.fl", LIB_HELPER),
                ("src/App/Math.fl", LIB_MATH),
            ],
        );
        let built = flick_in(&cjs_root, &["build", "--profile", profile, "--offline"]);
        assert!(built.status.success(), "cjs {profile}: {built:?}");
        let dist = cjs_root.join("dist");
        assert!(dist.join("index.cjs").is_file());
        assert!(dist.join("index.d.ts").is_file());
        let pkg: Value =
            serde_json::from_str(&fs::read_to_string(dist.join("package.json")).expect("pkg"))
                .expect("json");
        assert_eq!(pkg["type"], "commonjs");
        assert_eq!(pkg["main"], "./index.cjs");
        assert_eq!(pkg["types"], "./index.d.ts");
        let cjs_out = run_node(
            &dist,
            &[
                "-e",
                "const lib = require('./index.cjs'); console.log(JSON.stringify({ keys: Object.keys(lib).sort(), sum: lib.addNumbers(10)(5), dbl: lib.doubleValue(9) }));",
            ],
        );
        let cjs_val: Value = serde_json::from_str(cjs_out.trim()).expect("cjs json");
        assert_eq!(
            cjs_val["keys"],
            serde_json::json!(["addNumbers", "doubleValue"])
        );
        assert_eq!(cjs_val["sum"], 16);
        assert_eq!(cjs_val["dbl"], 18);

        // 3. IIFE library (`format = "iife"`, custom `global-name`)
        let iife_toml = "\
schema-version = 1
[project]
kind = \"lib\"
[target.js]
kind = \"library\"
format = \"iife\"
global-name = \"MyMath\"
";
        let iife_root = project(
            &format!("lib_js_iife_{profile}"),
            &[
                ("flick.toml", iife_toml),
                ("src/App/Helper.fl", LIB_HELPER),
                ("src/App/Math.fl", LIB_MATH),
            ],
        );
        let built = flick_in(&iife_root, &["build", "--profile", profile, "--offline"]);
        assert!(built.status.success(), "iife {profile}: {built:?}");
        let dist = iife_root.join("dist");
        assert!(dist.join("index.iife.js").is_file());
        let dts = fs::read_to_string(dist.join("index.d.ts")).expect("iife dts");
        assert!(dts.contains("declare global {"), "{dts}");
        assert!(dts.contains("var MyMath: {"), "{dts}");
        let iife_out = run_node(
            &dist,
            &[
                "-e",
                "const fs = require('node:fs'); const vm = require('node:vm'); const box = {}; vm.runInNewContext(fs.readFileSync('./index.iife.js', 'utf8'), box); const lib = box.MyMath; console.log(JSON.stringify({ keys: Object.keys(lib).sort(), sum: lib.addNumbers(30)(11), dbl: lib.doubleValue(7) }));",
            ],
        );
        let iife_val: Value = serde_json::from_str(iife_out.trim()).expect("iife json");
        assert_eq!(
            iife_val["keys"],
            serde_json::json!(["addNumbers", "doubleValue"])
        );
        assert_eq!(iife_val["sum"], 42);
        assert_eq!(iife_val["dbl"], 14);
    }
}

#[test]
#[ignore = "requires just wasm-backends"]
fn fixture_backend_receives_host_exports_and_header_request_and_emits_c_header() {
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("just wasm-backends builds flick_test_backend.wasm");
    let digest = flick_backend::sha256(&bytes);
    let source = "\
module App.Calc exposing (add, is_pos)
// π preserves UTF-8 byte offsets

@export \"add_nums\"
add : Int -> Int -> Int
let add a b = a + b

@export \"is_positive\"
is_pos : Int -> Bool
let is_pos n = n > 0
";
    let config = format!(
        "schema-version = 1\n[project]\nkind = \"lib\"\n\n\
         [build.backend]\nid = \"test/literal\"\nversion = \"1.0.0\"\n\
         sha256 = \"{digest}\"\ncomponent = \"backend.wasm\"\n\n\
         [target.literal]\nkind = \"library\"\nheaders = \"c\"\n"
    );
    let _ = pinned(); // keep helper referenced consistently
    let root = project(
        "fixture_backend_library_exports",
        &[("flick.toml", &config), ("src/App/Calc.fl", source)],
    );
    fs::write(root.join("backend.wasm"), &bytes).expect("write component");

    let checked = flick_in(&root, &["check", "--offline"]);
    assert!(checked.status.success(), "check failed: {checked:?}");
    let built = flick_in(&root, &["build", "--offline"]);
    assert!(built.status.success(), "build failed: {built:?}");

    let report: Value = serde_json::from_str(
        &fs::read_to_string(root.join("dist/exports-report.json")).expect("exports-report.json"),
    )
    .expect("valid json");
    assert_eq!(report["kind"], "library");
    assert_eq!(
        report["headers"],
        serde_json::json!({ "enabled": true, "format": "c" })
    );
    let exports = report["exports"].as_array().expect("exports array");
    assert_eq!(exports.len(), 2, "{report}");
    assert_eq!(exports[0]["name"], "add_nums");
    assert_eq!(exports[0]["symbol"]["module"], "App.Calc");
    assert_eq!(exports[0]["symbol"]["name"], "add");
    let s0 = usize::try_from(exports[0]["span"]["start"].as_u64().expect("s0")).expect("usize");
    let e0 = usize::try_from(exports[0]["span"]["end"].as_u64().expect("e0")).expect("usize");
    assert_eq!(&source[s0..e0], "@export \"add_nums\"");

    assert_eq!(exports[1]["name"], "is_positive");
    assert_eq!(exports[1]["symbol"]["name"], "is_pos");
    let s1 = usize::try_from(exports[1]["span"]["start"].as_u64().expect("s1")).expect("usize");
    let e1 = usize::try_from(exports[1]["span"]["end"].as_u64().expect("e1")).expect("usize");
    assert_eq!(&source[s1..e1], "@export \"is_positive\"");

    let header = fs::read_to_string(root.join("dist/library.h")).expect("library.h");
    assert!(
        header.contains("int32_t add_nums(int32_t, int32_t);"),
        "{header}"
    );
    assert!(header.contains("bool is_positive(int32_t);"), "{header}");
}
