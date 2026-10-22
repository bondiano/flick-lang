use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{Host, Project, check, discover, is_module_name, module_name, module_path, normalize};
use crate::source::SourceMap;

struct Memory(BTreeMap<PathBuf, String>);

impl Host for Memory {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        self.0
            .get(path)
            .map(|text| text.clone().into_bytes())
            .ok_or_else(|| std::io::ErrorKind::NotFound.into())
    }

    fn display(&self, path: &Path) -> String {
        path.strip_prefix("/p")
            .unwrap_or(path)
            .display()
            .to_string()
    }
}

const LIB: &str = "schema-version = 1\n[project]\nkind = \"lib\"\n";

fn memory(files: &[(&str, &str)]) -> Memory {
    Memory(
        files
            .iter()
            .map(|(p, t)| (Path::new("/p").join(p), (*t).to_owned()))
            .collect(),
    )
}

/// Checks `roots` of the project at `/p`; each diagnostic as `code path text-at-span: message`.
fn run(host: &Memory, roots: &[&str]) -> Vec<String> {
    let mut sources = SourceMap::default();
    let project: Option<Project> =
        discover(host, &mut sources, Path::new("/p"), None).expect("valid config");
    let roots: Vec<PathBuf> = roots.iter().map(|r| Path::new("/p").join(r)).collect();
    check(host, &mut sources, project.as_ref(), &roots)
        .iter()
        .map(|d| {
            let at = d.span.map_or(String::new(), |s| {
                let file = sources.get(s.file);
                format!("{} `{}`", file.path, &file.text[s.range()])
            });
            format!("{} {at}: {}", d.code, d.message)
        })
        .collect()
}

#[test]
fn diagnostics_do_not_depend_on_import_order() {
    let left = "module A.Left exposing (..)\nfind : Number\nlet find = 1\ntype T = | T\n";
    let right = "module A.Right exposing (..)\nfind : Number\nlet find = 2\ntype T = | T\n";
    let main =
        |imports: &str| format!("module A.Main exposing (x)\n{imports}\nx : T\nlet x = find\n");
    let orders = [
        "import A.Left exposing (..)\nimport A.Right exposing (..)\nimport A.Left as Q\nimport A.Right as Q exposing (T)\nimport A.Left exposing (T)",
        "import A.Right as Q exposing (T)\nimport A.Left exposing (T)\nimport A.Right exposing (..)\nimport A.Left as Q\nimport A.Left exposing (..)",
    ];
    let results: Vec<Vec<String>> = orders
        .iter()
        .map(|imports| {
            let main = main(imports);
            let host = memory(&[
                ("flick.toml", LIB),
                ("src/A/Left.fl", left),
                ("src/A/Right.fl", right),
                ("src/A/Main.fl", &main),
            ]);
            run(&host, &["src/A/Main.fl"])
        })
        .collect();
    assert_eq!(results[0], results[1]);
    assert_eq!(results[0].len(), 3, "{:#?}", results[0]);
    assert!(
        results[0]
            .iter()
            .any(|d| d.contains("`find` is ambiguous: it is exposed by `A.Left`, `A.Right`"))
    );
}

#[test]
fn named_paths_still_check_their_imports() {
    let host = memory(&[
        ("flick.toml", LIB),
        (
            "src/A/Main.fl",
            "module A.Main exposing (x)\nimport A.Util exposing (y)\nx : Number\nlet x = y\n",
        ),
        (
            "src/A/Util.fl",
            "module A.Util exposing (y, z)\ny : Number\nlet y = 1\n",
        ),
        ("src/A/Unrelated.fl", "module A.Unrelated exposing (nope)\n"),
    ]);
    assert_eq!(
        run(&host, &["src/A/Main.fl"]),
        [
            "FLICK-R011 src/A/Util.fl `z`: cannot export `z`: it is neither defined in this module nor imported"
        ]
    );
}

#[test]
fn test_modules_see_sources_but_not_the_reverse() {
    let host = memory(&[
        ("flick.toml", LIB),
        (
            "src/A/Lib.fl",
            "module A.Lib exposing (f)\nimport A.Helper\nf : Number\nlet f = 1\n",
        ),
        (
            "test/A/Helper.fl",
            "module A.Helper exposing (h)\nh : Number\nlet h = 1\n",
        ),
        (
            "test/A/LibTest.fl",
            "module A.LibTest exposing (t)\nimport A.Lib exposing (f)\nimport A.Helper exposing (h)\nt : Number\nlet t = f\n",
        ),
    ]);
    let result = run(&host, &["test/A/LibTest.fl", "src/A/Lib.fl"]);
    assert_eq!(
        result,
        ["FLICK-R003 src/A/Lib.fl `A.Helper`: unknown module `A.Helper`"]
    );
}

#[test]
fn without_a_project_imports_resolve_from_the_module_root() {
    let host = memory(&[
        (
            "lib/A/Main.fl",
            "module A.Main exposing (x)\nimport A.Util exposing (y)\nx : Number\nlet x = y\n",
        ),
        (
            "lib/A/Util.fl",
            "module A.Util exposing (y)\ny : Number\nlet y = 1\n",
        ),
        (
            "loose.fl",
            "module Loose exposing (z)\nimport A.Util\nz : Number\nlet z = 1\n",
        ),
    ]);
    assert_eq!(run(&host, &["lib/A/Main.fl"]), Vec::<String>::new());
    let loose = run(&host, &["loose.fl"]);
    assert_eq!(loose.len(), 1);
    assert!(
        loose[0].starts_with("FLICK-R003 loose.fl `A.Util`"),
        "{loose:?}"
    );
}

#[test]
fn module_names_follow_paths() {
    assert_eq!(
        module_name(Path::new("App/Users.fl")).as_deref(),
        Some("App.Users")
    );
    assert_eq!(module_name(Path::new("Main.fl")).as_deref(), Some("Main"));
    assert_eq!(module_name(Path::new("app/Users.fl")), None);
    assert_eq!(module_name(Path::new("App/Users.txt")), None);
    assert_eq!(module_name(Path::new("App/my-file.fl")), None);
    assert_eq!(module_path("App.Users"), Path::new("App/Users.fl"));
    assert!(is_module_name("App.Users") && !is_module_name("App..Users") && !is_module_name("app"));
}

#[test]
fn normalize_is_lexical() {
    assert_eq!(normalize(Path::new("/a/./b/../c")), Path::new("/a/c"));
    assert_eq!(normalize(Path::new("../x")), Path::new("../x"));
}
