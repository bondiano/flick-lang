//! What plugins do with a build besides blocks: `emit` gets the files
//! `flick build` writes and gives what it writes instead, a plugin after
//! another in the order of the dependencies.

use std::path::Path;

use flick_core::build::Artifact;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::plugin::Failure;
use flick_core::project::{self, Project};
use flick_core::source::SourceMap;

use super::Emission;
use crate::plugins;
use crate::sources::FsHost;

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
