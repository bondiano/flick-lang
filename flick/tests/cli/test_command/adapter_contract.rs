use std::fs;

use super::{common::flick_in, project};

#[test]
#[ignore = "needs Node and pnpm install"]
fn cached_tests_reject_a_new_invalid_adapter_contract() {
    let dir = project("adapter_contract_cache");
    let config = dir.join("flick.toml");
    let text = fs::read_to_string(&config).expect("config");
    fs::write(config, format!("{text}\n[build]\neffect = \"effect\"\n")).expect("effect backend");
    fs::write(
        dir.join("test/Test/Sample.fl"),
        "module Test.Sample exposing (tests)\n\ntests : List Test.Test\nlet tests = [Test.test \"adds\" (fun () -> Test.equal 2 (1 + 1))]\n",
    )
    .expect("suite");
    let run = || flick_in(&dir, &["test", "--filter", "adds"]);
    let first = run();
    assert!(first.status.success(), "{first:?}");
    let reused = run();
    assert!(reused.status.success(), "{reused:?}");
    assert!(String::from_utf8_lossy(&reused.stderr).contains("Reusing test build"));
    let manifest = dir.join("node_modules/@flick-lang/effect-ts/package.json");
    fs::create_dir_all(manifest.parent().expect("adapter directory")).expect("directory");
    fs::write(&manifest, "").expect("empty manifest");
    let empty = run();
    assert!(
        String::from_utf8_lossy(&empty.stderr).contains("FLICK-P015"),
        "{empty:?}"
    );
    assert_eq!(empty.status.code(), Some(1));
    fs::remove_file(manifest).expect("remove manifest");
    let unreadable = run();
    assert!(
        String::from_utf8_lossy(&unreadable.stderr).contains("FLICK-P015"),
        "{unreadable:?}"
    );
    assert_eq!(unreadable.status.code(), Some(1));
}
