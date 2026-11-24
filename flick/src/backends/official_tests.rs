//! Tests verifying official backend pins, descriptor parity with `flick-core`,
//! and rendered EN/RU value-ABI documentation tables.

use std::path::Path;
use std::{env, fs};

use flick_backend::Backend;
use flick_backend::host::{Budget, WasmBackend};
use flick_core::project::official_backends;
use flick_core::types::{DocLocale, render_value_abi_markdown, replace_value_abi_block};

use super::{GO, IDS, JS, checkout};
use crate::backends::options;
use crate::component;

/// The backends index lists what each pin says its component accepts,
/// so the pin must say what the component's descriptor does.
#[test]
fn each_official_pin_lists_what_its_component_accepts() {
    let core_official = official_backends();
    for id in IDS {
        let (pin, bytes) = checkout(id).expect("this checkout's component");
        let engine = component::engine().expect("an engine");
        let cancel = component::cancellation().expect("a cancellation").clone();
        let backend =
            WasmBackend::load_with_cancellation(engine, pin, &bytes, Budget::default(), cancel)
                .expect("the component loads");
        let descriptor = backend.describe();
        assert!(flick_backend::validate_tags(&descriptor).is_ok(), "{id}");
        assert!(
            flick_backend::validate_foreign_metadata(&descriptor).is_ok(),
            "{id}"
        );
        assert!(
            flick_backend::validate_value_abi(&descriptor).is_ok(),
            "{id}"
        );
        let from_core = core_official
            .iter()
            .find(|b| b.id == id)
            .unwrap_or_else(|| panic!("official_backends missing {id}"));
        assert_eq!(
            &options::descriptor_tags(&descriptor),
            from_core,
            "{id} component descriptor differs from core official_backends()"
        );
        let name = id.trim_start_matches("flick/");
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../backends/{name}/package/backend.json"));
        let listed: serde_json::Value =
            serde_json::from_slice(&fs::read(path).expect("the pin")).expect("JSON");
        for (key, range) in [
            ("backend_api", &descriptor.backend_api),
            ("ir", &descriptor.ir),
        ] {
            assert_eq!(listed[key]["minimum"], range.minimum.as_str(), "{id} {key}");
            assert_eq!(
                listed[key]["maximum_exclusive"],
                range.maximum_exclusive.as_str(),
                "{id} {key}"
            );
        }
    }
}

fn load_rendered_value_abi(id: &str) -> (String, String) {
    let (pin, bytes) = checkout(id).unwrap_or_else(|| panic!("this checkout's {id} component"));
    let engine = component::engine().expect("an engine");
    let cancel = component::cancellation().expect("a cancellation").clone();
    let backend =
        WasmBackend::load_with_cancellation(engine, pin, &bytes, Budget::default(), cancel)
            .unwrap_or_else(|e| panic!("{id} component loads: {e}"));
    let descriptor = backend.describe();
    let tags = options::descriptor_tags(&descriptor);
    let table = tags
        .value_abi
        .as_ref()
        .unwrap_or_else(|| panic!("{id} publishes a value ABI table"));
    (
        render_value_abi_markdown(table, DocLocale::En),
        render_value_abi_markdown(table, DocLocale::Ru),
    )
}

/// Official English and Russian FFI and backend documentation tables must
/// match what the shipped backend components publish in `descriptor.value-abi`.
/// Run with `FLICK_UPDATE_ABI_DOCS=1` to rewrite the rendered blocks in place.
#[test]
fn official_value_abi_tables_match_rendered_docs() {
    let (js_en, js_ru) = load_rendered_value_abi(JS);
    let (go_en, go_ru) = load_rendered_value_abi(GO);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root");
    let update = env::var_os("FLICK_UPDATE_ABI_DOCS").is_some();
    let targets = [
        (
            "docs/src/content/docs/spec/06-typescript-and-ffi.md",
            "flick/js:en",
            &js_en,
        ),
        (
            "docs/src/content/docs/ru/spec/06-typescript-and-ffi.md",
            "flick/js:ru",
            &js_ru,
        ),
        (
            "docs/src/content/docs/reference/ffi.md",
            "flick/js:en",
            &js_en,
        ),
        (
            "docs/src/content/docs/ru/reference/ffi.md",
            "flick/js:ru",
            &js_ru,
        ),
        (
            "docs/src/content/docs/reference/ffi.md",
            "flick/go:en",
            &go_en,
        ),
        (
            "docs/src/content/docs/ru/reference/ffi.md",
            "flick/go:ru",
            &go_ru,
        ),
        (
            "docs/src/content/docs/reference/codegen.md",
            "flick/js:en",
            &js_en,
        ),
        (
            "docs/src/content/docs/ru/reference/codegen.md",
            "flick/js:ru",
            &js_ru,
        ),
        (
            "docs/src/content/docs/reference/codegen.md",
            "flick/go:en",
            &go_en,
        ),
        (
            "docs/src/content/docs/ru/reference/codegen.md",
            "flick/go:ru",
            &go_ru,
        ),
    ];
    for (rel, marker, rendered) in targets {
        let path = root.join(rel);
        let actual =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {rel}: {e}"));
        let expected = replace_value_abi_block(&actual, marker, rendered)
            .unwrap_or_else(|| panic!("{rel} is missing value-abi markers for `{marker}`"));
        if update {
            if actual != expected {
                fs::write(&path, &expected)
                    .unwrap_or_else(|e| panic!("failed to write {rel}: {e}"));
            }
        } else {
            assert_eq!(
                actual, expected,
                "{rel} value-abi block `{marker}` drifted from official descriptor; \
                 run `FLICK_UPDATE_ABI_DOCS=1 cargo test --profile e2e -p flick --bin flick official_value_abi_tables_match_rendered_docs` \
                 and restamp translations with `node docs/scripts/parity.mjs --stamp ...`"
            );
        }
    }
}
