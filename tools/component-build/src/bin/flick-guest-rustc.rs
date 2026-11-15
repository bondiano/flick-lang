//! Give the pinned guest a host-independent crate identity.
//!
//! Cargo hashes the host compiler identity into build-script/proc-macro metadata,
//! then recursively hashes dependency metadata into target crates. Path remapping
//! alone therefore cannot make a cross-host guest reproducible. Replace only the
//! guest's symbol metadata; Cargo still owns filenames, caches and host builds.

use std::{
    env,
    ffi::OsString,
    fs,
    path::{Component, Path},
    process::{self, Command},
};

const TARGET: &str = "wasm32-unknown-unknown";
const HEX: &[u8; 16] = b"0123456789abcdef";

struct Identity {
    source: String,
    package: String,
    version: String,
    compiler: String,
}

fn required(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("missing or non-UTF-8 {name}"))
}

fn canonical_source(manifest: &Path, root: &Path, cargo_home: &Path) -> Result<String, String> {
    for (base, prefix) in [(root, "workspace"), (cargo_home, "cargo")] {
        if let Ok(relative) = manifest.strip_prefix(base) {
            let parts = relative
                .components()
                .map(|part| match part {
                    Component::Normal(value) => value
                        .to_str()
                        .ok_or_else(|| "non-UTF-8 package source".to_owned()),
                    _ => Err("non-canonical package source".to_owned()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(format!("{prefix}/{}", parts.join("/")));
        }
    }
    Err("guest package source is outside the checkout and Cargo home".to_owned())
}

impl Identity {
    fn from_environment() -> Result<Self, String> {
        let path = |name: &str| {
            fs::canonicalize(required(name)?)
                .map_err(|error| format!("cannot resolve {name}: {error}"))
        };
        let source = canonical_source(
            &path("CARGO_MANIFEST_DIR")?,
            &path("FLICK_GUEST_ROOT")?,
            &path("FLICK_GUEST_CARGO_HOME")?,
        )?;
        let compiler = required("FLICK_GUEST_COMPILER_COMMIT")?;
        if compiler.len() != 40 || !compiler.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("guest compiler must have a full commit hash".to_owned());
        }
        Ok(Self {
            source,
            package: required("CARGO_PKG_NAME")?,
            version: required("CARGO_PKG_VERSION")?,
            compiler,
        })
    }
}

fn values(args: &[OsString], name: &str) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let prefix = format!("{name}=");
    for (index, arg) in args.iter().enumerate() {
        let value = if arg == name {
            Some(
                args.get(index + 1)
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| format!("missing or non-UTF-8 {name} value"))?,
            )
        } else {
            arg.to_str().and_then(|arg| arg.strip_prefix(&prefix))
        };
        if let Some(value) = value {
            result.push(value.to_owned());
        }
    }
    Ok(result)
}

fn metadata_positions(args: &[OsString]) -> Vec<(usize, &'static str)> {
    args.iter()
        .enumerate()
        .filter_map(|(index, arg)| {
            let arg = arg.to_str()?;
            for prefix in ["-Cmetadata=", "--codegen=metadata="] {
                if arg.starts_with(prefix) {
                    return Some((index, prefix));
                }
            }
            if arg.starts_with("metadata=")
                && index > 0
                && (args[index - 1] == "-C" || args[index - 1] == "--codegen")
            {
                return Some((index, "metadata="));
            }
            None
        })
        .collect()
}

fn seed(args: &[OsString], identity: &Identity) -> Result<String, String> {
    let names = values(args, "--crate-name")?;
    if names.len() != 1 {
        return Err("guest compilation must name exactly one crate".to_owned());
    }
    let mut kinds = values(args, "--crate-type")?
        .into_iter()
        .flat_map(|value| value.split(',').map(str::to_owned).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    if kinds.is_empty() {
        return Err("guest compilation must specify its crate type".to_owned());
    }
    kinds.sort();
    kinds.dedup();
    let mut cfgs = values(args, "--cfg")?;
    cfgs.sort();
    cfgs.dedup();
    let mut fields = vec![
        "flick-guest-v1".to_owned(),
        identity.source.clone(),
        identity.package.clone(),
        identity.version.clone(),
        names[0].clone(),
        TARGET.to_owned(),
        identity.compiler.clone(),
        kinds.len().to_string(),
    ];
    fields.extend(kinds);
    fields.push(cfgs.len().to_string());
    fields.extend(cfgs);
    // A length-prefixed encoding is injective; hex keeps every byte acceptable
    // in rustc's metadata argument. Rustc supplies the symbol-identity hash.
    let mut raw = String::new();
    for value in fields {
        raw.push_str(&value.len().to_string());
        raw.push(':');
        raw.push_str(&value);
    }
    let mut encoded = String::from("flick_guest_v1_");
    for byte in raw.bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 15)]));
    }
    Ok(encoded)
}

fn normalize(
    args: &mut [OsString],
    identity: impl FnOnce() -> Result<Identity, String>,
) -> Result<(), String> {
    let query = args.iter().any(|arg| {
        arg.to_str().is_some_and(|arg| {
            matches!(
                arg,
                "-vV" | "-V" | "--version" | "-h" | "--help" | "--print" | "--explain"
            ) || arg.starts_with("--print=")
                || arg.starts_with("--explain=")
        })
    });
    let positions = metadata_positions(args);
    if query || positions.is_empty() || values(args, "--target")? != [TARGET] {
        return Ok(());
    }
    let metadata = seed(args, &identity()?)?;
    for (index, prefix) in positions {
        args[index] = format!("{prefix}{metadata}").into();
    }
    Ok(())
}

fn run() -> Result<i32, String> {
    let mut args = env::args_os().skip(1);
    let compiler = args.next().ok_or("expected rustc executable")?;
    let mut args = args.collect::<Vec<_>>();
    normalize(&mut args, Identity::from_environment)?;
    let status = Command::new(compiler)
        .args(args)
        .status()
        .map_err(|error| format!("cannot execute rustc: {error}"))?;
    Ok(status.code().unwrap_or(1))
}

fn main() {
    match run() {
        Ok(code) => process::exit(code),
        Err(error) => {
            eprintln!("flick-guest-rustc: {error}");
            process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> Identity {
        Identity {
            source: "cargo/registry/src/crates-io/example-1.0.0".into(),
            package: "example".into(),
            version: "1.0.0".into(),
            compiler: "59807616e1fa2540724bfbac14d7976d7e4a3860".into(),
        }
    }

    fn args(metadata: &str) -> Vec<OsString> {
        [
            "--crate-name",
            "example",
            "--crate-type",
            "lib",
            "--target",
            TARGET,
            "--cfg",
            "feature=\"std\"",
            "-C",
            metadata,
            "-Cextra-filename=-cargo-hash",
            "--extern",
            "other=libother-cargo-hash.rlib",
        ]
        .map(OsString::from)
        .to_vec()
    }

    #[test]
    fn host_derived_metadata_does_not_change_guest_identity() {
        let mut mac = args("metadata=mac-host-hash");
        let mut linux = args("metadata=linux-host-hash");
        let unchanged = mac[10..].to_vec();
        normalize(&mut mac, || Ok(identity())).expect("canonical metadata");
        normalize(&mut linux, || Ok(identity())).expect("canonical metadata");
        assert_eq!(mac, linux);
        assert_eq!(mac[10..], unchanged);
    }

    #[test]
    fn package_source_version_features_and_compiler_are_distinct() {
        let original = seed(&args("metadata=old"), &identity()).expect("seed");
        for field in ["source", "package", "version", "compiler"] {
            let mut changed = identity();
            match field {
                "source" => changed.source.push_str("/another-source"),
                "package" => changed.package.push_str("-other"),
                "version" => changed.version = "2.0.0".into(),
                _ => changed.compiler = "a".repeat(40),
            }
            assert_ne!(
                original,
                seed(&args("metadata=old"), &changed).expect("seed")
            );
        }
        for (index, value) in [(1, "other_crate"), (3, "cdylib"), (7, "feature=\"alloc\"")] {
            let mut changed = args("metadata=old");
            changed[index] = value.into();
            assert_ne!(original, seed(&changed, &identity()).expect("seed"));
        }
    }

    #[test]
    fn cfg_order_and_metadata_option_spellings_are_canonical() {
        let base = args("metadata=old");
        let expected = seed(&base, &identity()).expect("seed");
        for flags in [
            vec!["-C", "metadata=host"],
            vec!["-Cmetadata=host"],
            vec!["--codegen", "metadata=host"],
            vec!["--codegen=metadata=host"],
        ] {
            let mut changed = base[..8].to_vec();
            changed.extend(flags.into_iter().map(OsString::from));
            normalize(&mut changed, || Ok(identity())).expect("canonical metadata");
            let (index, prefix) = metadata_positions(&changed)[0];
            assert_eq!(
                changed[index],
                OsString::from(format!("{prefix}{expected}"))
            );
        }
        let mut first = base.clone();
        first.extend(["--cfg", "custom"].map(OsString::from));
        let mut second = base[..6].to_vec();
        second.extend(["--cfg=custom", "--cfg=feature=\"std\""].map(OsString::from));
        assert_eq!(
            seed(&first, &identity()).expect("seed"),
            seed(&second, &identity()).expect("seed")
        );
    }

    #[test]
    fn host_and_queries_pass_through_without_package_environment() {
        let mut host = args("metadata=host");
        host.drain(4..6);
        let mut query = args("metadata=query");
        query.push("--print=cfg".into());
        let mut version = vec![OsString::from("-vV")];
        for args in [&mut host, &mut query, &mut version] {
            let original = args.clone();
            normalize(args, || panic!("pass-through must not read identity"))
                .expect("pass-through");
            assert_eq!(*args, original);
        }
    }

    #[test]
    fn source_identity_omits_host_roots_and_rejects_external_sources() {
        assert_eq!(
            canonical_source(
                Path::new("/mac/checkout/backends/js"),
                Path::new("/mac/checkout"),
                Path::new("/mac/cargo")
            )
            .expect("source"),
            canonical_source(
                Path::new("/linux/checkout/backends/js"),
                Path::new("/linux/checkout"),
                Path::new("/linux/cargo")
            )
            .expect("source")
        );
        assert_eq!(
            canonical_source(
                Path::new("/mac/cargo/registry/src/index/example"),
                Path::new("/mac/checkout"),
                Path::new("/mac/cargo")
            )
            .expect("source"),
            "cargo/registry/src/index/example"
        );
        assert!(
            canonical_source(
                Path::new("/external/source"),
                Path::new("/checkout"),
                Path::new("/cargo")
            )
            .is_err()
        );
    }
}
