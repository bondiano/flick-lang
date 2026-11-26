//! End-to-end tests for layout-aware whole-program specialization and fallback
//! boxing (`#739` AC #2, `#743` AC #1–5) through the real `flick-test-backend`
//! WASM component (`case = "layout-specialize"`), plus cross-target parity
//! against the official JS and Go backends.

use std::fs;
use std::path::Path;

use serde_json::Value;

#[path = "layout_specialize_fixtures.rs"]
mod fixtures;

use fixtures::{BUDGET_EXCEEDED_SOURCE, FALLBACK_FORMS_SOURCE, SPECIALIZED_AND_LAYOUTS_SOURCE};

use super::CONFIG;
use super::common::{flick_in, project};
use super::pinned;

fn component_bytes() -> Vec<u8> {
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    fs::read(&component).unwrap_or_else(|e| {
        panic!(
            "just backend-fixtures / just wasm-backends must build {}: {e}",
            component.display()
        )
    })
}

fn literal_config(entry: &str, digest: &str, extra_literal_toml: &str) -> String {
    format!(
        "{}\n[target.literal]\ncase = \"layout-specialize\"\n{extra_literal_toml}",
        pinned()
            .replace("entry = \"App\"", &format!("entry = \"{entry}\""))
            .replace(&"a".repeat(64), digest)
    )
}

fn js_config(entry: &str) -> String {
    CONFIG.replace("entry = \"App\"", &format!("entry = \"{entry}\""))
}

fn go_config(entry: &str) -> String {
    format!(
        "{}[build]\ntarget = \"go\"\n[tools]\n\"flick/go\" = [\"go\"]\n",
        CONFIG.replace("entry = \"App\"", &format!("entry = \"{entry}\""))
    )
}

#[test]
#[ignore = "requires just backend-fixtures (and Go for cross-target parity)"]
#[allow(clippy::too_many_lines)]
fn layout_specialize_reports_exact_layouts_zero_boxed_cells_and_is_deterministic() {
    let bytes = component_bytes();
    let digest = flick_backend::sha256(&bytes);
    let config = literal_config("Fixture.LayoutSpecialize", &digest, "");
    let root = project(
        "backend_layout_specialize_layouts",
        &[
            ("flick.toml", &config),
            (
                "src/Fixture/LayoutSpecialize.fl",
                SPECIALIZED_AND_LAYOUTS_SOURCE,
            ),
        ],
    );
    fs::write(root.join("backend.wasm"), &bytes).expect("component");

    // Build twice from clean dist/ and verify byte-for-byte determinism.
    let build1 = flick_in(&root, &["build", "--profile", "debug", "--offline"]);
    assert!(build1.status.success(), "{build1:?}");
    let report1_bytes =
        fs::read(root.join("dist/layout-specialize-report.json")).expect("report 1");
    let js1_bytes = fs::read(root.join("dist/_flick/Fixture/LayoutSpecialize.js"))
        .or_else(|_| fs::read(root.join("dist/Fixture/LayoutSpecialize.js")))
        .expect("emitted js 1");

    fs::remove_dir_all(root.join("dist")).expect("clean dist");
    let build2 = flick_in(&root, &["build", "--profile", "debug", "--offline"]);
    assert!(build2.status.success(), "{build2:?}");
    let report2_bytes =
        fs::read(root.join("dist/layout-specialize-report.json")).expect("report 2");
    let js2_bytes = fs::read(root.join("dist/_flick/Fixture/LayoutSpecialize.js"))
        .or_else(|_| fs::read(root.join("dist/Fixture/LayoutSpecialize.js")))
        .expect("emitted js 2");

    assert_eq!(
        report1_bytes, report2_bytes,
        "layout-specialize report must be byte-for-byte identical across runs"
    );
    assert_eq!(
        js1_bytes, js2_bytes,
        "emitted specialized artifact must be byte-for-byte identical across runs"
    );

    let report: Value = serde_json::from_slice(&report1_bytes).expect("valid report json");
    assert_eq!(report["ir_version"], "0.19.0");
    assert_eq!(report["mode"], "whole-program");

    // 1. Independently expected 64-bit layouts (`#739` AC #2):
    let layouts = &report["layouts"];

    // Shape = Circle Float | Rect Float Float
    // Tag(1B @ 0) + 7B pad + Float(8B @ 8) + Float(8B @ 16) = 24B, align 8.
    let shape = &layouts["Fixture.LayoutSpecialize.Shape"];
    assert_eq!(shape["size"], 24, "Shape layout: {shape}");
    assert_eq!(shape["align"], 8);
    assert_eq!(shape["discriminant"]["kind"], "tag");
    assert_eq!(shape["discriminant"]["offset"], 0);
    assert_eq!(shape["discriminant"]["width"], 1);
    assert_eq!(shape["variants"][0]["name"], "Circle");
    assert_eq!(shape["variants"][0]["tag"], 0);
    assert_eq!(shape["variants"][0]["fields"][0]["offset"], 8);
    assert_eq!(shape["variants"][0]["fields"][0]["size"], 8);
    assert_eq!(shape["variants"][0]["fields"][0]["storage"], "by_value");
    assert_eq!(shape["variants"][1]["name"], "Rect");
    assert_eq!(shape["variants"][1]["tag"], 1);
    assert_eq!(shape["variants"][1]["fields"][0]["offset"], 8);
    assert_eq!(shape["variants"][1]["fields"][1]["offset"], 16);

    // List Int: non-null pointer handle on 64-bit (size 8, align 8, null niche 0..=0).
    let list_int = &layouts["Prelude.List Prelude.Int"];
    assert_eq!(list_int["size"], 8, "List Int layout: {list_int}");
    assert_eq!(list_int["align"], 8);
    assert_eq!(list_int["niche"]["offset"], 0);
    assert_eq!(list_int["niche"]["width"], 8);
    assert_eq!(list_int["niche"]["start"], 0);
    assert_eq!(list_int["niche"]["end"], 0);

    // Option Float: Tag(1B @ 0) + 7B pad + Float(8B @ 8) = 16B, align 8.
    let opt_float = &layouts["Prelude.Option Prelude.Float"];
    assert_eq!(opt_float["size"], 16, "Option Float layout: {opt_float}");
    assert_eq!(opt_float["align"], 8);
    assert_eq!(opt_float["discriminant"]["kind"], "tag");
    assert_eq!(opt_float["discriminant"]["offset"], 0);
    assert_eq!(opt_float["discriminant"]["width"], 1);
    assert_eq!(opt_float["variants"][0]["name"], "None");
    assert_eq!(opt_float["variants"][1]["name"], "Some");
    assert_eq!(opt_float["variants"][1]["fields"][0]["offset"], 8);
    assert_eq!(opt_float["variants"][1]["fields"][0]["size"], 8);
    assert_eq!(opt_float["variants"][1]["fields"][0]["storage"], "by_value");

    // Closed record `{ flag : Bool; tag : Int; value : Float }`:
    // Packed descending align/size: value (Float, 8B @ 0), tag (Int, 4B @ 8), flag (Bool, 1B @ 12)
    // -> total size 16B, align 8, canonical UTF-16 field order ["flag", "tag", "value"].
    let record_entry = layouts
        .as_object()
        .expect("layouts map")
        .iter()
        .find(|(_, v)| {
            v["kind"] == "record"
                && v["fields"].as_array().is_some_and(|fs| {
                    fs.len() == 3
                        && fs[0]["label"] == "flag"
                        && fs[1]["label"] == "tag"
                        && fs[2]["label"] == "value"
                })
        })
        .map(|(_, v)| v)
        .expect("closed Metrics record layout present");
    assert_eq!(record_entry["size"], 16, "record layout: {record_entry}");
    assert_eq!(record_entry["align"], 8);
    assert_eq!(record_entry["fields"][0]["label"], "flag");
    assert_eq!(record_entry["fields"][0]["offset"], 12);
    assert_eq!(record_entry["fields"][1]["label"], "tag");
    assert_eq!(record_entry["fields"][1]["offset"], 8);
    assert_eq!(record_entry["fields"][2]["label"], "value");
    assert_eq!(record_entry["fields"][2]["offset"], 0);

    // 2. Zero boxed cells in specialized `fold_sum` (`List Int`) and `opt_map` (`Option Float`) (`#743` AC #1):
    let specs = report["specializations"]
        .as_array()
        .expect("specializations");
    let opt_map_spec = specs
        .iter()
        .find(|s| s["name"] == "opt_map")
        .expect("specialized opt_map instance");
    let fold_sum_spec = specs
        .iter()
        .find(|s| s["name"] == "fold_sum")
        .expect("specialized fold_sum instance");
    let per_fn_boxed = &report["stats"]["per_function_boxed_cells"];
    let opt_sym = opt_map_spec["symbol"].as_str().expect("symbol");
    let sum_sym = fold_sum_spec["symbol"].as_str().expect("symbol");
    assert_eq!(
        per_fn_boxed
            .get(opt_sym)
            .and_then(Value::as_u64)
            .unwrap_or(0),
        0,
        "specialized opt_map (Option Float) must have 0 boxed cells"
    );
    assert_eq!(
        per_fn_boxed
            .get(sum_sym)
            .and_then(Value::as_u64)
            .unwrap_or(0),
        0,
        "specialized fold_sum (List Int) must have 0 boxed cells"
    );
    assert_eq!(
        report["stats"]["fallback_definitions"]
            .as_u64()
            .unwrap_or(1),
        0,
        "specialized fixture must have 0 fallback definitions: {}",
        report["stats"]
    );

    // 3. Execute on fixture backend, JS, and Go and verify exact independently expected stdout:
    let expected = "shape=19|opt=20.5|sum=42|rec=T:20.5:42\n";
    let run_lit = flick_in(&root, &["run", "--profile", "debug", "--offline"]);
    assert!(run_lit.status.success(), "{run_lit:?}");
    assert_eq!(
        String::from_utf8(run_lit.stdout).expect("utf8"),
        expected,
        "fixture backend output"
    );

    let js_root = project(
        "backend_layout_specialize_js",
        &[
            ("flick.toml", &js_config("Fixture.LayoutSpecialize")),
            (
                "src/Fixture/LayoutSpecialize.fl",
                SPECIALIZED_AND_LAYOUTS_SOURCE,
            ),
        ],
    );
    let run_js = flick_in(&js_root, &["run", "--profile", "debug", "--offline"]);
    assert!(run_js.status.success(), "{run_js:?}");
    assert_eq!(String::from_utf8(run_js.stdout).expect("utf8"), expected);

    let go_root = project(
        "backend_layout_specialize_go",
        &[
            ("flick.toml", &go_config("Fixture.LayoutSpecialize")),
            (
                "src/Fixture/LayoutSpecialize.fl",
                SPECIALIZED_AND_LAYOUTS_SOURCE,
            ),
        ],
    );
    let run_go = flick_in(&go_root, &["run", "--profile", "debug", "--offline"]);
    assert!(run_go.status.success(), "{run_go:?}");
    assert_eq!(String::from_utf8(run_go.stdout).expect("utf8"), expected);

    fs::remove_dir_all(root).expect("cleanup");
    fs::remove_dir_all(js_root).expect("cleanup");
    fs::remove_dir_all(go_root).expect("cleanup");
}

#[test]
#[ignore = "requires just backend-fixtures (and Go for cross-target parity)"]
#[allow(clippy::too_many_lines)]
fn layout_specialize_handles_all_fallback_forms_and_matches_js_and_go() {
    let bytes = component_bytes();
    let digest = flick_backend::sha256(&bytes);
    let config = literal_config("Fixture.LayoutSpecialize", &digest, "");
    let root = project(
        "backend_layout_specialize_fallbacks",
        &[
            ("flick.toml", &config),
            ("src/Fixture/LayoutSpecialize.fl", FALLBACK_FORMS_SOURCE),
        ],
    );
    fs::write(root.join("backend.wasm"), &bytes).expect("component");

    let build = flick_in(&root, &["build", "--profile", "debug", "--offline"]);
    assert!(build.status.success(), "{build:?}");
    let report: Value = serde_json::from_slice(
        &fs::read(root.join("dist/layout-specialize-report.json")).expect("report"),
    )
    .expect("valid json");

    let cls = &report["classifications"];
    let has_reason = |key: &str, reason: &str| {
        cls.get(key)
            .and_then(|v| v.get("fallback"))
            .and_then(Value::as_array)
            .is_some_and(|arr| arr.iter().any(|r| r == reason))
    };
    assert!(
        has_reason(
            "Fixture.LayoutSpecialize.poly_bounce",
            "polymorphic_recursion"
        ),
        "expected polymorphic_recursion on poly_bounce: {cls}"
    );
    assert!(
        has_reason("Fixture.LayoutSpecialize.apply_hr", "higher_rank_callback"),
        "expected higher_rank_callback on apply_hr: {cls}"
    );
    assert!(
        has_reason(
            "Fixture.LayoutSpecialize.packed_hashed",
            "packed_or_existential"
        ) || has_reason(
            "Fixture.LayoutSpecialize.packed_bracketed",
            "packed_or_existential"
        ),
        "expected packed_or_existential on packed modules: {cls}"
    );
    assert!(
        has_reason(
            "Fixture.LayoutSpecialize.use_open_row",
            "higher_rank_callback"
        ) || has_reason(
            "Fixture.LayoutSpecialize.use_open_row",
            "open_row_polymorphic"
        ),
        "expected fallback on use_open_row: {cls}"
    );
    assert!(
        has_reason("List.Core.length", "polymorphic_foreign"),
        "expected polymorphic_foreign on List.Core.length: {cls}"
    );

    // Polymorphic recursion compiles ONCE over boxed cells (0 specialized copies).
    assert!(
        report["stats"]["instances_per_def"]
            .get("Fixture.LayoutSpecialize.poly_bounce")
            .is_none(),
        "poly_bounce must compile once without specialized clones: {}",
        report["stats"]
    );

    // `shown` must preserve runtime dynamic dictionaries for the two `unpack` sites
    // while allowing static specialization for the global `Show Int` call!
    let shown_specs: Vec<&Value> = report["specializations"]
        .as_array()
        .expect("specializations")
        .iter()
        .filter(|s| s["name"] == "shown")
        .collect();
    assert!(
        shown_specs
            .iter()
            .any(|s| s["runtime_dicts"].as_array().is_some_and(|d| !d.is_empty())),
        "dynamic unpack dictionaries must remain runtime params: {shown_specs:?}"
    );

    // Independently expected result:
    // - poly_bounce + apply_hr -> 7:ok
    // - Hashed unpack -> "#5 <#6> Some #5"
    // - Bracketed unpack -> "[5] <[6]> Some [5]" (distinct from Hashed at same concrete type Int!)
    // - Global outside unpack -> "5 <6> Some 5"
    // - GADT ShowBox -> "box:99,box:\"hi\""
    // - Open-row record -> 11 + 31 = 42
    // - Polymorphic foreign List.length -> 3 + 2 = 5
    let expected = "pr=7:ok|hash=#5 <#6> Some #5|brack=[5] <[6]> Some [5]|glob=5 <6> Some 5|gadt=box:99,box:\"hi\"|row=42|flen=5\n";

    let run_lit = flick_in(&root, &["run", "--profile", "debug", "--offline"]);
    assert!(run_lit.status.success(), "{run_lit:?}");
    assert_eq!(
        String::from_utf8(run_lit.stdout).expect("utf8"),
        expected,
        "fixture backend fallback execution"
    );

    let js_root = project(
        "backend_layout_specialize_fallbacks_js",
        &[
            ("flick.toml", &js_config("Fixture.LayoutSpecialize")),
            ("src/Fixture/LayoutSpecialize.fl", FALLBACK_FORMS_SOURCE),
        ],
    );
    let run_js = flick_in(&js_root, &["run", "--profile", "debug", "--offline"]);
    assert!(run_js.status.success(), "{run_js:?}");
    assert_eq!(String::from_utf8(run_js.stdout).expect("utf8"), expected);

    let go_root = project(
        "backend_layout_specialize_fallbacks_go",
        &[
            ("flick.toml", &go_config("Fixture.LayoutSpecialize")),
            ("src/Fixture/LayoutSpecialize.fl", FALLBACK_FORMS_SOURCE),
        ],
    );
    let run_go = flick_in(&go_root, &["run", "--profile", "debug", "--offline"]);
    assert!(run_go.status.success(), "{run_go:?}");
    assert_eq!(String::from_utf8(run_go.stdout).expect("utf8"), expected);

    fs::remove_dir_all(root).expect("cleanup");
    fs::remove_dir_all(js_root).expect("cleanup");
    fs::remove_dir_all(go_root).expect("cleanup");
}

#[test]
#[ignore = "requires just backend-fixtures"]
fn layout_specialize_pathological_growth_triggers_budget_diagnostic_naming_definition() {
    let bytes = component_bytes();
    let digest = flick_backend::sha256(&bytes);
    let config = literal_config(
        "Fixture.LayoutSpecialize",
        &digest,
        "max-instances = \"4\"\n",
    );
    let root = project(
        "backend_layout_specialize_budget",
        &[
            ("flick.toml", &config),
            ("src/Fixture/LayoutSpecialize.fl", BUDGET_EXCEEDED_SOURCE),
        ],
    );
    fs::write(root.join("backend.wasm"), &bytes).expect("component");

    let built = flick_in(
        &root,
        &[
            "build",
            "--profile",
            "debug",
            "--offline",
            "--diagnostics",
            "json",
        ],
    );
    assert_eq!(built.status.code(), Some(1), "{built:?}");
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(
        stderr.contains("FLICK-P012"),
        "expected FLICK-P012 diagnostic: {stderr}"
    );
    assert!(
        stderr.contains("budget exceeded") && stderr.contains("Fixture.LayoutSpecialize."),
        "diagnostic must report budget exceeded and name offending definition: {stderr}"
    );
    assert!(
        !root.join("dist").exists(),
        "failed build publishes nothing"
    );

    fs::remove_dir_all(root).expect("cleanup");
}
