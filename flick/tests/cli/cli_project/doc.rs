use super::*;
use flick_core::doc;

#[test]
fn doc_shows_source_types_kinds_constraints_effects_and_captures() {
    let dir = project("cli_doc", &[("flick.toml", LIB), ("src/Lib/Api.fl", API)]);
    let out = flick_in(&dir, &["doc", "--out-dir", "api"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
    let page = fs::read_to_string(dir.join("api/Lib.Api.md")).expect("page");
    for expected in [
        "### `Shape`",
        "Kind: `Type`",
        "Circle : Float -> Shape",
        "A shape.",
        "service Store =",
        "put : String ->{; StoreError} Unit",
        "class Named a =",
        "fetch : Named a => List a ->{Store; StoreError} Unit",
        "keep : (Unit -> Float)^c -> Unit -> Float",
        "The area.",
    ] {
        assert!(page.contains(expected), "`{expected}` in\n{page}");
    }
    assert!(
        fs::read_to_string(dir.join("api/index.md"))
            .expect("index")
            .contains("Lib.Api")
    );
}

#[test]
fn doc_lists_only_public_modules_after_a_private_reexport() {
    let dir = project(
        "cli_doc_private",
        &[
            ("flick.toml", LIB),
            (
                "src/Lib/Hidden.fl",
                "private module Lib.Hidden exposing (answer)\nanswer : Int\nlet answer = 42\n",
            ),
            (
                "src/Lib/Api.fl",
                "module Lib.Api exposing (answer)\nimport Lib.Hidden exposing (answer)\n",
            ),
        ],
    );
    let out = flick_in(&dir, &["doc", "--out-dir", "api"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
    assert!(dir.join("api/Lib.Api.md").is_file());
    assert!(!dir.join("api/Lib.Hidden.md").exists());
    let index = fs::read_to_string(dir.join("api/index.md")).expect("index");
    assert!(
        index.contains("Lib.Api") && !index.contains("Lib.Hidden"),
        "{index}"
    );
}

#[test]
fn doc_uses_reexport_provenance_without_leaking_private_declarations() {
    let dir = project(
        "cli_doc_provenance",
        &[
            ("flick.toml", LIB),
            (
                "src/Lib/Hidden.fl",
                "private module Lib.Hidden exposing (answer)\n/// The public answer.\nanswer : Int\nlet answer = 42\n/// SECRET COMMENT\nsecret : Int\nlet secret = 0\n",
            ),
            (
                "src/Lib/Api.fl",
                "module Lib.Api exposing (answer)\nimport Lib.Hidden exposing (answer)\n",
            ),
        ],
    );
    let out = flick_in(
        &dir,
        &[
            "doc",
            "--out-dir",
            "api",
            "--source-base-url",
            "https://example.test/blob/revision/",
        ],
    );
    assert_eq!(code(&out), Some(0), "{out:?}");
    let page = fs::read_to_string(dir.join("api/Lib.Api.md")).expect("page");
    assert!(page.contains("The public answer."), "{page}");
    assert!(
        page.contains("https://example.test/blob/revision/src/Lib/Hidden.fl#L4"),
        "{page}"
    );
    assert!(
        !page.contains("SECRET COMMENT") && !page.contains("### `secret`"),
        "{page}"
    );
    let plain = flick_in(&dir, &["doc", "--out-dir", "plain"]);
    assert_eq!(code(&plain), Some(0), "{plain:?}");
    assert!(
        !fs::read_to_string(dir.join("plain/Lib.Api.md"))
            .expect("test fixture")
            .contains("[Source]")
    );
}

#[test]
fn doc_checks_examples_with_real_package_imports() {
    let dir = project(
        "cli_doc_examples",
        &[
            ("flick.toml", LIB),
            (
                "src/Lib/Api.fl",
                "module Lib.Api exposing (answer)\n/// ```flick\n/// import Lib.Api exposing (answer)\n/// value : Int\n/// let value = answer\n/// ```\nanswer : Int\nlet answer = 42\n",
            ),
        ],
    );
    let out = flick_in(&dir, &["doc", "--out-dir", "api", "--check-examples"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
    let source = fs::read_to_string(dir.join("src/Lib/Api.fl")).expect("test fixture");
    fs::write(
        dir.join("src/Lib/Api.fl"),
        source.replace("/// value : Int", "/// value : String"),
    )
    .expect("test fixture");
    let out = flick_in(&dir, &["doc", "--out-dir", "api", "--check-examples"]);
    assert_ne!(code(&out), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("documentation example"),
        "{out:?}"
    );
}

#[test]
fn doc_ignored_module_example_must_still_parse() {
    let valid = "/// ```flick ignore Fragment deliberately has an undefined name.\n/// let value = unknown\n/// ```\nmodule Lib.Api exposing (answer)\nanswer : Int\nlet answer = 42\n";
    let dir = project(
        "cli_doc_ignore",
        &[("flick.toml", LIB), ("src/Lib/Api.fl", valid)],
    );
    let out = flick_in(&dir, &["doc", "--out-dir", "api", "--check-examples"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
    let page = fs::read_to_string(dir.join("api/Lib.Api.md")).expect("test fixture");
    assert!(page.contains("let value = unknown"), "{page}");
    fs::write(
        dir.join("src/Lib/Api.fl"),
        valid.replace("let value = unknown", "let value = ("),
    )
    .expect("test fixture");
    let out = flick_in(&dir, &["doc", "--out-dir", "api", "--check-examples"]);
    assert_ne!(code(&out), Some(0), "{out:?}");
    assert!(stderr(&out).contains("does not parse"), "{out:?}");
}

#[test]
fn doc_standard_library_uses_checked_builtin_interfaces() {
    let dir = project(
        "cli_doc_std",
        &[("flick.toml", LIB), ("src/Lib/Api.fl", API)],
    );
    let out = flick_in(
        &dir,
        &[
            "doc",
            "--std",
            "--out-dir",
            "api",
            "--source-base-url",
            "https://example.test/std",
        ],
    );
    assert_eq!(code(&out), Some(0), "{out:?}");
    let page = fs::read_to_string(dir.join("api/String.md")).expect("String page");
    assert!(page.contains("### `length`"), "{page}");
    assert!(
        page.contains("https://example.test/std/packages/core/src/String.fl#L"),
        "{page}"
    );
    let relative = "packages/core/src/String.fl";
    assert!(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(relative)
            .is_file()
    );
    assert!(!dir.join("api/Lib.Api.md").exists());
}

#[test]
fn doc_does_not_publish_external_private_declarations() {
    let manifest = format!("{LIB}\n[dependencies]\ndep = {{ path = \"dep\" }}\n");
    let dir = project(
        "cli_doc_external_private",
        &[
            ("flick.toml", &manifest),
            (
                "src/Lib/Api.fl",
                "module Lib.Api exposing (answer)\nimport Dep.Api exposing (answer)\n",
            ),
            ("dep/flick.toml", LIB),
            (
                "dep/src/Dep/Hidden.fl",
                "private module Dep.Hidden exposing (answer)\n/// Public answer from dependency.\nanswer : Int\nlet answer = 42\n/// EXTERNAL SECRET\nsecret : Int\nlet secret = 0\n",
            ),
            (
                "dep/src/Dep/Api.fl",
                "module Dep.Api exposing (answer)\nimport Dep.Hidden exposing (answer)\n",
            ),
        ],
    );
    let out = flick_in(&dir, &["doc", "--out-dir", "api"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
    let page = fs::read_to_string(dir.join("api/Lib.Api.md")).expect("page");
    assert!(page.contains("Public answer from dependency."), "{page}");
    for entry in fs::read_dir(dir.join("api")).expect("test fixture") {
        let entry = entry.expect("test fixture");
        let text = fs::read_to_string(entry.path()).expect("test fixture");
        assert!(
            !text.contains("EXTERNAL SECRET") && !text.contains("### `secret`"),
            "{text}"
        );
    }
    assert!(!dir.join("api/Dep.Hidden.md").exists());
}

#[test]
fn doc_builtin_reexport_keeps_comment_without_inventing_project_source_url() {
    let dir = project(
        "cli_doc_builtin_reexport",
        &[
            ("flick.toml", LIB),
            (
                "src/Lib/Api.fl",
                "module Lib.Api exposing (length)\nimport String exposing (length)\n",
            ),
        ],
    );
    let out = flick_in(
        &dir,
        &[
            "doc",
            "--out-dir",
            "api",
            "--source-base-url",
            "https://example.test/project",
        ],
    );
    assert_eq!(code(&out), Some(0), "{out:?}");
    let page = fs::read_to_string(dir.join("api/Lib.Api.md")).expect("page");
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages/core/src/String.fl"),
    )
    .expect("test fixture");
    let comment = doc::comments(&source)
        .remove("length")
        .expect("length comment");
    assert!(page.contains(&comment), "{page}");
    assert!(!page.contains("[Source]"), "{page}");
}
