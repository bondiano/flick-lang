#[test]
#[ignore = "needs Node and `pnpm install`; run `just e2e`"]
fn compiled_ui_core_imports_without_a_dom_renderer() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ui_core_import");
    let build = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("build")
        .arg(repo.join("tests/check/pass/ui/views.fl"))
        .arg("--out-dir")
        .arg(&dir)
        .args(["--emit", "js"])
        .output()
        .expect("flick runs");
    assert!(build.status.success(), "{build:?}");
    let imported = Command::new("node")
        .args([
            "--input-type=module",
            "--eval",
            r#"
import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { pathToFileURL } from "node:url";
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === "react-dom" || specifier.startsWith("react-dom/")) {
      throw new Error("a compiled Ui.mount_with consumer loaded react-dom");
    }
    return next(specifier, context);
  }
});
assert.equal(typeof document, "undefined");
assert.equal(typeof window, "undefined");
const app = await import(pathToFileURL(process.argv[1]).href);
assert.equal(typeof app.run, "function");
assert.equal(app.app.kind, "component");
"#,
        ])
        .arg(dir.join("App/Views.js"))
        .output()
        .expect("Node is installed");
    assert!(imported.status.success(), "{imported:?}");
}

/// `tsc --noEmit` over everything `--emit ts` wrote under `dir`.
fn typecheck(repo: &Path, dir: &Path) -> Option<String> {
    let config = r#"{
  "compilerOptions": {
    "strict": true,
    "noEmit": true,
    "allowImportingTsExtensions": true,
    "module": "nodenext",
    "moduleResolution": "nodenext",
    "target": "es2022",
    "skipLibCheck": true,
    "types": []
  },
  "include": ["**/*.ts"]
}
"#;
    fs::write(dir.join("tsconfig.json"), config).expect("write tsconfig");
    let tsc = Command::new(repo.join("node_modules/.bin/tsc"))
        .arg("-p")
        .arg(dir)
        .output()
        .expect("`tsc` is installed: run `pnpm install`");
    (!tsc.status.success())
        .then(|| format!("tsc --noEmit:\n{}", String::from_utf8_lossy(&tsc.stdout)))
}

/// Every module the checker accepts compiles to JS that Node can parse.
#[test]
#[ignore = "needs Node; run `just e2e`"]
fn passing_fixtures_compile_to_valid_js() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let root = repo.join("tests/check/pass");
    let mut paths = Vec::new();
    cases(&root, &mut paths);
    let out_root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fixtures");
    let report: Vec<String> = parallel(&paths, jobs(), |case| {
        let rel = case.strip_prefix(&root).expect("case under root");
        let out_dir = out_root.join(rel.with_extension(""));
        let _ = fs::remove_dir_all(&out_dir);
        let build = Command::new(env!("CARGO_BIN_EXE_flick"))
            .arg("build")
            .arg(case)
            .arg("--out-dir")
            .arg(&out_dir)
            .output()
            .expect("flick runs");
        let stderr = String::from_utf8_lossy(&build.stderr);
        // blocks compiled by later phases are reported, not miscompiled
        if !build.status.success() && !stderr.contains("FLICK-P012") {
            return vec![format!("{}: {stderr}", rel.display())];
        }
        let mut files = Vec::new();
        scripts(&out_dir, &mut files);
        files
            .iter()
            .filter_map(|file| {
                let check = Command::new("node")
                    .arg("--check")
                    .arg(file)
                    .output()
                    .expect("node");
                (!check.status.success()).then(|| {
                    format!(
                        "{}: {}",
                        file.display(),
                        String::from_utf8_lossy(&check.stderr)
                    )
                })
            })
            .collect()
    })
    .concat();
    assert!(report.is_empty(), "{}", report.join("\n"));
}

fn scripts(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scripts(&path, out);
        } else if path.extension().is_some_and(|e| e == "js") {
            out.push(path);
        }
    }
}
