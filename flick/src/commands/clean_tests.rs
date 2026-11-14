#![cfg(unix)]

use std::os::unix::fs::symlink;
use std::{env, fs, process};

use clap::Parser;
use flick_core::diagnostics::codes;
use flick_core::project::cache::{MANIFEST, Manifest};

use super::clean::clean;
use crate::args::{CommandArguments, Commands};

#[test]
fn clean_preflights_every_recorded_path_before_deleting_any_output() {
    for (case, stale) in [
        ("escape", "../../outside/kept.js"),
        ("redirect", "redirect/kept.js"),
    ] {
        let root = env::temp_dir().join(format!("flick-clean-{case}-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("output/second")).expect("output");
        fs::create_dir_all(root.join("outside")).expect("outside");
        fs::write(
            root.join("flick.toml"),
            "schema-version = 1\n[project]\nkind = \"lib\"\n",
        )
        .expect("project");
        fs::write(root.join("output/old.js"), "previous build").expect("old output");
        fs::write(root.join("outside/kept.js"), "owned elsewhere").expect("outside file");
        symlink(root.join("outside"), root.join("output/second/redirect")).expect("symlink");
        let mut manifest = Manifest::default();
        manifest.record("output", vec!["old.js".into()]);
        // A later output directory must also be checked before the first is touched.
        manifest.record("output/second", vec![stale.into()]);
        let path = root.join(MANIFEST);
        fs::create_dir_all(path.parent().expect("parent")).expect("manifest directory");
        let original = serde_json::to_vec(&manifest).expect("manifest JSON");
        fs::write(&path, &original).expect("manifest");
        let args = CommandArguments::parse_from([
            "flick",
            "--project",
            root.to_str().expect("UTF-8 path"),
            "clean",
        ]);
        let Commands::Clean(options) = args.cmd else {
            panic!("clean arguments")
        };
        let outcome = clean(&options, &args.global);
        assert_eq!(outcome.diagnostics.len(), 1, "unsafe manifest is refused");
        assert_eq!(outcome.diagnostics[0].code, codes::P013);
        assert_eq!(
            fs::read(root.join("output/old.js")).expect("old remains"),
            b"previous build"
        );
        assert_eq!(
            fs::read(root.join("outside/kept.js")).expect("outside remains"),
            b"owned elsewhere"
        );
        assert_eq!(fs::read(&path).expect("manifest remains"), original);
        fs::remove_dir_all(root).expect("cleanup");
    }
}
