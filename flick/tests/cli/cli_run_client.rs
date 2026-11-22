//! Client entries delegate to the selected backend's `run-client` / `dev-client`
//! command (for `flick/js`, Vite via granted `node`; for backends without a
//! client runner such as `flick/go`, `FLICK-P067`).

use crate::common;

#[cfg(unix)]
#[test]
fn client_entry_builds_and_previews_with_vite() {
    use std::env;
    use std::fs;
    use std::process::Command;

    let config = concat!(
        "schema-version = 1\n",
        "[project]\nkind = \"app\"\n",
        "entries = { web = { module = \"App.Web\", runner = \"Runner.Browser\", target = \"client\" } }\n",
        "[tools]\n\"flick/js\" = [\"pnpm\", \"node\"]\n",
    );
    let dir = common::project(
        "run_client",
        &[
            ("flick.toml", config),
            (
                "node_modules/vite/package.json",
                r#"{"name":"vite","version":"8.3.5","bin":{"vite":"bin/vite.js"}}"#,
            ),
            ("node_modules/vite/bin/vite.js", "// fake vite\n"),
        ],
    );
    fs::create_dir_all(dir.join("bin")).expect("dir");
    common::write_script(
        &dir.join("bin/node"),
        concat!(
            "#!/bin/sh\n",
            "if [ \"$1\" = --version ]; then echo v24.18.0; exit 0; fi\n",
            "case \"$1:$2\" in\n",
            "  node_modules/vite/bin/vite.js:build) printf 'vite-built\\n' ;;\n",
            "  node_modules/vite/bin/vite.js:preview) printf 'vite-preview\\n' ;;\n",
            "  *) exit 9 ;;\n",
            "esac\n",
        ),
    );
    let mut paths = vec![dir.join("bin")];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    let out = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("run")
        .current_dir(&dir)
        .env("PATH", env::join_paths(&paths).expect("PATH"))
        .output()
        .expect("flick run");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "vite-built\nvite-preview\n"
    );
}

#[test]
fn client_entry_requires_node_tool_grant() {
    let config = concat!(
        "schema-version = 1\n",
        "[project]\nkind = \"app\"\n",
        "entries = { web = { module = \"App.Web\", runner = \"Runner.Browser\", target = \"client\" } }\n",
        "[tools]\n\"flick/js\" = [\"pnpm\"]\n",
    );
    let dir = common::project(
        "run_client_no_grant",
        &[
            ("flick.toml", config),
            (
                "node_modules/vite/package.json",
                r#"{"name":"vite","version":"8.3.5","bin":{"vite":"bin/vite.js"}}"#,
            ),
        ],
    );
    for verb in ["run", "dev"] {
        let out = common::flick_in(&dir, &[verb]);
        assert_eq!(out.status.code(), Some(1), "{verb}: {out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("FLICK-P051"),
            "{verb}: {out:?}"
        );
    }
}

#[test]
fn client_entry_on_go_target_reports_p067() {
    let config = concat!(
        "schema-version = 1\n",
        "[project]\nkind = \"app\"\n",
        "entries = { main = { module = \"App.Web\", target = \"client\" } }\n",
        "[build]\ntarget = \"go\"\n",
    );
    let dir = common::project("run_client_go", &[("flick.toml", config)]);
    for verb in ["run", "dev"] {
        let out = common::flick_in(&dir, &[verb]);
        assert_eq!(out.status.code(), Some(2), "{verb}: {out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("FLICK-P067"),
            "{verb}: {out:?}"
        );
    }
    // Explicit `--entry` overrides the project's client entry instead of misrouting to `run-client`.
    let explicit = common::flick_in(&dir, &["run", "--entry", "App.Other"]);
    assert!(
        !String::from_utf8_lossy(&explicit.stderr).contains("FLICK-P067"),
        "explicit --entry must not dispatch to run-client: {explicit:?}"
    );
}

#[cfg(unix)]
#[test]
fn client_entry_run_exits_with_signal_code_without_p017() {
    use std::env;
    use std::fs;
    use std::io::{BufRead, BufReader, Read};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    let config = concat!(
        "schema-version = 1\n",
        "[project]\nkind = \"app\"\n",
        "entries = { web = { module = \"App.Web\", runner = \"Runner.Browser\", target = \"client\" } }\n",
        "[tools]\n\"flick/js\" = [\"pnpm\", \"node\"]\n",
    );
    let dir = common::project(
        "run_client_signal",
        &[
            ("flick.toml", config),
            (
                "node_modules/vite/package.json",
                r#"{"name":"vite","version":"8.3.5","bin":{"vite":"bin/vite.js"}}"#,
            ),
            ("node_modules/vite/bin/vite.js", "// fake vite\n"),
        ],
    );
    fs::create_dir_all(dir.join("bin")).expect("dir");
    common::write_script(
        &dir.join("bin/node"),
        concat!(
            "#!/bin/sh\n",
            "if [ \"$1\" = --version ]; then echo v24.18.0; exit 0; fi\n",
            "case \"$1:$2\" in\n",
            "  node_modules/vite/bin/vite.js:build) printf 'vite-built\\n' ;;\n",
            "  node_modules/vite/bin/vite.js:preview)\n",
            "    trap 'exit 143' TERM\n",
            "    printf 'vite-preview-ready\\n'\n",
            "    while true; do sleep 1; done\n",
            "    ;;\n",
            "  *) exit 9 ;;\n",
            "esac\n",
        ),
    );
    let mut paths = vec![dir.join("bin")];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("run")
        .current_dir(&dir)
        .env("PATH", env::join_paths(&paths).expect("PATH"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn flick run");
    let stdout = child.stdout.take().expect("stdout");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut saw_built = false;
    loop {
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(line) if line == "vite-built" => saw_built = true,
            Ok(line) if line == "vite-preview-ready" => break,
            Ok(_) => {}
            Err(err) => panic!("timed out waiting for vite preview: {err}"),
        }
    }
    assert!(saw_built, "expected vite build before preview");
    Command::new("kill")
        .args(["-s", "TERM", &child.id().to_string()])
        .status()
        .expect("kill");
    let status = child.wait().expect("wait");
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("stderr")
        .read_to_string(&mut stderr)
        .expect("read stderr");
    assert_eq!(status.code(), Some(143), "stderr:\n{stderr}");
    assert!(
        !stderr.contains("FLICK-P017"),
        "normal SIGTERM shutdown must not emit FLICK-P017:\n{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn client_entry_dev_starts_vite_and_exits_on_signal() {
    use std::env;
    use std::fs;
    use std::io::{BufRead, BufReader, Read};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    let config = concat!(
        "schema-version = 1\n",
        "[project]\nkind = \"app\"\n",
        "entries = { web = { module = \"App.Web\", runner = \"Runner.Browser\", target = \"client\" } }\n",
        "[tools]\n\"flick/js\" = [\"pnpm\", \"node\"]\n",
    );
    let dir = common::project(
        "dev_client_vite",
        &[
            ("flick.toml", config),
            (
                "node_modules/vite/package.json",
                r#"{"name":"vite","version":"8.3.5","bin":{"vite":"bin/vite.js"}}"#,
            ),
            ("node_modules/vite/bin/vite.js", "// fake vite\n"),
        ],
    );
    fs::create_dir_all(dir.join("bin")).expect("dir");
    common::write_script(
        &dir.join("bin/node"),
        concat!(
            "#!/bin/sh\n",
            "if [ \"$1\" = --version ]; then echo v24.18.0; exit 0; fi\n",
            "if [ \"$1\" = \"node_modules/vite/bin/vite.js\" ]; then\n",
            "  shift\n",
            "  trap 'exit 143' TERM\n",
            "  printf 'vite-dev-ready %s\\n' \"$*\"\n",
            "  while true; do sleep 1; done\n",
            "fi\n",
            "exit 9\n",
        ),
    );
    let mut paths = vec![dir.join("bin")];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["dev", "--", "--port", "5173"])
        .current_dir(&dir)
        .env("PATH", env::join_paths(&paths).expect("PATH"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn flick dev");
    let stdout = child.stdout.take().expect("stdout");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let line = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("vite dev ready line");
    assert_eq!(line, "vite-dev-ready --port 5173");
    Command::new("kill")
        .args(["-s", "TERM", &child.id().to_string()])
        .status()
        .expect("kill");
    let status = child.wait().expect("wait");
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("stderr")
        .read_to_string(&mut stderr)
        .expect("read stderr");
    assert_eq!(status.code(), Some(143), "stderr:\n{stderr}");
    assert!(
        !stderr.contains("FLICK-P017"),
        "normal SIGTERM shutdown of dev-client must not emit FLICK-P017:\n{stderr}"
    );
}
