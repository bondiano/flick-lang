//! `flick test --watch`: a change runs the tests again once the run before
//! it has released what it held.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use std::{fs, thread};

use super::project;

const SLOW: &str = r#"module Test.Slow exposing (tests)

sleep : Float ->{Async;} Unit
foreign sleep from "./sleep.mjs" "sleep"

tests : List Test.Test
let tests =
  [
    Test.effect_test
      "holds a resource"
      (task {
        let r =
          Resource.acquire_release
            (task { Platform.stdout_line "acquired" })
            (fun _ -> Platform.stdout_line "released")
        sleep 3000
      })
  ]
"#;

/// Collects stdout as it comes and signals when its last writer closes it.
fn collect(child: &mut Child) -> (Arc<Mutex<String>>, mpsc::Receiver<()>) {
    let out = Arc::new(Mutex::new(String::new()));
    let (finished, closed) = mpsc::channel();
    let mut stdout = child.stdout.take().expect("piped");
    let sink = out.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 256];
        loop {
            let n = stdout.read(&mut buf).expect("read stdout");
            if n == 0 {
                break;
            }
            sink.lock()
                .expect("lock")
                .push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        drop(stdout);
        let _ = finished.send(());
    });
    (out, closed)
}

/// Waits up to a minute: each run of `flick test` compiles the project
/// first, which takes over 10 s in a debug build.
fn wait_for(out: &Mutex<String>, text: &str, count: usize) -> String {
    for _ in 0..1200 {
        let now = out.lock().expect("lock").clone();
        if now.matches(text).count() >= count {
            return now;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("no {count}× {text:?} in {:?}", out.lock().expect("lock"));
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn watch_runs_again_after_the_previous_run_cleaned_up() {
    let dir = project("watch");
    fs::remove_file(dir.join("test/Test/Sample.fl")).expect("remove");
    fs::write(dir.join("test/Test/Slow.fl"), SLOW).expect("write");
    fs::write(
        dir.join("test/Test/sleep.mjs"),
        "export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));\n",
    )
    .expect("write");
    // outside the project, whose files the watcher follows
    let timings = Path::new(env!("CARGO_TARGET_TMPDIR")).join("watch-timings.txt");
    let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["test", "--watch", "--timings"])
        .current_dir(&dir)
        .stdout(Stdio::piped())
        .stderr(fs::File::create(&timings).expect("a file for stderr"))
        .spawn()
        .expect("flick runs");
    let (out, closed) = collect(&mut child);
    wait_for(&out, "acquired", 1);
    thread::sleep(Duration::from_millis(300));
    fs::write(
        dir.join("src/App/Lib.fl"),
        "module App.Lib exposing (one)\n\none : Float\nlet one = 2\n",
    )
    .expect("write");
    let text = wait_for(&out, "acquired", 2);
    // the runner of the second run stops by itself: its stdin closes with the watcher
    child.kill().expect("stop watcher");
    child.wait().expect("wait for watcher");
    closed
        .recv_timeout(Duration::from_secs(10))
        .expect("the runner exits after the watcher closes its stdin");
    assert_eq!(out.lock().expect("lock").matches("released").count(), 2);
    // each run reported its own build while the watcher ran: neither the
    // first build nor the tests it started are in the second report
    let reports = fs::read_to_string(&timings).expect("stderr");
    assert_eq!(reports.matches("Timings\n").count(), 2, "{reports}");
    assert_eq!(
        reports.matches(" ms  Compiling Flick modules\n").count(),
        2,
        "{reports}"
    );
    assert!(!reports.contains("Running a child process"), "{reports}");
    let released = text
        .find("released")
        .expect("the first run released its resource");
    let changed = text.find("a file changed").expect("the change was noticed");
    let second = text.rfind("acquired").expect("second run");
    assert!(released < changed && changed < second, "{text}");
}
