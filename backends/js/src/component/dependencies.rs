//! The dependency commands of a JavaScript project: `install`, `add`,
//! `remove`, `update` and `deps`, which `flick` runs after its own part on
//! the Flick libraries. Each is a plan that runs pnpm in the project's root,
//! the version `packageManager` in `package.json` pins or else this
//! backend's own; any other pnpm changes nothing. `install` and `update`
//! first add to `package.json` what the closure's
//! `[target.js.dependencies]` asks for.

use flick_backend::command::{
    Descriptor, Diagnostic, File, Invocation, Parameter, Plan, Step, Style, Tool, ToolStep,
    ValueKind,
};
use serde_json::Value;

use super::manifest;

/// The pnpm a project takes without `packageManager`.
// shortcut: kept equal to `toolchain::PNPM` of flick by hand; one copy once `new` writes its templates from this backend
pub(super) const PNPM: &str = "12.16.1";

/// What pnpm may change in the project's root.
const WRITES: [&str; 4] = [
    "package.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "node_modules",
];

pub(super) const NAMES: [&str; 5] = ["install", "add", "remove", "update", "deps"];

fn parameter(name: &str, kind: ValueKind, multiple: bool, required: bool, help: &str) -> Parameter {
    Parameter {
        name: name.into(),
        kind,
        multiple,
        required,
        help: help.into(),
    }
}

pub(super) fn descriptors() -> Vec<Descriptor> {
    let packages = |required| {
        parameter(
            "packages",
            ValueKind::Text,
            true,
            required,
            "npm package specs, such as `date-fns` or `date-fns@4`",
        )
    };
    let flag = |name: &str, help: &str| parameter(name, ValueKind::Flag, false, false, help);
    let command = |name: &str, summary: &str, positionals, options| Descriptor {
        name: name.into(),
        summary: summary.into(),
        positionals,
        options,
        inputs: vec!["package.json".into(), "pnpm-lock.yaml".into()],
        reads: vec![],
        writes: if name == "deps" {
            vec![]
        } else {
            WRITES.map(str::to_owned).to_vec()
        },
        style: Style::Plan,
    };
    vec![
        command(
            "install",
            "Installs the npm packages of `package.json` with the pinned pnpm",
            vec![],
            vec![
                flag("locked", "Install exactly `pnpm-lock.yaml`"),
                flag("offline", "Use only the packages pnpm has stored"),
            ],
        ),
        command(
            "add",
            "Adds npm packages to `package.json`",
            vec![packages(true)],
            vec![flag("dev", "Add them to `devDependencies`")],
        ),
        command(
            "remove",
            "Removes npm packages from `package.json`",
            vec![packages(true)],
            vec![],
        ),
        command(
            "update",
            "Updates npm packages within the ranges of `package.json`",
            vec![packages(false)],
            vec![],
        ),
        command(
            "deps",
            "Lists the installed npm packages, or why one is installed",
            vec![],
            vec![parameter(
                "why",
                ValueKind::Text,
                false,
                false,
                "Show why this package is installed",
            )],
        ),
    ]
}

fn values<'a>(invocation: &'a Invocation, name: &str) -> &'a [String] {
    (invocation.arguments.iter())
        .find(|a| a.name == name)
        .map_or(&[], |a| a.values.as_slice())
}

fn flag(invocation: &Invocation, name: &str) -> bool {
    !values(invocation, name).is_empty()
}

fn input<'a>(invocation: &'a Invocation, path: &str) -> Option<&'a File> {
    invocation.inputs.iter().find(|f| f.path == path)
}

pub(super) fn run(invocation: &Invocation) -> Result<Plan, Vec<Diagnostic>> {
    let text = input(invocation, "package.json").map(|f| String::from_utf8_lossy(&f.bytes));
    let parsed: Option<Value> = text.as_deref().and_then(|t| serde_json::from_str(t).ok());
    let name = invocation.name.as_str();
    let merged = if matches!(name, "install" | "update") {
        manifest::merged(text.as_deref(), &invocation.dependencies)
            .map_err(|message| vec![Diagnostic::error("P005", message)])?
    } else {
        None
    };
    // a project without npm packages has nothing to install, update or list
    if text.is_none() && merged.is_none() && matches!(name, "install" | "update" | "deps") {
        return Ok(Plan::empty());
    }
    let pnpm = pnpm(invocation, parsed.as_ref(), name != "deps")?;
    let locked = flag(invocation, "locked");
    let mut plan = Plan::empty();
    if let Some(merged) = merged {
        if locked {
            return Err(vec![Diagnostic::error(
                "P034",
                "`package.json` lacks npm packages that `[target.js.dependencies]` name: run `flick install` without `--locked`".into(),
            )]);
        }
        plan.files.push(File {
            path: "package.json".into(),
            bytes: merged.into_bytes(),
            executable: false,
        });
    }
    if locked && input(invocation, "pnpm-lock.yaml").is_none() {
        return Err(vec![Diagnostic::error(
            "P017",
            "`install --locked` needs `pnpm-lock.yaml`; run `flick install` to create it".into(),
        )]);
    }
    let packages = values(invocation, "packages").iter().cloned();
    let arguments: Vec<String> = match name {
        "install" => ["install".to_owned()]
            .into_iter()
            .chain(locked.then(|| "--frozen-lockfile".to_owned()))
            .chain(flag(invocation, "offline").then(|| "--offline".to_owned()))
            .collect(),
        "add" => ["add".to_owned()]
            .into_iter()
            .chain(flag(invocation, "dev").then(|| "--save-dev".to_owned()))
            .chain(packages)
            .collect(),
        "remove" | "update" => [name.to_owned()].into_iter().chain(packages).collect(),
        _ => match values(invocation, "why").first() {
            Some(package) => vec!["why".into(), package.clone()],
            None => vec!["list".into(), "--depth".into(), "Infinity".into()],
        },
    };
    plan.steps.push(Step::Tool(ToolStep {
        id: name.into(),
        tool: pnpm,
        arguments,
        environment: vec![],
        inputs: vec![],
        reads: vec![],
        outputs: if name == "deps" {
            vec![]
        } else {
            WRITES.map(str::to_owned).to_vec()
        },
        in_place: true,
    }));
    Ok(plan)
}

/// The pnpm the host found and the project allows; with `exact`, only the
/// version the project pins, else nothing is changed.
fn pnpm(
    invocation: &Invocation,
    manifest: Option<&Value>,
    exact: bool,
) -> Result<Tool, Vec<Diagnostic>> {
    let pinned = (manifest.and_then(|m| m["packageManager"].as_str()))
        .and_then(|spec| spec.strip_prefix("pnpm@"))
        .map_or(PNPM, |v| v.split('+').next().unwrap_or(v));
    let instruction = format!(
        "run `corepack prepare pnpm@{pinned} --activate`, or `npm install --global pnpm@{pinned}`"
    );
    let next = "nothing was changed (`flick doctor` lists the toolchain versions)";
    match invocation.tools.iter().find(|t| t.id == "pnpm") {
        None => {
            let mut d = Diagnostic::error(
                "P051",
                format!(
                    "`{}` runs `pnpm`, which `[tools]` does not allow `flick/js`",
                    invocation.name
                ),
            );
            d.help = Some("allow it in `flick.toml`: `[tools]` `\"flick/js\" = [\"pnpm\"]`".into());
            Err(vec![d])
        }
        Some(t) if t.sha256.is_empty() => Err(vec![Diagnostic::error(
            "P016",
            format!(
                "`pnpm` is not on PATH and the project pins pnpm {pinned} in `packageManager`: {instruction}; {next}"
            ),
        )]),
        Some(t) if exact && t.version != pinned => Err(vec![Diagnostic::error(
            "P016",
            format!(
                "the project pins pnpm {pinned} in `packageManager`, but `pnpm` is {}: {instruction}; {next}",
                t.version
            ),
        )]),
        Some(t) => Ok(t.clone()),
    }
}
