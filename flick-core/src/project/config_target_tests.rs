use super::*;

#[test]
fn target_tables_and_build_target_parse_open_names() {
    let config = parse_text(
        "schema-version = 1\n[project]\nkind = \"lib\"\n\
         [build]\ntarget = \"js\"\n\
         [target.js]\ntypescript = true\ndeclarations = true\npromise-facade = true\n\
         runtime = \"effect\"\nui = \"./renderer.mjs\"\n\
         [target.go]\ncgo = true\n\
         [target.custom_1]\ntags = [\"a\", \"b\"]\n",
    )
    .expect("valid");
    assert_eq!(config.build.target.as_deref(), Some("js"));
    assert_eq!(config.default_target(), "js");
    assert!(!config.js_is_rt());
    assert!(config.js_flag("typescript"));
    assert!(config.js_flag("declarations"));
    assert!(config.js_flag("promise-facade"));
    assert_eq!(config.targets["go"]["cgo"].value, TargetValue::Bool(true));
    assert!(matches!(
        &config.targets["custom_1"]["tags"].value,
        TargetValue::List(items) if items.len() == 2 && items[0].0 == "a" && items[1].0 == "b"
    ));
}

#[test]
fn removed_build_keys_fail_with_p057_and_replacement_hint() {
    let cases = [
        ("emit = \"js\"", "\"js\"", "[build] target = \"js\""),
        ("emit = \"ts\"", "\"ts\"", "[target.js] typescript = true"),
        ("emit = \"go\"", "\"go\"", "[build] target = \"go\""),
        ("emit = \"wasm\"", "\"wasm\"", "[build] target = \"wasm\""),
        ("emit-dts = true", "true", "[target.js] declarations = true"),
        (
            "promise-facade = true",
            "true",
            "[target.js] promise-facade = true",
        ),
        ("schema-aot = true", "true", "[target.js] schema-aot = true"),
        (
            "effect = \"effect\"",
            "\"effect\"",
            "[target.js] runtime = \"effect\"",
        ),
        (
            "ui = \"./renderer.mjs\"",
            "\"./renderer.mjs\"",
            "[target.js] ui = \"./renderer.mjs\"",
        ),
    ];
    for (line, spanned, expected_hint) in cases {
        let text = format!("schema-version = 1\n[project]\nkind = \"lib\"\n[build]\n{line}\n");
        let err = parse_text(&text).expect_err(line);
        assert_eq!(err.code, codes::P057, "{line}: {}", err.message);
        let span = err.span.expect("span on removed key");
        assert_eq!(&text[span.range()], spanned, "{line}");
        let hints: Vec<&str> = err.fixes.iter().map(|f| f.message.as_str()).collect();
        assert!(
            hints.iter().any(|h| h.contains(expected_hint)),
            "{line}: expected {expected_hint:?} in {hints:?}"
        );
    }
}

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
fn target_dependencies_parse_alongside_target_options_and_reject_old_js_table() {
    let config = parse_text(
        "\
schema-version = 1

[project]
kind = \"lib\"

[target.js]
runtime = \"effect\"
declarations = true

[target.js.dependencies]
left-pad = \"^1.3.0\"
\"@acme/util\" = \"2.0.0\"

[target.literal.dependencies]
widget = \"1.0.0\"
",
    )
    .expect("valid target options and dependencies");
    assert!(config.targets["js"].contains_key("runtime"));
    assert!(config.targets["js"].contains_key("declarations"));
    assert!(
        !config.targets["js"].contains_key("dependencies"),
        "dependencies must be extracted out of target options: {:?}",
        config.targets["js"]
    );
    assert!(config.targets["literal"].is_empty());
    let deps: Vec<(&str, &str, &str)> = config
        .target_dependencies
        .iter()
        .map(|d| (d.target.as_str(), d.name.as_str(), d.version.as_str()))
        .collect();
    assert_eq!(
        deps,
        [
            ("js", "@acme/util", "2.0.0"),
            ("js", "left-pad", "^1.3.0"),
            ("literal", "widget", "1.0.0"),
        ]
    );

    // Non-table `[target.js] dependencies = "bad"` and non-string values are P005.
    let bad_table = parse_text(
        "schema-version = 1\n[project]\nkind = \"lib\"\n[target.js]\ndependencies = \"bad\"\n",
    )
    .expect_err("dependencies must be a table");
    assert_eq!(bad_table.code, codes::P005);
    assert!(bad_table.span.is_some());

    let bad_version = parse_text(
        "schema-version = 1\n[project]\nkind = \"lib\"\n[target.js.dependencies]\nleft-pad = 123\n",
    )
    .expect_err("version must be a string");
    assert_eq!(bad_version.code, codes::P005);
    assert!(bad_version.span.is_some());

    // Removed `[js]` and `[js.dependencies]` report FLICK-P057 with a migration fix.
    for old in [
        "schema-version = 1\n[project]\nkind = \"lib\"\n[js.dependencies]\nleft-pad = \"^1.3.0\"\n",
        "schema-version = 1\n[project]\nkind = \"lib\"\n[js]\n",
    ] {
        let removed = parse_text(old).expect_err(old);
        assert_eq!(removed.code, codes::P057, "{old}: {:?}", removed.message);
        assert!(removed.span.is_some(), "{old}");
        assert!(
            removed
                .fixes
                .iter()
                .any(|f| f.message.contains("[target.js.dependencies]")),
            "{old}: {:?}",
            removed.fixes
        );
    }

    // Workspace root rejects `[target.js.dependencies]` as a project-only `[target]` table.
    let Err(root) = manifest(
        "schema-version = 1\n[workspace]\nmembers = [\"a\"]\n[target.js.dependencies]\nleft-pad = \"^1.3.0\"\n",
    ) else {
        panic!("workspace root cannot declare target dependencies");
    };
    assert_eq!(root.code, codes::P005);
    assert!(
        root.message.contains("`[target]` belongs to a project"),
        "{}",
        root.message
    );
}
