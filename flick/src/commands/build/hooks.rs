//! What plugins do with a build besides blocks, a plugin after another in
//! the order of the dependencies: `ir` gets the IR of the project's modules
//! before the build optimizes it and gives what it compiles instead; `emit`
//! gets the files `flick build` writes and gives what it writes instead.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use flick_core::build::{Artifact, effects};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::plugin::Failure;
use flick_core::project::{self, Compilation, Project};
use flick_core::source::SourceMap;
use flick_core::types;
use flick_ir::{Module, Program, VERSION, validate};

use super::Emission;
use crate::sources::FsHost;
use crate::{plugins, toolchain};

/// The IR of `compilation` as each plugin whose package says `hooks =
/// ["ir"]` gives it, after the checks of effects pass it; IR that does not
/// hold together is `FLICK-P047`, and a rewrite is the note `FLICK-P048`.
pub(crate) fn ir(
    project: Option<&Project>,
    compilation: &mut Compilation,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(project) = project else {
        return;
    };
    let hooked = project::hooked_plugins(&FsHost::new(), &mut SourceMap::default(), project, "ir");
    if hooked.is_empty() || diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return;
    }
    // a plugin gets only code the checks of effects pass, which then report it
    let mut found = Vec::new();
    for m in &compilation.modules {
        effects::check(&m.ir, &mut found);
    }
    if found.iter().any(|d| d.severity == Severity::Error) {
        return;
    }
    let mut files = BTreeSet::new();
    for m in &compilation.modules {
        m.ir.clone().each_span_mut(&mut |s| {
            files.insert(s.file);
        });
    }
    let builtin = |m: &str| types::builtins().iter().any(|b| b.module == m);
    let error = |code, message: String| Diagnostic::new(code, Severity::Error, message);
    for (package, component) in hooked {
        let plugin = match plugins::plugin(&package, component.as_deref()) {
            Ok(plugin) => plugin,
            Err(why) => {
                let message = format!("the plugin of package `{package}` cannot be loaded: {why}");
                diagnostics.push(error(codes::P035, message));
                return;
            }
        };
        let given = Program {
            ir_version: VERSION.into(),
            compiler: format!("flick {}", toolchain::COMPILER),
            files: Vec::new(),
            modules: compilation.modules.iter().map(|m| m.ir.clone()).collect(),
        };
        let written = match plugin.transform(&given.to_cbor()) {
            Ok(written) => written,
            Err(Failure::Refused(found)) => {
                let said = found
                    .into_iter()
                    .map(|d| format!("plugin `{package}`: {}", d.message));
                diagnostics.extend(said.map(|message| error(codes::P036, message)));
                return;
            }
            Err(Failure::Broken(why)) => {
                let message = format!("plugin `{package}` failed to transform the IR: {why}");
                diagnostics.push(error(codes::P036, message));
                return;
            }
        };
        let mut problems = match Program::from_cbor(&written) {
            Ok(program) => {
                let mut problems = validate(&program, &builtin, &files);
                let names = |p: &Program| {
                    p.modules
                        .iter()
                        .map(|m| m.name.clone())
                        .collect::<BTreeSet<_>>()
                };
                if names(&program) != names(&given) {
                    problems.push("the modules are not those of the build".into());
                }
                if problems.is_empty() {
                    let mut by_name: HashMap<String, Module> = (program.modules.into_iter())
                        .map(|m| (m.name.clone(), m))
                        .collect();
                    for m in &mut compilation.modules {
                        if let Some(rewritten) = by_name.remove(&m.ir.name) {
                            m.ir = rewritten;
                        }
                    }
                }
                problems
            }
            Err(e) => vec![format!("it does not read: {e}")],
        };
        if !problems.is_empty() {
            let more = problems.len() - 1;
            let first = problems.swap_remove(0);
            let also = if more > 0 {
                format!(", and {more} more")
            } else {
                String::new()
            };
            let message =
                format!("plugin `{package}` gave IR that does not hold together: {first}{also}");
            diagnostics.push(error(codes::P047, message));
            return;
        }
        let message = format!(
            "plugin `{package}` rewrote the IR of this build: what the program does is its plugin's to answer for, outside the checks of Flick"
        );
        diagnostics.push(Diagnostic::new(codes::P048, Severity::Note, message));
    }
}

/// The files of the build as each plugin whose package says `hooks =
/// ["emit"]` writes them instead; a `.js` it changes while the map it was
/// given stays the same or is gone is `FLICK-P046`, and the files stay as
/// they were.
pub(super) fn emit(project: &Project, emitted: &mut Emission) {
    let hooked =
        project::hooked_plugins(&FsHost::new(), &mut SourceMap::default(), project, "emit");
    let error = |code, message: String| Diagnostic::new(code, Severity::Error, message);
    for (package, component) in hooked {
        let plugin = match plugins::plugin(&package, component.as_deref()) {
            Ok(plugin) => plugin,
            Err(why) => {
                let message = format!("the plugin of package `{package}` cannot be loaded: {why}");
                emitted.diagnostics.push(error(codes::P035, message));
                continue;
            }
        };
        let given: Vec<flick_plugin::Artifact> = (emitted.artifacts.iter())
            .map(|a| flick_plugin::Artifact {
                path: a.path.clone(),
                contents: a.contents.clone(),
            })
            .collect();
        match plugin.emit(&given) {
            Ok(written) => match unmapped(&given, &written) {
                Some(path) => {
                    let message = format!("plugin `{package}` changed `{path}` without its map");
                    emitted.diagnostics.push(error(codes::P046, message));
                }
                None => {
                    emitted.artifacts = (written.into_iter())
                        .map(|a| Artifact {
                            path: a.path,
                            contents: a.contents,
                        })
                        .collect();
                }
            },
            Err(Failure::Refused(found)) => emitted.diagnostics.extend(
                (found.into_iter())
                    .map(|d| error(codes::P036, format!("plugin `{package}`: {}", d.message))),
            ),
            Err(Failure::Broken(why)) => {
                let message = format!("plugin `{package}` failed to emit: {why}");
                emitted.diagnostics.push(error(codes::P036, message));
            }
        }
    }
}

/// The first `.js` of `written` that differs from `given` while its `.map`,
/// which `given` has, stays the same or is gone: a script that lost the map
/// of its code.
fn unmapped(
    given: &[flick_plugin::Artifact],
    written: &[flick_plugin::Artifact],
) -> Option<String> {
    let text = |files: &[flick_plugin::Artifact], path: &str| {
        (files.iter())
            .find(|a| a.path == path)
            .map(|a| a.contents.clone())
    };
    (written.iter())
        .filter(|a| Path::new(&a.path).extension().is_some_and(|e| e == "js"))
        .find(|a| {
            let map = format!("{}.map", a.path);
            let old_map = text(given, &map);
            let new_map = text(written, &map);
            text(given, &a.path).as_ref() != Some(&a.contents)
                && old_map.is_some()
                && (new_map.is_none() || new_map == old_map)
        })
        .map(|a| a.path.clone())
}

#[cfg(test)]
mod tests {
    use flick_plugin::Artifact;

    use super::unmapped;

    fn file(path: &str, contents: &str) -> Artifact {
        Artifact {
            path: path.into(),
            contents: contents.into(),
        }
    }

    #[test]
    fn a_changed_script_needs_a_changed_map_when_the_build_wrote_one() {
        let given = [file("a.js", "x"), file("a.js.map", "m"), file("b.js", "y")];
        let mapped = [
            file("a.js", "x2"),
            file("a.js.map", "m2"),
            file("b.js", "y"),
        ];
        assert_eq!(unmapped(&given, &mapped), None);
        let stale = [file("a.js", "x2"), file("a.js.map", "m"), file("b.js", "y")];
        assert_eq!(unmapped(&given, &stale).as_deref(), Some("a.js"));
        let dropped = [file("a.js", "x2"), file("b.js", "y")];
        assert_eq!(unmapped(&given, &dropped).as_deref(), Some("a.js"));
        // a script the build wrote no map for may change alone
        let alone = [file("a.js", "x"), file("a.js.map", "m"), file("b.js", "y2")];
        assert_eq!(unmapped(&given, &alone), None);
    }
}
