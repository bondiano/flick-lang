//! Tests for `[tools]`, `[package] when`, and `[host]` manifest sections.

use super::{manifest, parse_text};
use crate::diagnostics::codes;

#[test]
fn a_project_names_the_host_tools_each_backend_may_run() {
    let config = parse_text(
        "schema-version = 1\n[project]\nkind = \"lib\"\n[tools]\n\"flick/go\" = [\"go\"]\n\"acme/x\" = []\n",
    )
    .expect("valid");
    assert_eq!(config.tools["flick/go"], ["go"]);
    assert!(config.tools["acme/x"].is_empty());
    assert!(
        parse_text("schema-version = 1\n[project]\nkind = \"lib\"\n")
            .expect("valid")
            .tools
            .is_empty()
    );
    let wrong = parse_text(
        "schema-version = 1\n[project]\nkind = \"lib\"\n[tools]\n\"flick/go\" = \"go\"\n",
    )
    .expect_err("a list of tools");
    assert_eq!(wrong.code, codes::P005);
    let Err(root) = manifest(
        "schema-version = 1\n[workspace]\nmembers = [\"a\"]\n[tools]\n\"flick/go\" = [\"go\"]\n",
    ) else {
        panic!("a workspace root grants nothing");
    };
    assert!(
        root.message.contains("`[tools]` belongs to a project"),
        "{}",
        root.message
    );
}

#[test]
fn package_when_and_host_manifests_parse_and_reject_invalid_forms() {
    let valid_pkg = "schema-version = 1\n[package]\nname = \"web-ui\"\nversion = \"1.0.0\"\nwhen = \"js && !node\"\n[project]\nkind = \"lib\"\n";
    let cfg = parse_text(valid_pkg).expect("valid package when");
    let (cond, span) = cfg.package.expect("pkg").when.expect("when");
    assert_eq!(cond, "js && !node");
    assert_eq!(&valid_pkg[span.range()], "\"js && !node\"");

    let bad_when = "schema-version = 1\n[package]\nname = \"web-ui\"\nversion = \"1.0.0\"\nwhen = \"js & node\"\n[project]\nkind = \"lib\"\n";
    let err = parse_text(bad_when).expect_err("bad when syntax");
    assert_eq!(err.code, codes::P032);
    assert_eq!(&bad_when[err.span.expect("span").range()], "&");

    let valid_host = "schema-version = 1\n[project]\nkind = \"lib\"\n[host]\nname = \"roblox\"\nbackend = \"lua\"\nrequires = \"luau && !ffi\"\ntags = [\"sandbox\", \"engine\"]\n";
    let cfg = parse_text(valid_host).expect("valid host");
    let host = cfg.host.expect("host");
    assert_eq!(host.name, "roblox");
    assert_eq!(host.backend, "lua");
    assert_eq!(
        host.requires.as_ref().map(|(c, _)| c.as_str()),
        Some("luau && !ffi")
    );
    let tag_names: Vec<&str> = host.tags.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(tag_names, ["sandbox", "engine"]);

    // `[host]` is rejected on `kind = "app"` and on workspace roots.
    let on_app = "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n[host]\nname = \"x\"\nbackend = \"js\"\n";
    assert_eq!(parse_text(on_app).expect_err("app host").code, codes::P032);

    let on_ws = "schema-version = 1\n[workspace]\nmembers = [\"a\"]\n[host]\nname = \"x\"\nbackend = \"js\"\n";
    assert_eq!(manifest(on_ws).expect_err("ws host").code, codes::P005);

    for bad in [
        "name = \"Bad\"\nbackend = \"js\"",
        "name = \"ok\"\nbackend = \"Bad/Name\"",
        "name = \"ok\"\nbackend = \"js\"\nrequires = \"\"",
        "name = \"ok\"\nbackend = \"js\"\ntags = [\"BadTag\"]",
        "name = \"ok\"\nbackend = \"js\"\ntags = [\"dup\", \"dup\"]",
    ] {
        let text = format!("schema-version = 1\n[project]\nkind = \"lib\"\n[host]\n{bad}\n");
        let err = parse_text(&text).expect_err(bad);
        assert_eq!(err.code, codes::P032, "{bad}: {}", err.message);
    }
}
