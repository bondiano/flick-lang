//! Public API Markdown and package-context documentation examples.
use std::fmt::Write;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flick_core::diagnostics::{Severity, codes};
use flick_core::plugin::{Expander, reach::Outside};
use flick_core::project::{self, Compilation, Host, Project};
use flick_core::source::{SourceMap, Span};
use flick_core::types::{builtin_sources, builtins, implicit_package_of};
use flick_core::{doc, syntax};

use super::{Checked, compile, failure, write};
use crate::args::DocArguments;
use crate::builtins as builtin_cache;
use crate::command::Outcome;
use crate::sources::FsHost;

pub fn doc(options: &DocArguments, explicit: Option<&str>) -> Outcome {
    if options.std {
        return doc_std(options);
    }
    let checked = match compile(explicit) {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };
    if options.check_examples
        && let Some(message) = check_project_examples(&checked)
    {
        return failure(checked.sources, codes::P013, message);
    }
    let out = checked.host.cwd.join(&options.output.out_dir);
    let mut index = String::from("# API\n\n");
    for module in checked
        .compilation
        .modules
        .iter()
        .filter(|m| !m.test && !m.interface.private)
    {
        // the expanded text has the documentation of generated declarations
        let text =
            (module.expansion.as_ref()).map_or(&checked.sources.get(module.file).text, |e| &e.text);
        let page = doc::page_with_metadata(&module.interface, text, &|export| {
            let origin = checked
                .compilation
                .modules
                .iter()
                .find(|m| m.name == export.entity.module);
            let Some(origin) = origin else {
                let comment = builtin_sources()
                    .iter()
                    .find(|(name, _)| *name == export.entity.module)
                    .and_then(|(_, text)| doc::comments(text).remove(&export.entity.name));
                // The caller's project root is not the builtin package's repository.
                return (comment, None);
            };
            let source = checked.sources.get(origin.file);
            let expanded = origin
                .expansion
                .as_ref()
                .map_or(source.text.as_str(), |e| e.text.as_str());
            let comment = doc::comments(expanded).remove(&export.entity.name);
            let root = checked
                .project
                .as_ref()
                .map_or(checked.host.cwd.as_path(), |p| p.root.as_path());
            let link = options.source_base_url.as_ref().and_then(|base| {
                let relative = origin.path.strip_prefix(root).ok()?;
                // Expanded offsets do not necessarily refer to the original source.
                // Resolve the original declaration's range from its public interface.
                let original = origin
                    .interface
                    .exports
                    .iter()
                    .find(|e| e.entity == export.entity)?;
                let at = original.at?;
                let span = Span::new(origin.file, at[0], at[1]);
                let span = origin.expansion.as_ref().map_or(span, |e| e.place(span));
                let line = source.line_col(span.start).0 + 1;
                let path = relative
                    .to_string_lossy()
                    .replace('\\', "/")
                    .replace(' ', "%20");
                Some(format!("{}/{path}#L{line}", base.trim_end_matches('/')))
            });
            (comment, link)
        });
        let file = format!("{}.md", module.name);
        if let Err(err) = write(&out.join(&file), &page) {
            return failure(
                checked.sources,
                codes::P013,
                format!("cannot write `{file}`: {err}"),
            );
        }
        let _ = writeln!(index, "- [{}]({file})", module.name);
    }
    if let Err(err) = write(&out.join("index.md"), &index) {
        return failure(
            checked.sources,
            codes::P013,
            format!("cannot write `index.md`: {err}"),
        );
    }
    Outcome::default()
}

fn check_project_examples(checked: &Checked) -> Option<String> {
    for module in &checked.compilation.modules {
        if module.test || module.interface.private {
            continue;
        }
        let source = checked.sources.get(module.file);
        let text = module
            .expansion
            .as_ref()
            .map_or(source.text.as_str(), |e| e.text.as_str());
        if let Some(comment) = doc::module_comment(text)
            && let Some(message) = check_doc_examples(checked, &comment)
        {
            return Some(format!(
                "documentation example of `{}`: {message}",
                module.name
            ));
        }
        for export in module
            .interface
            .exports
            .iter()
            .filter(|e| e.parent.is_none())
        {
            let Some(origin) = checked
                .compilation
                .modules
                .iter()
                .find(|m| m.name == export.entity.module)
            else {
                if let Some((_, text)) = builtin_sources()
                    .iter()
                    .find(|(name, _)| *name == export.entity.module)
                    && let Some(comment) = doc::comments(text).get(&export.entity.name)
                    && let Some(message) = check_doc_examples(checked, comment)
                {
                    return Some(format!(
                        "documentation example of `{}.{}`: {message}",
                        module.name, export.entity.name
                    ));
                }
                continue;
            };
            let source = checked.sources.get(origin.file);
            let text = origin
                .expansion
                .as_ref()
                .map_or(source.text.as_str(), |e| e.text.as_str());
            if let Some(comment) = doc::comments(text).get(&export.entity.name)
                && let Some(message) = check_doc_examples(checked, comment)
            {
                return Some(format!(
                    "documentation example of `{}.{}`: {message}",
                    module.name, export.entity.name
                ));
            }
        }
    }
    None
}

fn doc_std(options: &DocArguments) -> Outcome {
    if let Err(diagnostic) = builtin_cache::prepare() {
        return Outcome {
            diagnostics: vec![diagnostic],
            ..Outcome::default()
        };
    }
    let example_context = Checked {
        host: FsHost::new(),
        sources: SourceMap::default(),
        project: None,
        compilation: Compilation::default(),
    };
    let sources = builtin_sources();
    let interfaces = builtins();
    let out = Path::new(&options.output.out_dir);
    let mut index = String::from("# API\n\n");
    for interface in interfaces.iter().filter(|i| !i.private) {
        let text = sources
            .iter()
            .find(|(name, _)| *name == interface.module)
            .map_or("", |(_, text)| *text);
        if options.check_examples {
            if let Some(comment) = doc::module_comment(text)
                && let Some(message) = check_doc_examples(&example_context, &comment)
            {
                return failure(
                    SourceMap::default(),
                    codes::P013,
                    format!("documentation example of `{}`: {message}", interface.module),
                );
            }
            for export in interface.exports.iter().filter(|e| e.parent.is_none()) {
                if let Some((_, source)) = sources
                    .iter()
                    .find(|(name, _)| *name == export.entity.module)
                    && let Some(comment) = doc::comments(source).get(&export.entity.name)
                    && let Some(message) = check_doc_examples(&example_context, comment)
                {
                    return failure(
                        SourceMap::default(),
                        codes::P013,
                        format!(
                            "documentation example of `{}.{}`: {message}",
                            interface.module, export.entity.name
                        ),
                    );
                }
            }
        }
        let page = doc::page_with_metadata(interface, text, &|export| {
            let Some((_, source)) = sources
                .iter()
                .find(|(name, _)| *name == export.entity.module)
            else {
                return (None, None);
            };
            let comment = doc::comments(source).remove(&export.entity.name);
            let link = options.source_base_url.as_ref().and_then(|base| {
                let original = interfaces
                    .iter()
                    .find(|i| i.module == export.entity.module)?
                    .exports
                    .iter()
                    .find(|e| e.entity == export.entity)?;
                let at = original.at?;
                let line = source
                    .get(..at[0] as usize)?
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count()
                    + 1;
                let path = project::module_path(&export.entity.module)
                    .to_string_lossy()
                    .replace('\\', "/");
                let package = implicit_package_of(&export.entity.module).unwrap_or("core");
                Some(format!(
                    "{}/packages/{package}/src/{path}#L{line}",
                    base.trim_end_matches('/')
                ))
            });
            (comment, link)
        });
        let file = format!("{}.md", interface.module);
        if let Err(err) = write(&out.join(&file), &page) {
            return failure(
                SourceMap::default(),
                codes::P013,
                format!("cannot write `{file}`: {err}"),
            );
        }
        let _ = writeln!(index, "- [{}]({file})", interface.module);
    }
    if let Err(err) = write(&out.join("index.md"), &index) {
        return failure(
            SourceMap::default(),
            codes::P013,
            format!("cannot write index: {err}"),
        );
    }
    Outcome::default()
}

/// An in-memory example is a source root in the real package, not a single-file
/// check that trusts its imports. No example files are written to the project.
fn check_doc_examples(checked: &Checked, comment: &str) -> Option<String> {
    use flick_core::markdown::{self, Mode};
    struct ExampleHost<'a> {
        host: &'a FsHost,
        path: PathBuf,
        text: String,
    }
    impl Host for ExampleHost<'_> {
        fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
            if path == self.path {
                Ok(self.text.as_bytes().to_vec())
            } else {
                self.host.read(path)
            }
        }
        fn display(&self, path: &Path) -> String {
            self.host.display(path)
        }
        fn plugin(
            &self,
            package: &str,
            component: Option<&Path>,
        ) -> Result<Arc<dyn Expander>, String> {
            self.host.plugin(package, component)
        }
        fn outside(&self) -> Option<Arc<dyn Outside>> {
            self.host.outside()
        }
    }
    for block in markdown::blocks(comment) {
        if block.mode == Mode::Ignore {
            let mut sources = SourceMap::default();
            let file = sources.add("documentation example", block.text.as_str());
            let parsed = syntax::parse(&block.text, file);
            if !parsed.diagnostics.is_empty() {
                return Some(format!(
                    "line {} does not parse: {:?}",
                    block.line, parsed.diagnostics
                ));
            }
            continue;
        }
        let root = checked
            .project
            .as_ref()
            .map_or_else(|| checked.host.cwd.clone(), Project::source_root);
        let declared = block.text.lines().find_map(|line| {
            line.strip_prefix("module ")
                .and_then(|s| s.split_whitespace().next())
        });
        let name = declared.unwrap_or("DocExample");
        let text = if declared.is_some() {
            block.text.clone()
        } else {
            format!("module {name} exposing (..)\n{}", block.text)
        };
        let path = root.join(project::module_path(name));
        let host = ExampleHost {
            host: &checked.host,
            path: path.clone(),
            text,
        };
        let mut sources = SourceMap::default();
        let diagnostics = project::check(&host, &mut sources, checked.project.as_ref(), &[path]);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        let valid = if block.mode == Mode::Fail {
            !errors.is_empty()
                && block.codes.iter().all(|code| {
                    errors.iter().any(|d| {
                        d.code.to_string().trim_start_matches("FLICK-")
                            == code.trim_start_matches("FLICK-")
                    })
                })
        } else {
            errors.is_empty()
        };
        if !valid {
            return Some(format!(
                "line {} did not satisfy {:?}: {errors:?}",
                block.line, block.mode
            ));
        }
    }
    None
}
