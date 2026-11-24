//! Backend pack artifact collection, collision-safe materialization, and
//! `.iface` verification.

use std::path::{Component, Path, PathBuf};
use std::{fs, io};

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::iface;

use super::failure;
use crate::args::GlobalArguments;
use crate::cache::files_under;
use crate::command::Outcome;
use crate::report;

/// Collects regular files staged by a backend under `stage_out`, refusing any
/// symlink (including `stage_out` itself or a parent leaving `project_root`) or
/// non-normal relative path before writing into `out_dir`.
pub(super) fn collect_backend_artifacts(
    project_root: &Path,
    stage_out: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>, Outcome> {
    let meta = match fs::symlink_metadata(stage_out) {
        Ok(meta) => meta,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(failure(
                codes::P013,
                format!("cannot inspect `{}`: {err}", stage_out.display()),
            ));
        }
    };
    if meta.file_type().is_symlink() {
        return Err(failure(
            codes::P013,
            format!(
                "backend pack output directory `{}` cannot be a symlink",
                stage_out.display()
            ),
        ));
    }
    if !meta.file_type().is_dir() {
        return Err(failure(
            codes::P013,
            format!(
                "backend pack output `{}` is not a directory",
                stage_out.display()
            ),
        ));
    }
    let canonical_root = fs::canonicalize(project_root).map_err(|err| {
        failure(
            codes::P013,
            format!(
                "cannot resolve project root `{}`: {err}",
                project_root.display()
            ),
        )
    })?;
    let canonical_stage_out = fs::canonicalize(stage_out).map_err(|err| {
        failure(
            codes::P013,
            format!(
                "cannot resolve backend pack output `{}`: {err}",
                stage_out.display()
            ),
        )
    })?;
    if !canonical_stage_out.starts_with(&canonical_root) {
        return Err(failure(
            codes::P013,
            format!(
                "backend pack output directory `{}` leaves the project root",
                stage_out.display()
            ),
        ));
    }
    let mut collected = Vec::new();
    walk_stage_out(stage_out, stage_out, &mut collected)?;
    collected.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(collected)
}

fn walk_stage_out(
    base: &Path,
    dir: &Path,
    out: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), Outcome> {
    let entries = fs::read_dir(dir).map_err(|err| {
        failure(
            codes::P013,
            format!("cannot read `{}`: {err}", dir.display()),
        )
    })?;
    let mut paths: Vec<PathBuf> = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            failure(
                codes::P013,
                format!("cannot read `{}`: {err}", dir.display()),
            )
        })?
        .into_iter()
        .map(|e| e.path())
        .collect();
    paths.sort();
    for path in paths {
        let meta = fs::symlink_metadata(&path).map_err(|err| {
            failure(
                codes::P013,
                format!("cannot inspect `{}`: {err}", path.display()),
            )
        })?;
        if meta.file_type().is_symlink() {
            return Err(failure(
                codes::P013,
                format!(
                    "backend pack output `{}` cannot be a symlink",
                    path.display()
                ),
            ));
        }
        if meta.file_type().is_dir() {
            walk_stage_out(base, &path, out)?;
        } else if meta.file_type().is_file() {
            let rel = path.strip_prefix(base).map_err(|_| {
                failure(
                    codes::P013,
                    format!("backend pack output `{}` leaves staging", path.display()),
                )
            })?;
            if rel.as_os_str().is_empty()
                || !rel.components().all(|c| matches!(c, Component::Normal(_)))
            {
                return Err(failure(
                    codes::P013,
                    format!("invalid backend pack output path `{}`", rel.display()),
                ));
            }
            out.push((rel.to_path_buf(), path));
        }
    }
    Ok(())
}

/// Moves validated backend artifacts into `out`. Any artifact whose filename
/// collides with `source_file` is placed under `out/<target_short>/` so both
/// the Flick source tarball and the backend artifact remain intact.
pub(super) fn move_backend_artifacts(
    artifacts: &[(PathBuf, PathBuf)],
    out: &Path,
    source_file: Option<&Path>,
    target_short: &str,
    global: &GlobalArguments,
) -> Option<Outcome> {
    if let Err(err) = fs::create_dir_all(out) {
        return Some(failure(
            codes::P013,
            format!("cannot create `{}`: {err}", out.display()),
        ));
    }
    for (rel, src) in artifacts {
        let mut dest = out.join(rel);
        let collided = source_file.is_some_and(|sf| sf == dest);
        if collided {
            dest = out.join(target_short).join(rel);
        }
        if let Some(parent) = dest.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            return Some(failure(
                codes::P013,
                format!("cannot create `{}`: {err}", parent.display()),
            ));
        }
        if let Err(err) = fs::rename(src, &dest)
            .or_else(|_| fs::copy(src, &dest).and_then(|_| fs::remove_file(src)))
        {
            return Some(failure(
                codes::P013,
                format!(
                    "cannot move `{}` to `{}`: {err}",
                    src.display(),
                    dest.display()
                ),
            ));
        }
        if collided {
            report::log(
                global,
                &format!(
                    "backend `{target_short}` artifact written to `{}`",
                    dest.display()
                ),
            );
        }
    }
    None
}

/// Every emitted `.iface` under `dist` must be readable and agree with the
/// fingerprints of any dependency interfaces also emitted in `dist`.
pub(super) fn verify_interfaces(dist: &Path) -> Vec<Diagnostic> {
    let mut files = Vec::new();
    files_under(dist, &mut files);
    let mut out = Vec::new();
    for path in files
        .iter()
        .filter(|f| f.extension().is_some_and(|e| e == "iface"))
    {
        let module = relative(path, dist)
            .trim_end_matches(".iface")
            .replace('/', ".");
        let Some(interface) = fs::read(path)
            .ok()
            .and_then(|b| iface::read(&b, &module).ok())
        else {
            out.push(Diagnostic::new(
                codes::P020,
                Severity::Error,
                format!("`{module}` has an unreadable interface"),
            ));
            continue;
        };
        for (dependency, fingerprint) in &interface.dependencies {
            let dep_path = dist.join(format!("{}.iface", dependency.replace('.', "/")));
            let Some(dep) = fs::read(&dep_path)
                .ok()
                .and_then(|b| iface::read(&b, dependency).ok())
            else {
                continue;
            };
            let actual = match iface::fingerprint(&dep) {
                Ok(actual) => actual,
                Err(error) => {
                    out.push(Diagnostic::new(codes::P020, Severity::Error, error));
                    continue;
                }
            };
            if &actual != fingerprint {
                out.push(Diagnostic::new(
                    codes::P020,
                    Severity::Error,
                    format!("`{module}` was built against another interface of `{dependency}`"),
                ));
            }
        }
    }
    out
}

fn relative(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::{env, fs, process};

    use flick_core::diagnostics::codes;

    use super::{collect_backend_artifacts, move_backend_artifacts, verify_interfaces};
    use crate::args::{BuildArguments, Color, DiagnosticsFormat, GlobalArguments, ProfileArgument};
    use crate::command::Command;
    use crate::commands::build::{BuildCommand, write};

    fn test_global() -> GlobalArguments {
        GlobalArguments {
            diagnostics: DiagnosticsFormat::Human,
            color: Color::Never,
            quiet: true,
            verbose: false,
            timings: false,
            project: None,
            package: None,
            warnings_as_errors: false,
            offline: false,
            allow_tool: Vec::new(),
        }
    }

    #[test]
    fn verify_interfaces_reports_unreadable_and_mismatched_fingerprints() {
        let root = env::temp_dir().join(format!("flick-pack-iface-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        write(
            &root.join("flick.toml"),
            "schema-version = 1\n[project]\nkind = \"lib\"\n",
        )
        .expect("config");
        write(
            &root.join("src/Lib/Base.fl"),
            "module Lib.Base exposing (base)\nbase : String\nlet base = \"hi\"\n",
        )
        .expect("base");
        write(
            &root.join("src/Lib/Greeting.fl"),
            "module Lib.Greeting exposing (greet)\nimport Lib.Base\ngreet : String -> String\nlet greet name = Lib.Base.base ++ name\n",
        )
        .expect("greeting");
        let built = BuildCommand::new(
            BuildArguments {
                paths: Vec::new(),
                target: Some("js".into()),
                out_dir: None,
                emit_ir: None,
                profile: Some(ProfileArgument::Release),
                labels: None,
                inputs: false,
                watch: false,
                no_native_overrides: false,
                backend_options: BTreeMap::new(),
            },
            Some(root.to_string_lossy().into_owned()),
            None,
        )
        .run();
        assert!(built.diagnostics.is_empty(), "{:?}", built.diagnostics);
        let dist = root.join("dist");
        assert!(verify_interfaces(&dist).is_empty());

        // Changing a dependency interface breaks its fingerprint match.
        let base_iface = dist.join("Lib/Base.iface");
        let orig_base = fs::read_to_string(&base_iface).expect("read base iface");
        fs::write(&base_iface, orig_base.replace("\"String\"", "\"Int\"")).expect("tamper base");
        let mismatched = verify_interfaces(&dist);
        assert!(
            mismatched
                .iter()
                .any(|d| d.code == codes::P020 && d.message.contains("Lib.Base")),
            "{mismatched:?}"
        );

        // Unreadable interface triggers P020.
        fs::write(&base_iface, "{not json").expect("corrupt base");
        let unreadable = verify_interfaces(&dist);
        assert!(
            unreadable
                .iter()
                .any(|d| d.code == codes::P020 && d.message.contains("unreadable interface")),
            "{unreadable:?}"
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn move_backend_artifacts_isolates_collisions_and_refuses_symlinks() {
        let root = env::temp_dir().join(format!("flick-pack-move-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        let stage_out = root.join("stage_out");
        let out = root.join("out");
        fs::create_dir_all(&stage_out).expect("stage_out");
        fs::create_dir_all(&out).expect("out");

        let source_tarball = out.join("demo-1.0.0.tgz");
        fs::write(&source_tarball, b"flick-source").expect("source");
        fs::write(stage_out.join("demo-1.0.0.tgz"), b"backend-archive").expect("colliding");
        fs::write(stage_out.join("extra.pkg"), b"extra").expect("extra");

        let collected = collect_backend_artifacts(&root, &stage_out)
            .ok()
            .expect("collect");
        assert_eq!(collected.len(), 2);
        let moved = move_backend_artifacts(
            &collected,
            &out,
            Some(&source_tarball),
            "js",
            &test_global(),
        );
        assert!(moved.is_none(), "expected clean move");
        assert_eq!(fs::read(&source_tarball).expect("source"), b"flick-source");
        assert_eq!(
            fs::read(out.join("js/demo-1.0.0.tgz")).expect("isolated"),
            b"backend-archive"
        );
        assert_eq!(fs::read(out.join("extra.pkg")).expect("extra"), b"extra");

        let missing = collect_backend_artifacts(&root, &root.join("missing"))
            .ok()
            .expect("missing stage_out");
        assert!(missing.is_empty());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link_dir = root.join("link_stage");
            fs::create_dir_all(&link_dir).expect("link_stage");
            symlink(&source_tarball, link_dir.join("evil.tgz")).expect("symlink");
            let err = collect_backend_artifacts(&root, &link_dir).expect_err("symlink refused");
            assert!(
                err.diagnostics.iter().any(|d| d.code == codes::P013),
                "{:?}",
                err.diagnostics
            );

            // Symlinked `stage_out` pointing outside `project_root` is refused without touching external files.
            let outside = env::temp_dir().join(format!("flick-pack-outside-{}", process::id()));
            let _ = fs::remove_dir_all(&outside);
            fs::create_dir_all(outside.join("nested")).expect("outside");
            let secret = outside.join("secret.tgz");
            let nested_secret = outside.join("nested/secret.tgz");
            fs::write(&secret, b"external").expect("secret");
            fs::write(&nested_secret, b"nested-external").expect("nested secret");

            let stage_symlink = root.join("stage_out_link");
            symlink(&outside, &stage_symlink).expect("stage_out symlink");
            let err = collect_backend_artifacts(&root, &stage_symlink)
                .expect_err("symlinked stage_out refused");
            assert!(
                err.diagnostics.iter().any(|d| d.code == codes::P013),
                "{:?}",
                err.diagnostics
            );
            assert_eq!(fs::read(&secret).expect("external untouched"), b"external");

            // Symlinked parent causing `stage_out` to resolve outside `project_root` is also refused.
            let parent_symlink = root.join("escape_parent");
            symlink(&outside, &parent_symlink).expect("parent symlink");
            let err = collect_backend_artifacts(&root, &parent_symlink.join("nested"))
                .expect_err("parent symlink escape refused");
            assert!(
                err.diagnostics.iter().any(|d| d.code == codes::P013),
                "{:?}",
                err.diagnostics
            );
            assert_eq!(
                fs::read(&nested_secret).expect("nested external untouched"),
                b"nested-external"
            );
            fs::remove_dir_all(&outside).expect("cleanup outside");
        }

        fs::remove_dir_all(root).expect("cleanup");
    }
}
