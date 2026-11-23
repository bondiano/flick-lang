use crate::common;

use common::{flick, stdout};

#[test]
fn version() {
    insta::assert_snapshot!(stdout(&["--version"]), @"flick 0.1.0");
}

#[test]
fn help_describes_global_and_build_options() {
    for command in [None, Some("check"), Some("build"), Some("fmt")] {
        let (help, flag) = match command {
            Some(command) => (stdout(&["help", command]), stdout(&[command, "--help"])),
            None => (stdout(&["help"]), stdout(&["--help"])),
        };
        assert_eq!(help, flag);
        for option in ["--quiet", "--silent", "--verbose", "--timings"] {
            assert!(help.contains(option), "{help}");
        }
        if command == Some("build") {
            for option in [
                "--inputs",
                "--target",
                "Options of `flick/js`",
                "--typescript",
                "--declarations",
                "--runtime",
            ] {
                assert!(help.contains(option), "{help}");
            }
        }
    }
    assert_eq!(
        flick(&["--silent", "--verbose", "build"]).status.code(),
        Some(2)
    );
}

#[test]
fn help_lists_std_schema_export_as_contributed_and_not_core() {
    let help = stdout(&["--help"]);
    let core_section = help
        .split("Commands of `std`:")
        .next()
        .expect("core help before contributed commands");
    assert!(
        !core_section
            .lines()
            .any(|line| line.trim_start().starts_with("schema")),
        "schema must not remain a core command:\n{core_section}"
    );
    assert!(
        help.contains("Commands of `std`:") && help.contains("schema export"),
        "expected contributed std commands in --help:\n{help}"
    );

    for args in [
        &["schema", "export", "--help"][..],
        &["std:schema", "export", "--help"][..],
    ] {
        let out = flick(args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.contains("--target") && text.contains("--out"),
            "{args:?}: {text}"
        );
    }

    let doc_help = stdout(&["doc", "--help"]);
    assert!(
        doc_help.contains("--check-examples") && doc_help.contains("--target"),
        "{doc_help}"
    );

    let pack_help = stdout(&["pack", "--help"]);
    assert!(
        pack_help.contains("--out-dir") && pack_help.contains("--target"),
        "{pack_help}"
    );
}

#[test]
fn bad_invocation_exits_2() {
    assert_eq!(flick(&["frobnicate"]).status.code(), Some(2));
    assert_eq!(flick(&["check"]).status.code(), Some(2));
    assert_eq!(
        flick(&["--color", "sometimes", "check", "x.fl"])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn core_help_mentions_no_target_ecosystem() {
    let verbs = [
        None,
        Some("check"),
        Some("prove"),
        Some("build"),
        Some("fmt"),
        Some("test"),
        Some("new"),
        Some("init"),
        Some("run"),
        Some("dev"),
        Some("clean"),
        Some("install"),
        Some("add"),
        Some("remove"),
        Some("update"),
        Some("deps"),
        Some("plugins"),
        Some("pack"),
        Some("diff"),
        Some("doc"),
        Some("explain"),
        Some("expand"),
        Some("doctor"),
        Some("inspect"),
        Some("lsp"),
    ];
    let forbidden_substrings = [
        "npm",
        "pnpm",
        "node_modules",
        "package.json",
        "TypeScript",
        "Vite",
        "JavaScript",
        "`go`",
        "`js`",
    ];
    for verb in verbs {
        let full = match verb {
            Some(v) => stdout(&[v, "--help"]),
            None => stdout(&["--help"]),
        };
        let core = full
            .split("\nOptions of ")
            .next()
            .unwrap_or(&full)
            .split("\nCommands of ")
            .next()
            .unwrap_or(&full);
        for term in forbidden_substrings {
            assert!(
                !core.contains(term),
                "core help for `{verb:?}` must not mention `{term}`:\n{core}"
            );
        }
        for word in ["Node", "Go"] {
            let found = core
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|tok| tok == word);
            assert!(
                !found,
                "core help for `{verb:?}` must not mention `{word}`:\n{core}"
            );
        }
    }
}

#[test]
fn core_cli_source_stays_backend_neutral() {
    use std::fs;
    use std::path::Path;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let allowed_boundary = [
        "flick/src/backends/tools.rs",
        "flick/src/backends/official.rs",
        "flick/src/backends/options/mod.rs",
        "flick/src/backends/options/cache.rs",
        "flick/src/backends/options/scan.rs",
        "flick/src/backends/request.rs",
        "flick/src/backends/request/javascript.rs",
        "flick/src/backends/request/javascript_facts.rs",
        "flick/src/backends/request/go.rs",
        "flick/src/backends/request/assets.rs",
        "flick/src/commands/build/native.rs",
        "flick/src/commands/build/js_artifacts.rs",
        "flick/src/commands/build/shipped.rs",
        "flick/src/commands/inspect.rs",
        "flick/src/commands/scaffold/source.rs",
        "flick-core/src/project/config.rs",
    ];
    let forbidden = [
        "node_modules",
        "package.json",
        "pnpm",
        "vite",
        "@flick-lang/",
        "host-node-devtools",
        "host-schema-aot",
        "host-source-build",
        "js.effect-bridge",
    ];
    let mut stack = vec![root.join("src"), root.join("../flick-core/src/project")];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "tests") {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if file_name == "tests.rs" || file_name.ends_with("_tests.rs") {
                continue;
            }
            let rel = path
                .strip_prefix(root.parent().expect("workspace root"))
                .expect("relative")
                .to_string_lossy()
                .replace('\\', "/");
            if allowed_boundary.contains(&rel.as_str()) {
                continue;
            }
            let text = fs::read_to_string(&path).expect("read rs");
            let prod = text.split("#[cfg(test)]").next().unwrap_or(&text);
            for token in forbidden {
                assert!(
                    !prod.contains(token),
                    "core file `{rel}` must not contain `{token}`"
                );
            }
        }
    }
}

#[test]
fn cli_reference_core_sections_mention_no_target_ecosystem() {
    use std::fs;
    use std::path::Path;

    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let pages = [
        (
            "docs/src/content/docs/reference/cli.md",
            "\n## Plugin-contributed commands and backends\n",
        ),
        (
            "docs/src/content/docs/ru/reference/cli.md",
            "\n## Команды плагинов и официальные бэкенды\n",
        ),
    ];
    let forbidden_ci = [
        "npm",
        "pnpm",
        "pnpm-lock.yaml",
        "node_modules",
        "package.json",
        "typescript",
        "javascript",
        "vite",
        "flick/js",
        "flick/go",
        "--target js",
        "--target go",
        "--emit js",
        "--emit ts",
        "--emit go",
        "`js`",
        "`go`",
        "`node`",
        "ffi import",
        "ffi check",
        "content-mapper",
        "schema export",
    ];
    for (rel, heading) in pages {
        let text = fs::read_to_string(workspace.join(rel)).expect("read cli reference");
        let (core, contributed) = text
            .split_once(heading)
            .unwrap_or_else(|| panic!("{rel} must contain plugin-contributed section `{heading}`"));
        let core_lower = core.to_ascii_lowercase();
        for term in forbidden_ci {
            assert!(
                !core_lower.contains(term),
                "core sections of `{rel}` must not mention `{term}`"
            );
        }
        for word in ["Node", "Go"] {
            let found = core
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|tok| tok == word);
            assert!(!found, "core sections of `{rel}` must not mention `{word}`");
        }
        for contributor in ["`std`", "`flick/js`", "`flick/go`"] {
            assert!(
                contributed.contains(contributor),
                "plugin-contributed section of `{rel}` must document {contributor}"
            );
        }
    }
}
