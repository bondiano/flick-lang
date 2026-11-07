//! More than one implicit package: a second one, of a directory and a list
//! entry, is checked after the first in its own order, every program sees it,
//! and a build writes its modules with the built-in ones. The binary holds one
//! set of implicit packages, so this file is its own test program.

#![cfg(feature = "native-project")]

use std::io;
use std::path::{Path, PathBuf};
use std::{env, fs, process};

use flick_core::build::{self, Extras, Settings};
use flick_core::diagnostics::Severity;
use flick_core::project::config::Backend;
use flick_core::project::{self, Host};
use flick_core::source::{FileId, SourceMap};
use flick_core::types;

/// The package `extra`: the order of `modules.txt` is not the alphabetical
/// one, a table binds a foreign on go, and its manifest names an npm package.
fn extra_package() -> PathBuf {
    let dir = env::temp_dir().join(format!("flick-implicit-extra-{}", process::id()));
    let _ = fs::remove_dir_all(&dir);
    let files = [
        (
            "flick.toml",
            "schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[js.dependencies]\nleft-pad = \"^1.3.0\"\n",
        ),
        (
            "src/modules.txt",
            "# in dependency order\nExtra.Zero\nExtra.Base\n",
        ),
        (
            "src/Extra/Zero.fl",
            "module Extra.Zero exposing (zero)\n\nzero : Int\nlet zero = 0\n",
        ),
        (
            "src/Extra/Base.fl",
            "module Extra.Base exposing (one)\n\nimport Extra.Zero\n\none : Int\nlet one = Extra.Zero.zero + 1\n",
        ),
        ("src/go/extra.txt", "[extra]\nnow Now\n"),
    ];
    for (file, text) in files {
        let path = dir.join(file);
        fs::create_dir_all(path.parent().expect("a parent")).expect("dir");
        fs::write(path, text).expect("write");
    }
    dir
}

struct Program(&'static str);

impl Host for Program {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if path == Path::new("/Main.fl") {
            Ok(self.0.as_bytes().to_vec())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

fn source_path(_: FileId) -> String {
    "file:///Main.fl".to_owned()
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "the settings of a build take this signature"
)]
fn source_dir(_: &str) -> Option<PathBuf> {
    Some(PathBuf::from("/"))
}

#[test]
fn a_second_implicit_package_is_one_more_entry_and_directory() {
    let std = env::var_os("FLICK_STD_PATH").expect("cargo names the std of this checkout");
    let extra = extra_package();
    assert!(types::set_implicit_packages(vec![
        types::read_package("std", Path::new(&std)).expect("std"),
        types::read_package("extra", &extra).expect("extra"),
    ]));
    fs::remove_dir_all(&extra).expect("fixture cleanup");

    // checked after what it depends on, in the order of its own `modules.txt`
    let names: Vec<&str> = types::builtin_sources().iter().map(|(n, _)| *n).collect();
    let at = |name: &str| names.iter().position(|n| *n == name).expect(name);
    assert_eq!(names.len(), at("Extra.Base") + 1);
    assert_eq!(at("Extra.Zero") + 1, at("Extra.Base"));
    assert!(at("Duration") < at("Extra.Zero"));
    let checked: Vec<&str> = types::builtins()
        .iter()
        .map(|i| i.module.as_str())
        .collect();
    assert_eq!(checked, names);

    assert_eq!(types::implicit_package_of("Extra.Base"), Some("extra"));
    assert_eq!(types::implicit_package_of("Duration"), Some("std"));
    assert_eq!(types::implicit_package_of("Prelude"), None);
    assert!(types::implicit_go_bindings().any(|(file, _)| *file == "extra.txt"));
    assert!(types::implicit_go_bindings().any(|(file, _)| *file == "globals.txt"));
    assert_eq!(
        types::implicit_js_dependencies().collect::<Vec<_>>(),
        [("extra", "left-pad", "^1.3.0")]
    );

    // a program qualifies its modules as it does any built-in one
    let main = "module Main exposing (answer)\n\nanswer : Int\nlet answer = Extra.Base.one\n";
    let mut sources = SourceMap::default();
    let (diagnostics, compilation) = project::compile(
        &Program(main),
        &mut sources,
        None,
        &[PathBuf::from("/Main.fl")],
    );
    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "{diagnostics:?}"
    );
    let settings = Settings {
        package: None,
        entries: &[],
        typescript: false,
        release: false,
        extras: Extras::default(),
        source_path: &source_path,
        source_dir: &source_dir,
        out_dir: Path::new("/dist"),
        root: None,
        backend: Backend::Effect,
        ui: None,
        runners: &[],
        core_modules: &[],
        core_only: false,
    };
    let (artifacts, diagnostics) = build::emit(&compilation, &sources, &settings);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    for file in ["_flick/Extra/Base.js", "_flick/Extra/Zero.js", "Main.js"] {
        assert!(artifacts.iter().any(|a| a.path == file), "{file}");
    }
}
