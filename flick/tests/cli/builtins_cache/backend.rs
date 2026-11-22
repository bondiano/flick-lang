use super::*;

#[test]
fn project_checks_save_the_interfaces_of_the_selected_backend() {
    for backend in ["rt", "effect"] {
        let app = project(
            &format!("builtins_cache_backend/{backend}"),
            &[
                (
                    "flick.toml",
                    &format!(
                        "schema-version = 1\n[project]\nkind = \"lib\"\n[target.js]\nruntime = \"{backend}\"\n"
                    ),
                ),
                (
                    "src/App/Main.fl",
                    "module App.Main exposing (x)\nx : Int\nlet x = 1\n",
                ),
            ],
        );
        let cache = app.join("cache");
        let std = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages/std");
        let out = check(&app, &std, &cache);
        assert!(out.status.success(), "{out:?}");
        let files: Vec<_> = fs::read_dir(cache.join("flick"))
            .expect("cache")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|name| {
                name.starts_with("builtins-")
                    && Path::new(name)
                        .extension()
                        .is_some_and(|ext| ext == "jsonl")
            })
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
        assert_eq!(files[0].contains("-by-class"), backend == "rt", "{files:?}");
    }
}

#[test]
fn project_builds_save_only_the_interfaces_of_the_selected_backend() {
    let std = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages/std");
    for backend in ["rt", "effect"] {
        let app = project(
            &format!("builtins_cache_build_backend/{backend}"),
            &[
                (
                    "flick.toml",
                    &format!(
                        "schema-version = 1\n[project]\nkind = \"lib\"\n[target.js]\nruntime = \"{backend}\"\n"
                    ),
                ),
                (
                    "src/App/Main.fl",
                    "module App.Main exposing (x)\nx : Int\nlet x = 1\n",
                ),
            ],
        );
        let cache = app.join("cache");
        let out = Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["build", "--profile", "release", "--quiet"])
            .current_dir(&app)
            .env("FLICK_STD_PATH", &std)
            .env("XDG_CACHE_HOME", &cache)
            .output()
            .expect("flick runs");
        assert!(out.status.success(), "{out:?}");
        let files: Vec<_> = fs::read_dir(cache.join("flick"))
            .expect("cache")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|name| {
                name.starts_with("builtins-")
                    && Path::new(name)
                        .extension()
                        .is_some_and(|ext| ext == "jsonl")
            })
            .collect();
        assert_eq!(files.len(), 1, "{backend}: {files:?}");
        assert_eq!(
            files[0].contains("-by-class"),
            backend == "rt",
            "{backend}: {files:?}"
        );
    }
}
