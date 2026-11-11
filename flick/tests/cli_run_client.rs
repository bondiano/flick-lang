//! Client entries run in a browser through Vite, not in Node.

mod common;

#[cfg(unix)]
#[test]
fn client_entry_builds_and_previews_with_vite() {
    use std::fs;

    let config = "schema-version = 1\n[project]\nkind = \"app\"\nentries = { web = { module = \"App.Web\", runner = \"Runner.Browser\", target = \"client\" } }\n";
    let dir = common::project("run_client", &[("flick.toml", config)]);
    fs::create_dir_all(dir.join("node_modules/.bin")).expect("dir");
    common::write_script(
        &dir.join("node_modules/.bin/vite"),
        "#!/bin/sh\ncase \"$1\" in\n  build) printf 'vite-built\\n' ;;\n  preview) printf 'vite-preview\\n' ;;\n  *) exit 9 ;;\nesac\n",
    );

    let out = common::flick_in(&dir, &["run"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "vite-built\nvite-preview\n"
    );
}
