//! Public API Markdown and package-context documentation examples.
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::doc::{self, DocComment};
use flick_core::iface::Interface;
use flick_core::plugin::expansion::Expansion;
use flick_core::project::{self, Compilation, Project};
use flick_core::source::{FileId, SourceMap, Span};
use flick_core::types::{builtin_sources, builtins, implicit_package_of};

use super::doc_run::{DocOrigin, check_doc_examples};
use super::{Checked, compile, compile_with_extra, failure, write};
use crate::args::DocArguments;
use crate::builtins as builtin_cache;
use crate::command::Outcome;
use crate::commands::check::select;
use crate::commands::run_flick::{self, PreparedTarget};
use crate::sources::FsHost;

pub fn doc(options: &DocArguments, explicit: Option<&str>) -> Outcome {
    if options.std {
        return doc_std(options);
    }
    let (mut checked, prepared) = match prepare_project_doc(options, explicit) {
        Ok(pair) => pair,
        Err(outcome) => return outcome,
    };

    if let Some(prepared) = prepared.as_ref() {
        let diagnostics = check_project_examples(&mut checked, prepared);
        if diagnostics.iter().any(|d| d.severity == Severity::Error) {
            return Outcome {
                sources: checked.sources,
                diagnostics,
            };
        }
    }

    write_project_docs(checked, options)
}

fn prepare_project_doc(
    options: &DocArguments,
    explicit: Option<&str>,
) -> Result<(Checked, Option<PreparedTarget>), Outcome> {
    if !options.check_examples {
        return Ok((compile(explicit)?, None));
    }
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let (project, _) = select(&host, &mut sources, explicit, &[]).map_err(|d| Outcome {
        sources,
        diagnostics: vec![d],
    })?;
    let prepared = run_flick::prepare_target(&host, project.as_ref(), options.target.as_deref())
        .map_err(|(err_sources, diagnostics)| Outcome {
            sources: err_sources,
            diagnostics,
        })?;
    let override_cfg = super::target_override(
        &prepared.backend,
        &prepared.choice,
        prepared.target_overridden,
        &prepared.options,
        prepared.rt,
        false,
        Vec::new(),
    );
    let checked = compile_with_extra(explicit, &[], Some(&override_cfg), true)?;
    Ok((checked, Some(prepared)))
}

fn write_project_docs(checked: Checked, options: &DocArguments) -> Outcome {
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

fn builtin_source_file(
    sources: &mut SourceMap,
    cache: &mut BTreeMap<String, FileId>,
    module_name: &str,
    text: &str,
) -> FileId {
    if let Some(&id) = cache.get(module_name) {
        return id;
    }
    let rel = project::module_path(module_name)
        .to_string_lossy()
        .replace('\\', "/");
    let package = implicit_package_of(module_name).unwrap_or("core");
    let display = format!("packages/{package}/src/{rel}");
    let id = sources.add(display, text);
    cache.insert(module_name.to_owned(), id);
    id
}

fn collect_project_origins(checked: &mut Checked) -> Vec<DocOrigin> {
    let mut origins: Vec<DocOrigin> = Vec::new();
    let mut builtin_files: BTreeMap<String, FileId> = BTreeMap::new();
    let builtins = builtin_sources();
    let mut module_comments_cache: BTreeMap<usize, BTreeMap<String, DocComment>> = BTreeMap::new();

    for module in &checked.compilation.modules {
        if module.test || module.interface.private {
            continue;
        }
        let text = module.expansion.as_ref().map_or_else(
            || checked.sources.get(module.file).text.as_str(),
            |e| e.text.as_str(),
        );
        if let Some(comment) = doc::located_module_comment(text) {
            origins.push(DocOrigin {
                owner: module.name.clone(),
                file: module.file,
                expansion: module.expansion.as_ref().map(Arc::clone),
                comment,
            });
        }
        let exports: Vec<(String, String)> = module
            .interface
            .exports
            .iter()
            .filter(|e| e.parent.is_none())
            .map(|e| (e.entity.module.clone(), e.entity.name.clone()))
            .collect();
        let owner_module = module.name.clone();
        for (entity_module, entity_name) in exports {
            if let Some(origin_idx) = checked
                .compilation
                .modules
                .iter()
                .position(|m| m.name == entity_module)
            {
                let origin_file = checked.compilation.modules[origin_idx].file;
                let origin_expansion = checked.compilation.modules[origin_idx]
                    .expansion
                    .as_ref()
                    .map(Arc::clone);
                let comments = module_comments_cache.entry(origin_idx).or_insert_with(|| {
                    let origin_mod = &checked.compilation.modules[origin_idx];
                    let text = origin_mod.expansion.as_ref().map_or_else(
                        || checked.sources.get(origin_mod.file).text.as_str(),
                        |e| e.text.as_str(),
                    );
                    doc::located_comments(text).into_iter().collect()
                });
                if let Some(comment) = comments.get(&entity_name).cloned() {
                    origins.push(DocOrigin {
                        owner: format!("{owner_module}.{entity_name}"),
                        file: origin_file,
                        expansion: origin_expansion,
                        comment,
                    });
                }
            } else if let Some((_, text)) = builtins.iter().find(|(name, _)| *name == entity_module)
                && let Some(comment) = doc::located_comments(text).remove(&entity_name)
            {
                let file = builtin_source_file(
                    &mut checked.sources,
                    &mut builtin_files,
                    &entity_module,
                    text,
                );
                origins.push(DocOrigin {
                    owner: format!("{owner_module}.{entity_name}"),
                    file,
                    expansion: None::<Arc<Expansion>>,
                    comment,
                });
            }
        }
    }
    origins
}

fn check_project_examples(checked: &mut Checked, prepared: &PreparedTarget) -> Vec<Diagnostic> {
    let origins = collect_project_origins(checked);
    for origin in &origins {
        let diagnostics = check_doc_examples(checked, prepared, origin);
        if diagnostics.iter().any(|d| d.severity == Severity::Error) {
            return diagnostics;
        }
    }
    Vec::new()
}

fn doc_std(options: &DocArguments) -> Outcome {
    let host = FsHost::new();
    let prepared = if options.check_examples {
        match run_flick::prepare_target(&host, None, options.target.as_deref()) {
            Ok(prepared) => Some(prepared),
            Err((sources, diagnostics)) => {
                return Outcome {
                    sources,
                    diagnostics,
                };
            }
        }
    } else {
        if let Err(diagnostic) = builtin_cache::prepare() {
            return Outcome {
                diagnostics: vec![diagnostic],
                ..Outcome::default()
            };
        }
        None
    };

    let sources = builtin_sources();
    let interfaces = builtins();
    if let Some(prepared) = prepared.as_ref()
        && let Some(outcome) = check_std_examples(host, prepared, interfaces, sources)
    {
        return outcome;
    }

    write_std_docs(options, interfaces, sources)
}

fn check_std_examples(
    host: FsHost,
    prepared: &PreparedTarget,
    interfaces: &[Interface],
    sources: &[(&str, &str)],
) -> Option<Outcome> {
    let mut example_context = Checked {
        host,
        sources: SourceMap::default(),
        project: None::<Project>,
        compilation: Compilation::default(),
    };
    let mut builtin_files: BTreeMap<String, FileId> = BTreeMap::new();
    for interface in interfaces.iter().filter(|i| !i.private) {
        let text = sources
            .iter()
            .find(|(name, _)| *name == interface.module)
            .map_or("", |(_, text)| *text);
        if let Some(comment) = doc::located_module_comment(text) {
            let file = builtin_source_file(
                &mut example_context.sources,
                &mut builtin_files,
                &interface.module,
                text,
            );
            let origin = DocOrigin {
                owner: interface.module.clone(),
                file,
                expansion: None,
                comment,
            };
            let diagnostics = check_doc_examples(&mut example_context, prepared, &origin);
            if diagnostics.iter().any(|d| d.severity == Severity::Error) {
                return Some(Outcome {
                    sources: example_context.sources,
                    diagnostics,
                });
            }
        }
        for export in interface.exports.iter().filter(|e| e.parent.is_none()) {
            if let Some((_, source)) = sources
                .iter()
                .find(|(name, _)| *name == export.entity.module)
                && let Some(comment) = doc::located_comments(source).remove(&export.entity.name)
            {
                let file = builtin_source_file(
                    &mut example_context.sources,
                    &mut builtin_files,
                    &export.entity.module,
                    source,
                );
                let origin = DocOrigin {
                    owner: format!("{}.{}", interface.module, export.entity.name),
                    file,
                    expansion: None,
                    comment,
                };
                let diagnostics = check_doc_examples(&mut example_context, prepared, &origin);
                if diagnostics.iter().any(|d| d.severity == Severity::Error) {
                    return Some(Outcome {
                        sources: example_context.sources,
                        diagnostics,
                    });
                }
            }
        }
    }
    None
}

fn write_std_docs(
    options: &DocArguments,
    interfaces: &[Interface],
    sources: &[(&str, &str)],
) -> Outcome {
    let out = Path::new(&options.output.out_dir);
    let mut index = String::from("# API\n\n");
    for interface in interfaces.iter().filter(|i| !i.private) {
        let text = sources
            .iter()
            .find(|(name, _)| *name == interface.module)
            .map_or("", |(_, text)| *text);
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
