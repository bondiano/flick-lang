//! The pnpm adapter: the version a project pins through `packageManager`
//! in its `package.json`, and how Flick's dependency commands map to pnpm.

/// The pnpm version `package.json` pins: `"packageManager": "pnpm@12.14.0"`,
/// without a `+sha…` suffix.
pub fn pinned(package_json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(package_json).ok()?;
    let spec = value.get("packageManager")?.as_str()?;
    let version = spec.strip_prefix("pnpm@")?;
    Some(version.split('+').next().unwrap_or(version).to_owned())
}

/// A dependency command of Flick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Install { locked: bool, offline: bool },
    Add { packages: Vec<String>, dev: bool },
    Remove { packages: Vec<String> },
    Update { packages: Vec<String> },
    Why { package: String },
    List,
}

/// The arguments of pnpm for `command`.
pub fn args(command: &Command) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    match command {
        Command::Install { locked, offline } => {
            out.push("install".into());
            if *locked {
                out.push("--frozen-lockfile".into());
            }
            if *offline {
                out.push("--offline".into());
            }
        }
        Command::Add { packages, dev } => {
            out.push("add".into());
            if *dev {
                out.push("--save-dev".into());
            }
            out.extend(packages.iter().cloned());
        }
        Command::Remove { packages } => {
            out.push("remove".into());
            out.extend(packages.iter().cloned());
        }
        Command::Update { packages } => {
            out.push("update".into());
            out.extend(packages.iter().cloned());
        }
        Command::Why { package } => {
            out.extend(["why".into(), package.clone()]);
        }
        Command::List => out.extend(["list".into(), "--depth".into(), "Infinity".into()]),
    }
    out
}

/// Whether the command changes the manifest, the lockfile or the tree:
/// those first check the pnpm version.
pub fn writes(command: &Command) -> bool {
    !matches!(command, Command::Why { .. } | Command::List)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_version_drops_the_hash() {
        assert_eq!(
            pinned(r#"{"packageManager": "pnpm@12.14.0+sha512.abc"}"#).as_deref(),
            Some("12.14.0")
        );
        assert_eq!(pinned(r#"{"packageManager": "yarn@4.0.0"}"#), None);
        assert_eq!(pinned("{}"), None);
    }

    #[test]
    fn locked_and_offline_map_to_pnpm_flags() {
        let install = Command::Install {
            locked: true,
            offline: true,
        };
        assert_eq!(
            args(&install),
            ["install", "--frozen-lockfile", "--offline"]
        );
        let add = Command::Add {
            packages: vec!["date-fns".into()],
            dev: true,
        };
        assert_eq!(args(&add), ["add", "--save-dev", "date-fns"]);
    }
}
