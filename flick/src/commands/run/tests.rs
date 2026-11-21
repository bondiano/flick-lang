use std::env;
use std::sync::{Barrier, mpsc};
use std::thread;

use crate::commands::clean::clean_generations;

use super::*;

#[test]
fn locks_keep_live_or_unknown_processes() {
    assert!(process_alive(&format!("\n{}\n", process::id())));
    #[cfg(unix)]
    assert!(!process_alive("999999999"));
    for pid in [
        "",
        " \n",
        "0",
        "-1",
        "-999999999",
        "2147483648",
        "1 2",
        "../1",
        "/proc/1",
        "1; exit 1",
    ] {
        assert!(process_alive(pid), "{pid:?}");
    }
}

#[test]
fn concurrent_reservations_survive_clean_before_launch_and_other_cleanup() {
    let root = env::temp_dir().join(format!("flick-reservations-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    let gate = Barrier::new(2);
    thread::scope(|scope| {
        let (reserved, paths) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let root = &root;
        let gate = &gate;
        let first = scope.spawn(move || {
            gate.wait();
            let generation = Generation::reserve(root).expect("first reservation");
            fs::write(generation.path().join("Main.js"), "first").expect("first artifact");
            reserved
                .send(generation.path().to_path_buf())
                .expect("path");
            released.recv().expect("release first");
        });
        gate.wait();
        let second = Generation::reserve(root).expect("second reservation");
        fs::write(second.path().join("Main.js"), "second").expect("second artifact");
        let first_path = paths.recv().expect("reserved path");
        assert_ne!(first_path, second.path());
        assert_eq!(clean_generations(root), 2, "construction owns both paths");
        assert!(first_path.join("Main.js").exists());
        assert!(second.path.join("Main.js").exists());
        release.send(()).expect("finish first");
        first.join().expect("first cleanup");
        assert!(!first_path.exists());
        assert_eq!(clean_generations(root), 1);
        assert_eq!(
            fs::read_to_string(second.path().join("Main.js")).expect("surviving artifact"),
            "second"
        );
        let second_path = second.path().to_path_buf();
        drop(second);
        assert!(!second_path.exists());
    });
    fs::remove_dir_all(root).expect("fixture cleanup");
}
