//! The Lua backend component checks a program against the profile its
//! environment selects: what the program reaches, at its source, before
//! any output; and it writes no code.

use std::fs;
use std::path::{Path, PathBuf};

use super::common::{flick_in, project};

fn config(environment: &str, sha: &str, extra: &str) -> String {
    format!(
        "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n{extra}\n\
         [build]\nenvironment = \"{environment}\"\n\n\
         [build.backend]\nid = \"flick/lua\"\nversion = \"0.1.0\"\n\
         sha256 = \"{sha}\"\ncomponent = \"backend.wasm\"\n"
    )
}

/// A project pinned to the distributed Lua component.
fn lua_project(name: &str, environment: &str, app: &str, files: &[(&str, &str)]) -> PathBuf {
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../backends/lua/package/component.wasm"),
    )
    .expect("the distributed Lua component");
    let sha = flick_backend::sha256(&bytes);
    let extra = if files.is_empty() {
        String::new()
    } else {
        "[dependencies]\nhost-love12 = { path = \"hosts/love12\" }\n".to_owned()
    };
    let config = config(environment, &sha, &extra);
    let mut all = vec![("flick.toml", config.as_str()), ("src/App.fl", app)];
    all.extend_from_slice(files);
    let root = project(name, &all);
    fs::write(root.join("backend.wasm"), &bytes).expect("component");
    root
}

/// The diagnostics `--diagnostics json` printed with `code`.
fn diagnostics(stderr: &[u8], code: &str) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(stderr)
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|d| d["code"] == code)
        .collect()
}

fn slice<'a>(diagnostic: &serde_json::Value, source: &'a str) -> &'a str {
    let at = |key: &str| {
        usize::try_from(diagnostic["range"][key].as_u64().expect("a range")).expect("fits")
    };
    &source[at("start")..at("end")]
}

const REACHED: &str = "module App exposing (main)\n\n\
    big : Unit -> BigInt\nlet big () = 2n * 3n\n\n\
    main : Unit -> Unit\nlet main () = if big () == big () then () else ()\n";

const UNREACHED: &str = "module App exposing (main)\n\n\
    big : Unit -> BigInt\nlet big () = 2n * 3n\n\n\
    main : Unit -> Unit\nlet main () = ()\n";

#[test]
#[ignore = "requires just wasm-backends"]
fn a_reachable_big_integer_is_refused_at_its_source() {
    let root = lua_project("lua_profile_bigint", "lua/love", REACHED, &[]);
    for verb in ["check", "build"] {
        let out = flick_in(&root, &[verb, "--diagnostics", "json", "--offline"]);
        assert_eq!(out.status.code(), Some(1), "{verb}: {out:?}");
        let found = diagnostics(&out.stderr, "FLICK-P012");
        let big = found
            .iter()
            .find(|d| {
                d["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("`BigInt` is not available"))
            })
            .unwrap_or_else(|| panic!("{verb}: {}", String::from_utf8_lossy(&out.stderr)));
        assert!(
            big["message"]
                .as_str()
                .is_some_and(|m| m.contains("lua/love")),
            "{big}"
        );
        assert_eq!(slice(big, REACHED), "2n * 3n", "{verb}");
    }
    assert!(!root.join("dist").exists(), "nothing is written");
}

#[test]
#[ignore = "requires just wasm-backends"]
fn an_unreachable_big_integer_passes_the_check() {
    let root = lua_project("lua_profile_unreached", "lua/love", UNREACHED, &[]);
    let out = flick_in(&root, &["check", "--diagnostics", "json", "--offline"]);
    assert!(out.status.success(), "{out:?}");
    assert!(diagnostics(&out.stderr, "FLICK-P012").is_empty(), "{out:?}");
}

#[test]
#[ignore = "requires just wasm-backends"]
fn a_profile_the_backend_does_not_know_is_refused() {
    let host = "schema-version = 1\n[package]\nname = \"host-love12\"\nversion = \"0.1.0\"\n\
        [project]\nkind = \"lib\"\n[host]\nname = \"love-12\"\nbackend = \"lua\"\ntags = [\"love12\"]\n";
    let root = lua_project(
        "lua_profile_unknown",
        "lua/love-12",
        UNREACHED,
        &[
            ("hosts/love12/flick.toml", host),
            (
                "hosts/love12/src/Love12.fl",
                "module Love12 exposing (x)\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    for verb in ["check", "build"] {
        let out = flick_in(&root, &[verb, "--diagnostics", "json", "--offline"]);
        assert_eq!(out.status.code(), Some(1), "{verb}: {out:?}");
        let found = diagnostics(&out.stderr, "FLICK-P012");
        assert!(
            found.iter().any(|d| d["message"]
                .as_str()
                .is_some_and(|m| m.contains("the profile `lua/love-12` is not available"))),
            "{verb}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
#[ignore = "requires just wasm-backends"]
fn generation_is_refused_and_writes_nothing() {
    let root = lua_project("lua_profile_generate", "lua/wow-retail", UNREACHED, &[]);
    assert!(flick_in(&root, &["check", "--offline"]).status.success());
    let out = flick_in(&root, &["build", "--diagnostics", "json", "--offline"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let found = diagnostics(&out.stderr, "FLICK-P012");
    assert!(
        found.iter().any(|d| d["message"]
            .as_str()
            .is_some_and(|m| m.contains("code generation is not implemented yet"))),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!root.join("dist").exists(), "nothing is written");
}

#[test]
#[ignore = "requires just wasm-backends"]
fn a_wow_global_outside_the_api_snapshot_is_refused_at_its_use() {
    let app = "module App exposing (main)\n\n\
        open_file : String ->{} Unit\nforeign open_file from global \"io.open\"\n  arity 1\n\n\
        main : Unit ->{} Unit\nlet main () = open_file \"x\"\n";
    let root = lua_project("lua_profile_snapshot", "lua/wow-retail", app, &[]);
    let out = flick_in(&root, &["check", "--diagnostics", "json", "--offline"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let found = diagnostics(&out.stderr, "FLICK-P012");
    let refused = found
        .iter()
        .find(|d| {
            d["message"]
                .as_str()
                .is_some_and(|m| m.contains("not in the API snapshot"))
        })
        .unwrap_or_else(|| panic!("{}", String::from_utf8_lossy(&out.stderr)));
    assert_eq!(slice(refused, app), "open_file");
}
