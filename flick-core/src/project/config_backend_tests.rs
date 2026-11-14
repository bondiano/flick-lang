use super::{Backend, parse_text};
use crate::diagnostics::codes;

fn manifest(selection: &str) -> String {
    format!(
        "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n\
         [build]\nenvironment = \"js/node\"\n{selection}\n"
    )
}

#[test]
fn a_backend_package_pin_is_independent_of_the_js_runtime() {
    let text = manifest(&format!(
        "[build.backend]\nid = \"test/literal\"\nversion = \"1.0.0\"\n\
         sha256 = \"{}\"\ncomponent = \"backends/literal.wasm\"",
        "a".repeat(64)
    ));
    let config = parse_text(&text).expect("a pinned backend package");
    assert_eq!(config.build.effect, Backend::Rt);
    let backend = config.build.backend.expect("backend selection");
    assert_eq!(backend.id, "test/literal");
    assert_eq!(backend.version, "1.0.0");
    assert_eq!(backend.sha256, "a".repeat(64));
    assert_eq!(backend.component, "backends/literal.wasm");
    let environment = config.build.environment.expect("environment");
    assert_eq!(
        (environment.backend.as_str(), environment.platform.as_str()),
        ("js", "node")
    );
}

#[test]
fn backend_pins_refuse_ranges_bad_digests_empty_paths_and_unknown_fields() {
    let good = format!(
        "[build.backend]\nid = \"test/literal\"\nversion = \"1.0.0\"\n\
         sha256 = \"{}\"\ncomponent = \"backends/literal.wasm\"",
        "a".repeat(64)
    );
    for bad in [
        good.replace("test/literal", ""),
        good.replace("1.0.0", "^1.0"),
        good.replace(&"a".repeat(64), &"A".repeat(64)),
        good.replace(&"a".repeat(64), "1234"),
        good.replace("backends/literal.wasm", ""),
        format!("{good}\ncomponnet = \"typo.wasm\""),
    ] {
        let text = manifest(&bad);
        let diagnostic = parse_text(&text).expect_err("an invalid backend pin");
        assert_eq!(diagnostic.code, codes::P005, "{text}");
        assert!(diagnostic.span.is_some(), "{diagnostic:?}");
    }
}

#[test]
fn environments_distinguish_browser_and_node_without_a_closed_backend_enum() {
    for value in ["js/browser", "js/node", "go/native", "example/vm-2"] {
        let text = manifest("").replace("js/node", value);
        let config = parse_text(&text).expect("an explicit environment");
        let (backend, platform) = value.split_once('/').expect("pair");
        let env = config.build.environment.expect("environment");
        assert_eq!(
            (env.backend.as_str(), env.platform.as_str()),
            (backend, platform)
        );
    }
    for value in ["js", "js/", "/node", "js/node/other", "js/ node"] {
        let text = manifest("").replace("js/node", value);
        let diagnostic = parse_text(&text).expect_err("an incomplete environment");
        assert_eq!(diagnostic.code, codes::P005, "{text}");
    }
}
