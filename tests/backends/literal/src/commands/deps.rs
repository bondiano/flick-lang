//! Fixture dependency verbs (`install`, `add`, `remove`, `update`, `deps`)
//! operating on `literal-deps.json` and `literal-deps.lock`.

use std::fmt::Write as _;

use flick_backend::command::{
    Descriptor, Diagnostic, Invocation, Parameter, Plan, Style, ValueKind,
};

use super::{file, value, values};

pub(super) fn commands() -> Vec<Descriptor> {
    vec![
        dep_descriptor(
            "install",
            "Installs literal ecosystem packages from `literal-deps.json`",
            vec![],
            vec![
                dep_param("locked", ValueKind::Flag, false, false, "Require lockfile"),
                dep_param("offline", ValueKind::Flag, false, false, "Offline mode"),
            ],
            &["literal-deps.json", "literal-deps.lock"],
        ),
        dep_descriptor(
            "add",
            "Adds literal ecosystem packages",
            vec![dep_param(
                "packages",
                ValueKind::Text,
                true,
                true,
                "Literal package specs `name` or `name@version`",
            )],
            vec![dep_param(
                "dev",
                ValueKind::Flag,
                false,
                false,
                "Dev package",
            )],
            &["literal-deps.json", "literal-deps.lock"],
        ),
        dep_descriptor(
            "remove",
            "Removes literal ecosystem packages",
            vec![dep_param(
                "packages",
                ValueKind::Text,
                true,
                true,
                "Literal package names",
            )],
            vec![],
            &["literal-deps.json", "literal-deps.lock"],
        ),
        dep_descriptor(
            "update",
            "Updates literal ecosystem packages",
            vec![dep_param(
                "packages",
                ValueKind::Text,
                true,
                false,
                "Literal package names",
            )],
            vec![],
            &["literal-deps.json", "literal-deps.lock"],
        ),
        dep_descriptor(
            "deps",
            "Lists literal ecosystem packages",
            vec![],
            vec![dep_param(
                "why",
                ValueKind::Text,
                false,
                false,
                "Explain why package is present",
            )],
            &[],
        ),
    ]
}

fn dep_param(name: &str, kind: ValueKind, multiple: bool, required: bool, help: &str) -> Parameter {
    Parameter {
        name: name.into(),
        kind,
        multiple,
        required,
        help: help.into(),
    }
}

fn dep_descriptor(
    name: &str,
    summary: &str,
    positionals: Vec<Parameter>,
    options: Vec<Parameter>,
    writes: &[&str],
) -> Descriptor {
    Descriptor {
        name: name.into(),
        summary: summary.into(),
        positionals,
        options,
        inputs: vec!["literal-deps.json".into(), "literal-deps.lock".into()],
        reads: vec![],
        writes: writes.iter().map(|&w| w.to_owned()).collect(),
        style: Style::Plan,
    }
}

struct LiteralManifest {
    doc: serde_json::Value,
    deps: serde_json::Map<String, serde_json::Value>,
    installed: Vec<String>,
    has_lock: bool,
}

impl LiteralManifest {
    fn load(invocation: &Invocation) -> Result<Self, Vec<Diagnostic>> {
        let manifest_file = invocation
            .inputs
            .iter()
            .find(|f| f.path == "literal-deps.json");
        let has_lock = invocation
            .inputs
            .iter()
            .any(|f| f.path == "literal-deps.lock");
        let mut doc: serde_json::Value = match manifest_file {
            Some(f) => serde_json::from_slice(&f.bytes).map_err(|e| {
                vec![Diagnostic::error(
                    "P005",
                    format!("`literal-deps.json` is not JSON: {e}"),
                )]
            })?,
            None => serde_json::json!({ "dependencies": {}, "installed": [] }),
        };
        let deps = doc
            .as_object_mut()
            .expect("object")
            .entry("dependencies")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .expect("dependencies map")
            .clone();
        let installed = doc["installed"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            doc,
            deps,
            installed,
            has_lock,
        })
    }

    fn write_to(&mut self, plan: &mut Plan) {
        self.installed.sort();
        self.doc["dependencies"] = serde_json::Value::Object(self.deps.clone());
        self.doc["installed"] = serde_json::json!(self.installed);
        plan.files.push(file(
            "literal-deps.json",
            &format!(
                "{}\n",
                serde_json::to_string_pretty(&self.doc).expect("json")
            ),
        ));
        plan.files.push(file(
            "literal-deps.lock",
            &format!("{}\n", self.installed.join("\n")),
        ));
    }
}

pub(super) fn run(invocation: &Invocation) -> Result<Plan, Vec<Diagnostic>> {
    let mut state = LiteralManifest::load(invocation)?;
    let mut plan = Plan::empty();
    match invocation.name.as_str() {
        "install" => {
            let locked = !values(invocation, "locked").is_empty();
            let mut added = false;
            for dep in &invocation.dependencies {
                if !state.deps.contains_key(&dep.name) {
                    state
                        .deps
                        .insert(dep.name.clone(), dep.version.clone().into());
                    added = true;
                }
            }
            if locked && added {
                return Err(vec![Diagnostic::error(
                    "P034",
                    "`literal-deps.json` lacks packages that `[target.literal.dependencies]` name: run `flick install` without `--locked`".into(),
                )]);
            }
            if locked && !state.has_lock {
                return Err(vec![Diagnostic::error(
                    "P017",
                    "`install --locked` needs `literal-deps.lock`; run `flick install` to create it"
                        .into(),
                )]);
            }
            for pkg in state.deps.keys() {
                if !state.installed.contains(pkg) {
                    state.installed.push(pkg.clone());
                }
            }
            if !locked {
                state.write_to(&mut plan);
            }
        }
        "add" => {
            for spec in values(invocation, "packages") {
                let (pkg, ver) = spec.split_once('@').unwrap_or((spec.as_str(), "1.0.0"));
                state.deps.insert(pkg.to_owned(), ver.to_owned().into());
                if !state.installed.iter().any(|n| n == pkg) {
                    state.installed.push(pkg.to_owned());
                }
            }
            state.write_to(&mut plan);
        }
        "remove" => {
            for pkg in values(invocation, "packages") {
                state.deps.remove(pkg);
                state.installed.retain(|n| n != pkg);
            }
            state.write_to(&mut plan);
        }
        "update" => {
            for dep in &invocation.dependencies {
                state
                    .deps
                    .entry(dep.name.clone())
                    .or_insert_with(|| dep.version.clone().into());
            }
            for pkg in state.deps.keys() {
                if !state.installed.contains(pkg) {
                    state.installed.push(pkg.clone());
                }
            }
            state.write_to(&mut plan);
        }
        "deps" => {
            let mut out = String::new();
            if let Some(why) = value(invocation, "why") {
                if let Some(ver) = state.deps.get(why).and_then(serde_json::Value::as_str) {
                    let _ = writeln!(out, "{why}@{ver} (literal)");
                }
            } else {
                for (pkg, ver) in &state.deps {
                    let _ = writeln!(out, "{pkg} {}", ver.as_str().unwrap_or_default());
                }
            }
            plan.stdout = out.into_bytes();
        }
        _ => {}
    }
    Ok(plan)
}
