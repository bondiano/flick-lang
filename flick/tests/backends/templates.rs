//! Third-party template packages via `--from path`, `--from url --sha256`,
//! and `--from git --tag`, using the `flick_test_backend.wasm` fixture.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;

use flick_backend::sha256;

use super::common::project;

fn fixture_component() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    fs::read(&path).expect("build the fixture with `just wasm-backends`")
}

fn backend_json(bytes: &[u8]) -> String {
    serde_json::json!({
        "identity": { "id": "test/literal", "version": "1.0.0" },
        "sha256": sha256(bytes),
    })
    .to_string()
}

fn flick_with_store(dir: &Path, store: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("FLICK_STORE", store)
        .env("NO_COLOR", "1")
        .output()
        .expect("flick runs")
}

fn serve_tarball(body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("port");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        for mut stream in listener.incoming().map_while(Result::ok) {
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut request = String::new();
            let _ = reader.read_line(&mut request);
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                line.clear();
            }
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
            if request.starts_with("GET /literal-tpl.tgz ") {
                break;
            }
        }
    });
    format!("http://127.0.0.1:{port}/literal-tpl.tgz")
}

fn pack_tarball(src_root: &Path, dir_name: &str) -> (Vec<u8>, String) {
    let tgz = src_root.join("literal-tpl.tgz");
    let status = Command::new("tar")
        .args(["czf", "literal-tpl.tgz", dir_name])
        .env("COPYFILE_DISABLE", "1")
        .current_dir(src_root)
        .status()
        .expect("tar runs");
    assert!(status.success(), "tar failed");
    let bytes = fs::read(&tgz).expect("tarball");
    let sha = sha256(&bytes);
    (bytes, sha)
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

fn make_pkg_dir(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let dir = root.join(rel);
    fs::create_dir_all(&dir).expect("pkg dir");
    fs::write(dir.join("backend.json"), backend_json(bytes)).expect("backend.json");
    fs::write(dir.join("component.wasm"), bytes).expect("component.wasm");
    dir
}

#[test]
#[ignore = "requires just wasm-backends"]
fn third_party_template_from_path_pins_lock_and_survives_install() {
    let bytes = fixture_component();
    let wasm_sha = sha256(&bytes);
    let root = project("third_party_templates_path", &[]);
    let store = root.join("store");
    let pkg = make_pkg_dir(&root, "pkg", &bytes);

    let out = flick_with_store(
        &root,
        &store,
        &[
            "new",
            "from-path",
            "--from",
            pkg.to_str().expect("utf8"),
            "--template",
            "starter",
        ],
    );
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Next: literal starter `from-path` ready"),
        "{stdout}"
    );
    // Untrusted `--from` backend packages must never populate `<store>/descriptors/`.
    let descriptors_dir = store.join("descriptors");
    let cached_descriptors = fs::read_dir(&descriptors_dir).map_or(0, Iterator::count);
    assert_eq!(
        cached_descriptors, 0,
        "third-party `--from` package must not write to descriptor cache: {descriptors_dir:?}"
    );
    let app = root.join("from-path");
    assert!(app.join("flick.toml").is_file());
    let readme = fs::read_to_string(app.join("README.md")).expect("readme");
    assert!(readme.contains("init: false"), "{readme}");
    let lock = fs::read_to_string(app.join("flick.lock")).expect("lock");
    assert!(
        lock.contains("[[template]]")
            && lock.contains("template = \"starter\"")
            && lock.contains("package = \"test/literal\"")
            && lock.contains("version = \"1.0.0\"")
            && lock.contains("source = \"path+../pkg\"")
            && lock.contains(&format!("component = \"sha256:{wasm_sha}\"")),
        "{lock}"
    );
    let checked = flick_with_store(&app, &store, &["check"]);
    assert!(checked.status.success(), "{checked:?}");
    let installed = flick_with_store(&app, &store, &["install", "--locked", "--offline"]);
    assert!(installed.status.success(), "{installed:?}");
    assert_eq!(
        fs::read_to_string(app.join("flick.lock")).expect("lock kept"),
        lock
    );
}

#[test]
#[ignore = "requires just wasm-backends"]
fn third_party_template_from_url_verifies_sha256_and_works_offline() {
    let bytes = fixture_component();
    let wasm_sha = sha256(&bytes);
    let root = project("third_party_templates_url", &[]);
    let store = root.join("store");
    let tar_root = root.join("tar-src");
    make_pkg_dir(&tar_root, "package", &bytes);
    let (tgz_bytes, tgz_sha) = pack_tarball(&tar_root, "package");
    let url = serve_tarball(tgz_bytes.clone());

    // Wrong sha256 fails with FLICK-P034 and creates no directory.
    let bad_url = serve_tarball(tgz_bytes);
    let bad = flick_with_store(
        &root,
        &store,
        &[
            "new",
            "bad-sha-app",
            "--from",
            &bad_url,
            "--sha256",
            &"0".repeat(64),
            "--template",
            "starter",
        ],
    );
    assert!(!bad.status.success(), "{bad:?}");
    assert!(
        String::from_utf8_lossy(&bad.stderr).contains("FLICK-P034"),
        "{bad:?}"
    );
    assert!(!root.join("bad-sha-app").exists());

    // Matching sha256 succeeds and pins in flick.lock.
    let out = flick_with_store(
        &root,
        &store,
        &[
            "new",
            "from-url",
            "--from",
            &url,
            "--sha256",
            &tgz_sha,
            "--template",
            "starter",
        ],
    );
    assert!(out.status.success(), "{out:?}");
    let url_lock = fs::read_to_string(root.join("from-url/flick.lock")).expect("url lock");
    assert!(
        url_lock.contains("[[template]]")
            && url_lock.contains(&format!("source = \"url+{url}\""))
            && url_lock.contains(&format!("archive = \"sha256:{tgz_sha}\""))
            && url_lock.contains(&format!("component = \"sha256:{wasm_sha}\"")),
        "{url_lock}"
    );

    // Server stopped after one request: `--offline` reuses the cached verified tree via url-pins.
    let offline = flick_with_store(
        &root,
        &store,
        &[
            "new",
            "from-url-offline",
            "--offline",
            "--from",
            &url,
            "--sha256",
            &tgz_sha,
            "--template",
            "starter",
        ],
    );
    assert!(offline.status.success(), "{offline:?}");
    assert!(root.join("from-url-offline/flick.toml").is_file());
}

#[test]
#[ignore = "requires just wasm-backends"]
fn third_party_template_from_git_tag_and_subdir_pins_commit() {
    let bytes = fixture_component();
    let root = project("third_party_templates_git", &[]);
    let store = root.join("store");
    let repo = root.join("git-repo");
    make_pkg_dir(&repo, "tpl", &bytes);
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "init template"]);
    git(&repo, &["tag", "v1.0.0"]);
    let git_url = format!("file://{}", repo.display());
    let out = flick_with_store(
        &root,
        &store,
        &[
            "new",
            "from-git",
            "--from",
            &git_url,
            "--tag",
            "v1.0.0",
            "--subdir",
            "tpl",
            "--template",
            "starter",
        ],
    );
    assert!(out.status.success(), "{out:?}");
    let git_lock = fs::read_to_string(root.join("from-git/flick.lock")).expect("git lock");
    assert!(
        git_lock.contains("[[template]]")
            && git_lock.contains(&format!("source = \"git+{git_url}?tag=v1.0.0&subdir=tpl#")),
        "{git_lock}"
    );
    fs::remove_dir_all(&repo).expect("remove repo");
    let git_offline = flick_with_store(
        &root,
        &store,
        &[
            "new",
            "from-git-offline",
            "--offline",
            "--from",
            &git_url,
            "--tag",
            "v1.0.0",
            "--subdir",
            "tpl",
            "--template",
            "starter",
        ],
    );
    assert!(git_offline.status.success(), "{git_offline:?}");
}

#[test]
#[ignore = "requires just wasm-backends"]
fn third_party_template_refusals_and_init_lock_merge() {
    let bytes = fixture_component();
    let root = project("third_party_template_guards", &[]);
    let store = root.join("store");
    let pkg = make_pkg_dir(&root, "pkg", &bytes);
    let pkg_str = pkg.to_str().expect("utf8");

    // Multi-template contributor with no `app` requires `--template`, listing all available (P060).
    let omitted = flick_with_store(&root, &store, &["new", "no-tpl", "--from", pkg_str]);
    assert_eq!(omitted.status.code(), Some(2), "{omitted:?}");
    let stderr = String::from_utf8_lossy(&omitted.stderr);
    assert!(
        stderr.contains("FLICK-P060") && stderr.contains("starter"),
        "{stderr}"
    );
    assert!(!root.join("no-tpl").exists());

    // Disallowed template plans (escaping paths and host tool steps) fail with FLICK-P055 and create nothing.
    for bad_tpl in ["escape", "stepped"] {
        let out = flick_with_store(
            &root,
            &store,
            &["new", bad_tpl, "--from", pkg_str, "--template", bad_tpl],
        );
        assert_eq!(out.status.code(), Some(1), "{bad_tpl}: {out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("FLICK-P055"),
            "{bad_tpl}: {out:?}"
        );
        assert!(!root.join(bad_tpl).exists(), "{bad_tpl}: no dir created");
    }
    assert!(!root.join("outside.txt").exists());

    // `init --from` in a non-empty directory preserves unrelated files, AGENTS.md, and existing flick.lock entries.
    let existing_lock = format!(
        "version = 1\n\n[[package]]\nname = \"geo\"\nversion = \"1.0.0\"\nsource = \"url+https://example.invalid/geo.tgz\"\nchecksum = \"sha256:{}\"\n",
        "ab".repeat(32)
    );
    let init_dir = project(
        "third_party_template_guards/init_dir",
        &[
            ("NOTES.md", "keep notes\n"),
            ("AGENTS.md", "keep agents\n"),
            ("flick.lock", &existing_lock),
        ],
    );
    let out = flick_with_store(
        &init_dir,
        &store,
        &["init", "--from", pkg_str, "--template", "starter"],
    );
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        fs::read_to_string(init_dir.join("NOTES.md")).expect("notes"),
        "keep notes\n"
    );
    assert_eq!(
        fs::read_to_string(init_dir.join("AGENTS.md")).expect("agents"),
        "keep agents\n"
    );
    let readme = fs::read_to_string(init_dir.join("README.md")).expect("readme");
    assert!(readme.contains("init: true"), "{readme}");
    let merged_lock = fs::read_to_string(init_dir.join("flick.lock")).expect("merged lock");
    assert!(
        merged_lock.contains("[[package]]")
            && merged_lock.contains("name = \"geo\"")
            && merged_lock.contains("[[template]]")
            && merged_lock.contains("template = \"starter\""),
        "{merged_lock}"
    );
}

#[test]
#[ignore = "requires just wasm-backends"]
fn third_party_template_malformed_lock_renders_without_panicking() {
    let bytes = fixture_component();
    let root = project("third_party_template_bad_lock", &[]);
    let store = root.join("store");
    let pkg = make_pkg_dir(&root, "pkg", &bytes);
    let pkg_str = pkg.to_str().expect("utf8");

    let bad_lock_human = flick_with_store(
        &root,
        &store,
        &[
            "new",
            "bad-lock-human",
            "--from",
            pkg_str,
            "--template",
            "bad-lock",
        ],
    );
    assert!(!bad_lock_human.status.success(), "{bad_lock_human:?}");
    let bad_lock_stderr = String::from_utf8_lossy(&bad_lock_human.stderr);
    assert!(
        !bad_lock_stderr.contains("panicked") && bad_lock_stderr.contains("FLICK-"),
        "{bad_lock_stderr}"
    );
    assert!(!root.join("bad-lock-human").exists());

    let bad_lock_json = flick_with_store(
        &root,
        &store,
        &[
            "--diagnostics",
            "json",
            "new",
            "bad-lock-json",
            "--from",
            pkg_str,
            "--template",
            "bad-lock",
        ],
    );
    assert!(!bad_lock_json.status.success(), "{bad_lock_json:?}");
    let json_stderr = String::from_utf8_lossy(&bad_lock_json.stderr);
    assert!(!json_stderr.contains("panicked"), "{json_stderr}");
    let events: Vec<serde_json::Value> = json_stderr
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}: {line}")))
        .collect();
    assert!(
        events.iter().any(|e| {
            e["code"]
                .as_str()
                .is_some_and(|code| code.starts_with("FLICK-"))
        }),
        "{events:?}"
    );
    assert!(!root.join("bad-lock-json").exists());
}

#[cfg(unix)]
#[test]
#[ignore = "requires just wasm-backends"]
fn third_party_git_subdir_symlink_escape_is_refused_with_p055() {
    use std::os::unix::fs::symlink;

    let bytes = fixture_component();
    let root = project("third_party_git_subdir_escape", &[]);
    let store = root.join("store");
    let outside = make_pkg_dir(&root, "outside-pkg", &bytes);

    let git_url = "https://example.invalid/escape-repo.git";
    let key = format!("git+{git_url}?tag=v1.0.0&subdir=escaped");
    let key_hash = sha256(key.as_bytes());
    let tree_hash = "ab".repeat(32);

    let tree_dir = store.join(&tree_hash);
    fs::create_dir_all(&tree_dir).expect("tree dir");
    symlink(&outside, tree_dir.join("escaped")).expect("symlink subdir");
    let pin_dir = store.join("git-pins");
    fs::create_dir_all(&pin_dir).expect("git-pins");
    fs::write(
        pin_dir.join(&key_hash),
        format!("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef\n{tree_hash}\n"),
    )
    .expect("pin file");

    let out = flick_with_store(
        &root,
        &store,
        &[
            "new",
            "escaped-app",
            "--offline",
            "--from",
            git_url,
            "--tag",
            "v1.0.0",
            "--subdir",
            "escaped",
            "--template",
            "starter",
        ],
    );
    assert!(!out.status.success(), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FLICK-P055"), "{stderr}");
    assert!(!root.join("escaped-app").exists());
}
