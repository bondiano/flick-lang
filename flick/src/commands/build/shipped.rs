//! The foreign modules of libraries a js build ships into its output: each
//! `.mjs` a module of a library binds is found beside that module there, and
//! so are the other `.mjs` and `.cjs` of the library, which it may import,
//! and the declarations beside them, which the TypeScript forms of its
//! types may name. They then resolve npm packages from the project, and a
//! packed app has them.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use flick_core::build::Artifact;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::ir::ItemKind;
use flick_core::project::{Compilation, ships};
use flick_core::syntax::ts_mark::{self, Origin};

use crate::cache::files_under;

/// The files of a shipped module's library, as output path, text and owner.
type Files<'a> = BTreeMap<String, (String, &'a str)>;

/// The foreign modules that the modules `selected` of `compilation` need, and
/// why some cannot ship: one is missing, or two libraries differ on a path.
pub(crate) fn foreign_files(
    compilation: &Compilation,
    selected: &dyn Fn(&str) -> bool,
) -> (Vec<Artifact>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut libraries: BTreeMap<&str, &Path> = BTreeMap::new();
    for module in compilation.modules.iter().filter(|m| selected(&m.name)) {
        // a file the form of a type names ships as one a binding names does
        let forms = module.interface.tags.get(ts_mark::TAG);
        let named = (forms.into_iter().flatten())
            .filter_map(|(_, parts)| ts_mark::form(parts).ok())
            .flat_map(|form| form.imports)
            .any(|import| match &import.from {
                Origin::Module(spec) => compilation.shipped_foreign(&module.name, spec).is_some(),
                _ => false,
            });
        if named && let Some((name, dir)) = compilation.library_of(&module.name) {
            libraries.insert(name, dir);
        }
        for item in &module.ir.items {
            let ItemKind::Foreign(foreign) = &item.kind else {
                continue;
            };
            let Some(spec) = foreign.binding.from.as_deref() else {
                continue;
            };
            let Some(path) = compilation.shipped_foreign(&module.name, spec) else {
                continue;
            };
            let Some((name, dir)) = compilation.library_of(&module.name) else {
                continue;
            };
            libraries.insert(name, dir);
            if !path.is_file() {
                diagnostics.push(Diagnostic::error(
                    codes::P041,
                    format!(
                        "`{}.{}` binds `{spec}`, which is not beside it in the library `{name}`",
                        module.name, foreign.name
                    ),
                    item.span,
                ));
            }
        }
    }
    let mut files = Files::new();
    for (name, dir) in libraries {
        library_files(name, dir, &mut files, &mut diagnostics);
    }
    let artifacts = files
        .into_iter()
        .map(|(path, (contents, _))| Artifact { path, contents })
        .collect();
    (artifacts, diagnostics)
}

/// Adds the `.mjs` and `.cjs` of the library `name` in `dir` to `files`. A
/// fetched library is not trusted to name a file outside itself by a link.
fn library_files<'a>(
    name: &'a str,
    dir: &Path,
    files: &mut Files<'a>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut found = Vec::new();
    files_under(dir, &mut found);
    found.retain(|p| ships(&p.to_string_lossy()));
    let inside = |path: &Path| match (fs::canonicalize(path), fs::canonicalize(dir)) {
        (Ok(path), Ok(dir)) => path.starts_with(dir),
        _ => false,
    };
    let problem = |message: String| Diagnostic::new(codes::P041, Severity::Error, message);
    for path in found {
        let relative = path.strip_prefix(dir).unwrap_or(&path);
        let output = (relative.components())
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let text = if inside(&path) {
            fs::read_to_string(&path).ok()
        } else {
            None
        };
        let Some(text) = text else {
            diagnostics.push(problem(format!(
                "`{output}` of the library `{name}` cannot ship: it is no text file inside the library"
            )));
            continue;
        };
        match files.get(&output) {
            Some((other, by)) if *other != text => {
                diagnostics.push(problem(format!(
                    "the libraries `{by}` and `{name}` both ship `{output}`, with different contents"
                )));
            }
            Some(_) => {}
            None => {
                files.insert(output, (text, name));
            }
        }
    }
}
