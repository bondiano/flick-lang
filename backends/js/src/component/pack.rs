//! Optional `pack` capability of the JavaScript backend: stages a production
//! `.flick/pack/package.json` for built JS apps and runs pinned `pnpm pack`
//! into `.flick/pack/out/`.

use std::path::Path;

use flick_backend::command::{
    Descriptor, Diagnostic, File, Invocation, Plan, Step, Style, ToolStep, ValueKind,
};
use serde_json::Value;

use super::dependencies::{flag, input, parameter, pnpm, values};
use crate::codegen::BUILTIN_DIR;

pub(super) fn descriptor() -> Descriptor {
    Descriptor {
        name: "pack".into(),
        summary: "Stages `package.json` for a built app and packs it with pnpm".into(),
        positionals: vec![],
        options: vec![
            parameter(
                "kind",
                ValueKind::Text,
                false,
                false,
                "Project kind (`app` or `lib`)",
            ),
            parameter(
                "has-package",
                ValueKind::Flag,
                false,
                false,
                "Whether `flick.toml` declares `[package]`",
            ),
            parameter(
                "entry",
                ValueKind::Text,
                true,
                false,
                "Declared app entry name",
            ),
            parameter(
                "runner",
                ValueKind::Text,
                true,
                false,
                "Entry runner mapping `<entry>=<runner-file>`",
            ),
            parameter(
                "file",
                ValueKind::Text,
                true,
                false,
                "Relative file path present in the staged pack directory",
            ),
            parameter(
                "option",
                ValueKind::Text,
                true,
                false,
                "Resolved build option `<key>=<value>`",
            ),
        ],
        inputs: vec!["package.json".into()],
        reads: vec![],
        writes: vec![".flick/pack".into()],
        style: Style::Plan,
    }
}

/// Stages `.flick/pack/package.json` for a built JS app and runs `pnpm pack`
/// into `.flick/pack/out/`. Libraries ship only the core Flick source tarball.
pub(super) fn run(invocation: &Invocation) -> Result<Plan, Vec<Diagnostic>> {
    if values(invocation, "kind").first().map(String::as_str) == Some("lib") {
        return Ok(Plan::empty());
    }
    let Some(raw) = input(invocation, "package.json") else {
        if flag(invocation, "has-package") {
            return Ok(Plan::empty());
        }
        return Err(vec![Diagnostic::error(
            "P020",
            "cannot read `package.json`: file not found".into(),
        )]);
    };
    let options = values(invocation, "option");
    if options.iter().any(|o| o == "typescript=true") {
        return Err(vec![Diagnostic::error(
            "P020",
            "npm packages ship JavaScript, not TypeScript: set `[target.js] typescript = false` (and `declarations = true` for `.d.ts` files)".into(),
        )]);
    }
    let dts = options.iter().any(|o| o == "declarations=true");
    let staged = values(invocation, "file");
    let incomplete = incomplete_js_companions(staged, dts);
    if !incomplete.is_empty() {
        return Err(incomplete);
    }
    let text = String::from_utf8_lossy(&raw.bytes);
    let mut manifest: Value = serde_json::from_str(&text).map_err(|err| {
        vec![Diagnostic::error(
            "P020",
            format!("`package.json` is not JSON: {err}"),
        )]
    })?;
    let Some(object) = manifest.as_object_mut() else {
        return Err(vec![Diagnostic::error(
            "P020",
            "`package.json` is not an object".into(),
        )]);
    };
    object.insert("type".into(), "module".into());
    object.insert("files".into(), serde_json::json!(["dist"]));
    object
        .entry("exports")
        .or_insert(serde_json::json!({ "./*": "./dist/*" }));
    let runners = resolve_runners(invocation, staged);
    if !runners.is_empty() {
        let package = object
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let unscoped = package.rsplit('/').next().unwrap_or(package).to_owned();
        let bin: serde_json::Map<String, Value> = runners
            .iter()
            .map(|(name, file)| {
                let command = if runners.len() == 1 && name == "main" && !unscoped.is_empty() {
                    unscoped.clone()
                } else {
                    name.clone()
                };
                (command, format!("./dist/{file}").into())
            })
            .collect();
        object.entry("bin").or_insert(Value::Object(bin));
    }
    object.remove("devDependencies");
    object.remove("scripts");
    if let Some(missing) = missing_exports(&manifest, staged) {
        return Err(vec![Diagnostic::error("P020", missing)]);
    }
    let exact = manifest
        .get("packageManager")
        .and_then(Value::as_str)
        .is_some_and(|spec| spec.starts_with("pnpm@"));
    let pnpm = pnpm(invocation, Some(&manifest), exact)?;
    let mut plan = Plan::empty();
    plan.files.push(File {
        path: ".flick/pack/package.json".into(),
        bytes: format!("{manifest:#}\n").into_bytes(),
        executable: false,
    });
    plan.steps.push(Step::Tool(ToolStep {
        id: "pack".into(),
        tool: pnpm,
        arguments: vec![
            "--dir".into(),
            ".flick/pack".into(),
            "pack".into(),
            "--pack-destination".into(),
            "out".into(),
        ],
        environment: vec![],
        inputs: vec![],
        reads: vec![],
        outputs: vec![".flick/pack".into()],
        in_place: true,
    }));
    Ok(plan)
}

/// Checks that every primary JS module under `dist/` has its `.iface` (and
/// `.d.ts` when `declarations = true`).
fn incomplete_js_companions(staged: &[String], dts: bool) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for path in staged {
        let Some(rel) = path.strip_prefix("dist/") else {
            continue;
        };
        let is_js = Path::new(rel)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("js"));
        let is_builtin = rel
            .strip_prefix(BUILTIN_DIR)
            .is_some_and(|rest| rest.starts_with('/'));
        if !is_js || rel.ends_with(".promise.js") || !rel.contains('/') || is_builtin {
            continue;
        }
        let stem = &rel[..rel.len() - 3];
        let mut wanted = vec!["iface"];
        if dts {
            wanted.push("d.ts");
        }
        for ext in wanted {
            let companion_rel = format!("{stem}.{ext}");
            let companion_staged = format!("dist/{companion_rel}");
            if !staged.iter().any(|f| f == &companion_staged) {
                diagnostics.push(Diagnostic::error(
                    "P020",
                    format!("`{rel}` has no `{companion_rel}`"),
                ));
            }
        }
    }
    diagnostics
}

/// Resolves `<entry>=<runner-file>` pairs, falling back to `flick-<entry>.js`
/// when `node` was not on PATH during build so launch paths were omitted.
fn resolve_runners(invocation: &Invocation, staged: &[String]) -> Vec<(String, String)> {
    let mut runners: Vec<(String, String)> = values(invocation, "runner")
        .iter()
        .filter_map(|entry| {
            let (name, file) = entry.split_once('=')?;
            Some((name.to_owned(), file.to_owned()))
        })
        .collect();
    for entry in values(invocation, "entry") {
        if runners.iter().any(|(name, _)| name == entry) {
            continue;
        }
        let fallback = format!("flick-{entry}.js");
        let staged_path = format!("dist/{fallback}");
        if staged.iter().any(|f| f == &staged_path) {
            runners.push((entry.clone(), fallback));
        }
    }
    runners
}

/// The first path `exports` names that the staged package lacks.
fn missing_exports(manifest: &Value, staged: &[String]) -> Option<String> {
    fn targets(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::String(s) => out.push(s.clone()),
            Value::Object(map) => map.values().for_each(|v| targets(v, out)),
            Value::Array(items) => items.iter().for_each(|v| targets(v, out)),
            _ => {}
        }
    }
    let mut paths = Vec::new();
    targets(manifest.get("exports")?, &mut paths);
    paths
        .into_iter()
        .find(|p| {
            if p.contains('*') {
                return false;
            }
            let norm = p.strip_prefix("./").unwrap_or(p);
            !staged.iter().any(|f| f == norm)
        })
        .map(|p| format!("`exports` names `{p}`, which the package does not have"))
}

#[cfg(test)]
mod tests {
    use flick_backend::command::{Argument, Tool};

    use super::*;
    use crate::component::dependencies::PNPM;

    fn tool(version: &str, sha256: &str) -> Tool {
        Tool {
            id: "pnpm".into(),
            version: version.into(),
            sha256: sha256.into(),
        }
    }

    fn arg(name: &str, values: &[&str]) -> Argument {
        Argument {
            name: name.into(),
            values: values.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    fn pack_invocation(
        manifest: Option<&str>,
        arguments: Vec<Argument>,
        tools: Vec<Tool>,
    ) -> Invocation {
        Invocation {
            name: "pack".into(),
            arguments,
            sources: "src".into(),
            inputs: manifest
                .map(|m| {
                    vec![File {
                        path: "package.json".into(),
                        bytes: m.as_bytes().to_vec(),
                        executable: false,
                    }]
                })
                .unwrap_or_default(),
            tools,
            dependencies: vec![],
        }
    }

    #[test]
    fn pack_returns_empty_plan_for_libraries_without_manifest_or_pnpm() {
        let inv = pack_invocation(None, vec![arg("kind", &["lib"])], vec![]);
        let plan = run(&inv).expect("lib pack succeeds");
        assert!(plan.files.is_empty());
        assert!(plan.steps.is_empty());
    }

    #[test]
    fn pack_returns_empty_plan_for_app_with_package_and_no_package_json() {
        let inv = pack_invocation(
            None,
            vec![arg("kind", &["app"]), arg("has-package", &["true"])],
            vec![],
        );
        let plan = run(&inv).expect("source-only app pack succeeds");
        assert!(plan.files.is_empty());
        assert!(plan.steps.is_empty());

        let missing = pack_invocation(None, vec![arg("kind", &["app"])], vec![]);
        let errs = run(&missing).expect_err("app without [package] needs package.json");
        assert_eq!(errs[0].code, "P020");
    }

    #[test]
    fn pack_rejects_typescript_true_and_missing_companions_and_exports() {
        let manifest = r#"{"name":"demo","version":"1.0.0"}"#;
        let ts_inv = pack_invocation(
            Some(manifest),
            vec![arg("kind", &["app"]), arg("option", &["typescript=true"])],
            vec![tool(PNPM, "abc")],
        );
        let errs = run(&ts_inv).expect_err("typescript=true rejected");
        assert_eq!(errs[0].code, "P020");
        assert!(errs[0].message.contains("typescript = false"));

        // Missing .iface for primary module
        let no_iface = pack_invocation(
            Some(manifest),
            vec![
                arg("kind", &["app"]),
                arg(
                    "file",
                    &[
                        "dist/Lib/Greet.js",
                        "dist/Lib/Greet.promise.js",
                        "dist/_flick/Host.js",
                        "dist/flick-main.js",
                    ],
                ),
            ],
            vec![tool(PNPM, "abc")],
        );
        let errs = run(&no_iface).expect_err("missing .iface rejected");
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].code, "P020");
        assert!(errs[0].message.contains("Lib/Greet.iface"));

        // Missing .d.ts when declarations=true
        let no_dts = pack_invocation(
            Some(manifest),
            vec![
                arg("kind", &["app"]),
                arg("option", &["declarations=true"]),
                arg("file", &["dist/Lib/Greet.js", "dist/Lib/Greet.iface"]),
            ],
            vec![tool(PNPM, "abc")],
        );
        let errs = run(&no_dts).expect_err("missing .d.ts rejected");
        assert_eq!(errs[0].code, "P020");
        assert!(errs[0].message.contains("Lib/Greet.d.ts"));

        // Missing explicit exports target
        let bad_exports = r#"{"name":"demo","version":"1.0.0","exports":{".":"./dist/Nope.js"}}"#;
        let inv = pack_invocation(
            Some(bad_exports),
            vec![
                arg("kind", &["app"]),
                arg("file", &["dist/Lib/Greet.js", "dist/Lib/Greet.iface"]),
            ],
            vec![tool(PNPM, "abc")],
        );
        let errs = run(&inv).expect_err("missing export rejected");
        assert_eq!(errs[0].code, "P020");
        assert!(errs[0].message.contains("./dist/Nope.js"));
    }

    #[test]
    fn pack_checks_pnpm_grant_and_package_manager_pin_and_stages_manifest() {
        let manifest = r#"{
            "name": "@acme/demo",
            "version": "1.2.3",
            "packageManager": "pnpm@10.5.0",
            "devDependencies": { "vitest": "1.0.0" },
            "scripts": { "test": "vitest" }
        }"#;
        let files = &[
            "dist/App/Main.js",
            "dist/App/Main.iface",
            "dist/flick-main.js",
        ];

        // Missing [tools] grant -> P051
        let ungranted = pack_invocation(
            Some(manifest),
            vec![arg("kind", &["app"]), arg("file", files)],
            vec![],
        );
        let errs = run(&ungranted).expect_err("ungranted pnpm");
        assert_eq!(errs[0].code, "P051");

        // Missing on PATH -> P016
        let missing = pack_invocation(
            Some(manifest),
            vec![arg("kind", &["app"]), arg("file", files)],
            vec![tool("", "")],
        );
        let errs = run(&missing).expect_err("missing pnpm");
        assert_eq!(errs[0].code, "P016");

        // Wrong version when packageManager is declared -> P016
        let wrong = pack_invocation(
            Some(manifest),
            vec![arg("kind", &["app"]), arg("file", files)],
            vec![tool("9.0.0", "abc")],
        );
        let errs = run(&wrong).expect_err("version mismatch");
        assert_eq!(errs[0].code, "P016");
        assert!(errs[0].message.contains("10.5.0"));

        // Matching version + entry fallback (no `runner`, only `entry`) -> stages manifest & pnpm step
        let ok = pack_invocation(
            Some(manifest),
            vec![
                arg("kind", &["app"]),
                arg("entry", &["main"]),
                arg("file", files),
            ],
            vec![tool("10.5.0", "abc")],
        );
        let plan = run(&ok).expect("pack plan");
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].path, ".flick/pack/package.json");
        let staged: Value = serde_json::from_slice(&plan.files[0].bytes).expect("staged json");
        assert_eq!(staged["type"], "module");
        assert_eq!(staged["files"], serde_json::json!(["dist"]));
        assert_eq!(staged["bin"]["demo"], "./dist/flick-main.js");
        assert!(staged.get("devDependencies").is_none());
        assert!(staged.get("scripts").is_none());
        assert_eq!(plan.steps.len(), 1);
        let Step::Tool(step) = &plan.steps[0] else {
            panic!("expected tool step");
        };
        assert!(step.in_place);
        assert_eq!(step.outputs, vec![".flick/pack"]);
        assert_eq!(
            step.arguments,
            vec!["--dir", ".flick/pack", "pack", "--pack-destination", "out"]
        );

        // A non-pnpm `packageManager` does not force an exact match against `PNPM`.
        let non_pnpm = r#"{"name":"demo","version":"1.0.0","packageManager":"npm@10.0.0"}"#;
        let inv = pack_invocation(
            Some(non_pnpm),
            vec![arg("kind", &["app"]), arg("file", files)],
            vec![tool("9.0.0", "abc")],
        );
        assert!(run(&inv).is_ok());
    }
}
