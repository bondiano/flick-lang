//! The Lua backend component checks a program against the profile its
//! environment selects: what the program reaches, at its source, before
//! any output. Then it writes a Lua chunk per reached module, the same at
//! each build, or refuses what it cannot generate and keeps the old output.

use std::fmt::Write as _;
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

const SHAPES: &str = "module App exposing (main)\n\n\
    type Shape = Circle Float | Square Float\n\n\
    area : Shape -> Float\nlet area s =\n  match s with\n  | Circle r -> r * r\n  | Square a -> a * a\n\n\
    main : Unit -> Unit\nlet main () = if area (Circle 2.0) > 1.0 then ()\n";

/// Every file of `dist`, by its path there.
fn dist(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in fs::read_dir(dir).expect("a directory").flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                let name = path.strip_prefix(base).expect("in dist");
                let name = name.to_string_lossy().replace('\\', "/");
                out.push((name, fs::read(&path).expect("a file")));
            }
        }
    }
    let mut out = Vec::new();
    walk(&root.join("dist"), &root.join("dist"), &mut out);
    out.sort();
    out
}

fn chunk(root: &Path) -> String {
    fs::read_to_string(root.join("dist/App.lua")).expect("the chunk of App")
}

#[test]
#[ignore = "requires just wasm-backends"]
fn a_build_writes_a_chunk_per_module_and_a_rebuild_is_identical() {
    let root = lua_project("lua_profile_generate", "lua/wow-retail", SHAPES, &[]);
    let out = flick_in(&root, &["build", "--offline"]);
    assert!(out.status.success(), "{out:?}");
    let first = dist(&root);
    let paths: Vec<&str> = first.iter().map(|(p, _)| p.as_str()).collect();
    assert!(
        paths.contains(&"App.lua") && paths.contains(&"App.lua.map"),
        "{paths:?}"
    );
    // WoW loads the files its `.toc` lists: no loader, and no other target
    assert!(
        !paths.iter().any(|p| {
            Path::new(p)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("js"))
                || *p == "main.lua"
        }),
        "{paths:?}"
    );
    let app = chunk(&root);
    assert!(
        app.starts_with("local _, ns = ...\nlocal K = ns.k\nlocal M = {}\nns.App = M\n"),
        "{app}"
    );
    assert!(app.contains("M.area = function(s)"), "{app}");
    let order = fs::read_to_string(root.join("dist/chunks.txt")).expect("the load order");
    assert_eq!(
        order,
        format!("{KERNEL}App.lua\n"),
        "the kernel loads first"
    );
    let map = fs::read_to_string(root.join("dist/App.lua.map")).expect("the map");
    assert!(map.contains(r#""sources":["src/App.fl"]"#), "{map}");
    // a build that generates again writes the same bytes
    assert!(flick_in(&root, &["clean", "--cache"]).status.success());
    let again = flick_in(&root, &["build", "--offline"]);
    assert!(again.status.success(), "{again:?}");
    assert_eq!(dist(&root), first);
    // LÖVE starts `main.lua`, which loads the chunks in order
    let love = lua_project("lua_profile_love", "lua/love", SHAPES, &[]);
    assert!(flick_in(&love, &["build", "--offline"]).status.success());
    let main = fs::read_to_string(love.join("dist/main.lua")).expect("the loader");
    let loads: Vec<String> = (KERNEL.lines().chain(["App.lua"]))
        .map(|p| format!("assert(love.filesystem.load(\"{p}\"))(nil, ns)\n"))
        .collect();
    assert_eq!(main, format!("local ns = {{}}\n{}", loads.concat()));
}

/// The chunks of the Lua kernel, in the order they load.
const KERNEL: &str = "_flick/lua-kernel/core.lua\n_flick/lua-kernel/strings.lua\n\
                      _flick/lua-kernel/lists.lua\n_flick/lua-kernel/bytes.lua\n\
                      _flick/lua-kernel/lang.lua\n";

/// `wide` with `n` locals besides its parameter, all of them used.
fn wide(n: usize) -> String {
    let mut lets = String::new();
    for i in 0..n {
        let _ = writeln!(lets, "  let x{i} = {i} == {i}");
    }
    let all: Vec<String> = (0..n).map(|i| format!("x{i}")).collect();
    format!(
        "module App exposing (main)\n\nwide : Unit -> Bool\nlet wide () =\n{lets}  {}\n\n\
         main : Unit -> Unit\nlet main () = if wide () then ()\n",
        all.join(" && ")
    )
}

#[test]
#[ignore = "requires just wasm-backends"]
fn what_cannot_be_generated_is_refused_at_its_source_and_keeps_the_output() {
    let root = lua_project("lua_profile_keep", "lua/wow-retail", SHAPES, &[]);
    assert!(flick_in(&root, &["build", "--offline"]).status.success());
    let before = dist(&root);
    // `**` of floats has special cases the Lua kernel does not compute yet
    let sum = "module App exposing (main)\n\nmain : Unit -> Unit\n\
               let main () = if 2.0 ** 3.0 == 8.0 then ()\n";
    let limit = wide(200);
    // one past the 200 locals of Lua 5.1, at the local that is one too many
    for (source, message, at) in [
        (
            sum,
            "`**` of floats is not available on the lua/wow-retail target",
            "2.0 ** 3.0",
        ),
        (limit.as_str(), "201 locals in scope at once", "199 == 199"),
    ] {
        fs::write(root.join("src/App.fl"), source).expect("source");
        let out = flick_in(&root, &["build", "--diagnostics", "json", "--offline"]);
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        let found = diagnostics(&out.stderr, "FLICK-P012");
        let refused = (found.iter())
            .find(|d| d["message"].as_str().is_some_and(|m| m.contains(message)))
            .unwrap_or_else(|| panic!("{}", String::from_utf8_lossy(&out.stderr)));
        assert_eq!(slice(refused, source), at);
        assert_eq!(dist(&root), before, "the earlier output stays");
    }
    // at the limit the program is generated
    fs::write(root.join("src/App.fl"), wide(199)).expect("source");
    assert!(flick_in(&root, &["build", "--offline"]).status.success());
    // and a refused first build writes nothing at all
    let fresh = lua_project("lua_profile_fresh", "lua/wow-retail", sum, &[]);
    let out = flick_in(&fresh, &["build", "--diagnostics", "json", "--offline"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        !diagnostics(&out.stderr, "FLICK-P012").is_empty(),
        "{out:?}"
    );
    assert!(!fresh.join("dist").exists(), "nothing is written");
}

/// `flick run`, and `flick test` without an interpreter or on World of Warcraft.
#[test]
#[ignore = "requires just wasm-backends"]
fn run_is_refused_because_the_host_runs_the_program_and_test_needs_an_interpreter() {
    let love = lua_project("lua_profile_run", "lua/love", SHAPES, &[]);
    let wow = lua_project("lua_profile_test_wow", "lua/wow-retail", SHAPES, &[]);
    for (root, verb, expected) in [
        (
            &love,
            "run",
            "`flick run` is not available on the lua/love target yet: a Lua program runs in its \
             host, World of Warcraft or LÖVE",
        ),
        (
            &love,
            "test",
            "`flick test` on the lua/love target with the dialect `lua51` needs `luajit` or `lua` \
             on PATH, allowed by `[tools]` or `--allow-tool`",
        ),
        (
            &wow,
            "test",
            "`flick test` is not available on the lua/wow-retail target yet: its tests run on a \
             Lua interpreter as LÖVE's, and no World of Warcraft client runs them here",
        ),
    ] {
        let out = flick_in(root, &[verb, "--diagnostics", "json", "--offline"]);
        assert_eq!(out.status.code(), Some(1), "{verb}: {out:?}");
        let found = diagnostics(&out.stderr, "FLICK-P012");
        assert!(
            found
                .iter()
                .any(|d| d["message"].as_str().is_some_and(|m| m.contains(expected))),
            "{verb}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        // the runtime is not what is missing
        assert!(!String::from_utf8_lossy(&out.stderr).contains("runtime on PATH"));
    }
}

#[test]
#[ignore = "requires just wasm-backends"]
fn a_dialect_the_host_does_not_run_is_refused_not_replaced() {
    let root = lua_project("lua_profile_dialect", "lua/wow-retail", SHAPES, &[]);
    assert!(flick_in(&root, &["build", "--offline"]).status.success());
    let before = dist(&root);
    let pinned = fs::read_to_string(root.join("flick.toml")).expect("config");
    let luajit = pinned.replace("[build]", "[target.lua]\ndialect = \"luajit\"\n\n[build]");
    fs::write(root.join("flick.toml"), luajit).expect("config");
    let out = flick_in(&root, &["build", "--diagnostics", "json", "--offline"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let found = diagnostics(&out.stderr, "FLICK-P012");
    assert!(
        found.iter().any(|d| d["message"].as_str().is_some_and(|m| {
            m.contains("the dialect `luajit` is not available on the lua/wow-retail target")
        })),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // neither Lua 5.1 nor JS stands in for it
    assert_eq!(dist(&root), before);
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
