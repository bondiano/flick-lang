use super::*;
use crate::args::{CommandArguments, Commands};
use clap::Parser;

#[test]
#[ignore = "requires just wasm-backends"]
fn a_generation_uses_the_successful_build_selection_after_config_changes() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/tmp/backend_generation_reload");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).expect("project");
    let config = "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n";
    fs::write(root.join("flick.toml"), config).expect("config");
    fs::write(
        root.join("src/App.fl"),
        "module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
    )
    .expect("source");
    let args =
        CommandArguments::parse_from(["flick", "--project", root.to_str().expect("path"), "run"]);
    let Commands::Run(mut options) = args.cmd else {
        panic!("run")
    };
    let stale = find_project(&args.global).expect("startup project");
    assert!(stale.config.build.backend.is_none());
    let mut bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/wasm32-wasip2/release/flick_test_backend.wasm"),
    )
    .expect("component fixture");
    for variant in 0..2 {
        if variant == 1 {
            bytes.extend([0, 1, 0]);
        }
        fs::write(root.join("backend.wasm"), &bytes).expect("component");
        let digest = flick_backend::sha256(&bytes);
        fs::write(root.join("flick.toml"), format!("{config}\n[build.backend]\nid = \"test/literal\"\nversion = \"1.0.0\"\nsha256 = \"{digest}\"\ncomponent = \"backend.wasm\"\n")).expect("new backend selection");
        let generation = build_into_generation(&stale, &options, &args.global, None)
            .expect("component generation");
        assert_eq!(
            generation.backend.as_ref().map(|p| p.sha256.as_str()),
            Some(digest.as_str()),
            "generation records the actual successful backend"
        );
        assert!(!generation.go);
        drop(generation);
    }
    options.devtools = Some("http://localhost:9000".into());
    assert!(
        matches!(
            build_into_generation(&stale, &options, &args.global, None),
            Err(1)
        ),
        "refreshed component selection gates devtools"
    );
    assert_eq!(
        fs::read_dir(generations(&root))
            .expect("run directory")
            .count(),
        0,
        "refusal leaves no generation"
    );
    options.devtools = None;
    fs::write(
        root.join("flick.toml"),
        format!("{config}\n[build]\neffect = \"rt\"\n"),
    )
    .expect("native RT selection");
    let generation =
        build_into_generation(&stale, &options, &args.global, None).expect("native RT generation");
    assert!(generation.backend.is_none());
    assert!(generation.rt, "runtime mode comes from the completed build");
    drop(generation);
    fs::remove_dir_all(root).expect("cleanup");
}
