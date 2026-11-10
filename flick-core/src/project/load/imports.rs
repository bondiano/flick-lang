//! Following a module's imports: loading the modules they name, from the
//! project's roots and its source packages, and the cycles among them.

use std::path::PathBuf;
use std::sync::Arc;

use super::{Loaded, Loader, Root, Virtual, module_path};
use crate::diagnostics::{Diagnostic, codes};
use crate::resolve::{self, ItemKind, ModuleApplication};
use crate::source::Span;
use crate::syntax::{self, kind::SyntaxKind as K};

impl Loader<'_> {
    /// Loads imported modules until the set is closed; returns the local
    /// import edges of each module, sorted by module name.
    pub(super) fn load_imports(&mut self) -> Vec<Vec<usize>> {
        let mut edges = Vec::new();
        let mut i = 0;
        while i < self.modules.len() {
            let mut targets = Vec::new();
            for import in self.modules[i]
                .syntax
                .imports
                .iter()
                .chain(&self.modules[i].syntax.inline_imports)
                .cloned()
                .collect::<Vec<_>>()
            {
                if let Some(target) = self.find_module(i, &import.module, import.span) {
                    targets.push(target);
                }
            }
            if let Some(virtual_of) = self.modules[i].virtual_of.clone() {
                for (_, contract, _) in virtual_of.parameters {
                    if let Some((module, _)) = contract.rsplit_once('.')
                        && let Some(target) = self.find_module(i, module, virtual_of.at)
                    {
                        targets.push(target);
                    }
                }
            }
            for application in self.modules[i].syntax.applications.clone() {
                self.instantiate(i, &application);
            }
            targets.sort_by(|a, b| self.modules[*a].name.cmp(&self.modules[*b].name));
            targets.dedup();
            edges.push(targets);
            i += 1;
        }
        edges
    }

    fn find_module(&mut self, importer: usize, name: &str, span: Span) -> Option<usize> {
        let packages = self.package_sources().into_iter().map(|(_, dir)| dir);
        // the project's own modules come before those of source packages
        let dirs: Vec<PathBuf> = match (&self.modules[importer].root, self.project) {
            (Root::Source, Some(p)) => [p.source_root()].into_iter().chain(packages).collect(),
            (Root::Test, Some(p)) => [p.source_root(), p.test_root()]
                .into_iter()
                .chain(packages)
                .collect(),
            (Root::Package(_), _) => packages.collect(),
            (Root::Other(dir), _) => dir.iter().cloned().collect(),
            _ => Vec::new(),
        };
        if !self.by_name.contains_key(name) {
            for dir in &dirs {
                if self.use_saved(dir, name) {
                    continue;
                }
                // absent files are simply not candidates
                let _ = self.load(&dir.join(module_path(name)));
                for path in self.loaded_paths(dir, name) {
                    let _ = self.load(&path);
                }
            }
        }
        if !self.by_name.contains_key(name)
            && let Some((parent_name, child)) = name.rsplit_once('.')
        {
            for dir in &dirs {
                let _ = self.load(&dir.join(module_path(parent_name)));
            }
            if let Some(&parent) = self.by_name.get(parent_name)
                && let Some(application) = self.modules[parent]
                    .syntax
                    .applications
                    .iter()
                    .find(|app| app.name == child)
                    .cloned()
            {
                let exposed = self.modules[parent]
                    .syntax
                    .header
                    .as_ref()
                    .and_then(|header| header.exposing.as_ref())
                    .is_some_and(|items| {
                        items
                            .iter()
                            .any(|item| item.kind == ItemKind::All || item.name == child)
                    });
                if exposed {
                    self.instantiate(parent, &application);
                }
            }
            if self.report_inline_import(&dirs, name, span) {
                return None;
            }
        }
        let local = self.by_name.get(name).copied();
        if local.is_none() && !self.from_packages.contains_key(name) {
            match self.builtins().iter().find(|i| i.module == name) {
                Some(builtin) => {
                    self.from_packages.insert(name.to_owned(), builtin.clone());
                }
                None => self.diagnostics.push(self.unknown_module(name, span)),
            }
        }
        let private = local
            .and_then(|j| self.modules[j].syntax.header.as_ref().map(|h| h.private))
            .or_else(|| self.from_packages.get(name).map(|i| i.private))
            .unwrap_or(false);
        let owner = local.map(|j| self.modules[j].root.clone()).or_else(|| {
            self.precompiled
                .package_of(name)
                .map(|p| Root::Package(p.to_owned()))
        });
        let same_package = owner
            .as_ref()
            .is_some_and(|r| self.modules[importer].root.same_package(r));
        if private && !same_package {
            self.diagnostics.push(Diagnostic::error(
                codes::R017,
                format!(
                    "module `{name}` is private to its package; import its public facade instead"
                ),
                span,
            ));
            return None;
        }
        local
    }

    /// An import may descend through several inline namespaces, but the first
    /// one already makes the whole path private to its source file.
    fn report_inline_import(&mut self, dirs: &[PathBuf], name: &str, span: Span) -> bool {
        let mut candidate = name;
        while let Some((file_module, suffix)) = candidate.rsplit_once('.') {
            for dir in dirs {
                let _ = self.load(&dir.join(module_path(file_module)));
            }
            if let Some(&parent) = self.by_name.get(file_module) {
                let child = suffix.split('.').next().unwrap_or(suffix);
                let source = self.sources.get(self.modules[parent].file);
                if let Some(decl) = self.modules[parent]
                    .parse
                    .tree
                    .nodes()
                    .filter(|n| n.kind == K::InlineModuleDecl)
                    .filter_map(|n| n.nodes().find(|n| n.kind == K::ModulePath))
                    .find(|path| {
                        path.text(&source.text)
                            .split_whitespace()
                            .collect::<String>()
                            == child
                    })
                {
                    let (start, end) = decl.range();
                    self.diagnostics.push(Diagnostic::error(
                        codes::R018,
                        format!("inline module `{name}` is private to its source file; use its members inside `{file_module}`, or move it to a separate module file"),
                        span,
                    ).with_related(Span::new(self.modules[parent].file, start, end), "inline module declared here"));
                    return true;
                }
                break;
            }
            candidate = file_module;
        }
        false
    }

    fn virtual_source(
        &self,
        parent: usize,
        template: usize,
        name: &str,
        header: &resolve::Header,
        application: &ModuleApplication,
    ) -> Option<String> {
        let source = self.sources.get(self.modules[template].file).text.clone();
        let header_node = self.modules[template]
            .parse
            .tree
            .nodes()
            .find(|node| node.kind == K::ModuleHeader)?;
        let exposed = header
            .exposing
            .as_ref()
            .map(|items| {
                let list = items
                    .iter()
                    .map(|item| match item.kind {
                        ItemKind::All => "..".to_owned(),
                        ItemKind::ConAll => format!("{}(..)", item.name),
                        _ => item.name.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(" exposing ({list})")
            })
            .unwrap_or_default();
        let visibility = if self.modules[parent]
            .syntax
            .header
            .as_ref()
            .is_some_and(|header| header.private)
        {
            "private "
        } else {
            ""
        };
        let mut generated = format!("{visibility}module {name}{exposed}\n");
        for (parameter, argument) in header.parameters.iter().zip(&application.arguments) {
            generated.push_str("import ");
            generated.push_str(argument);
            generated.push_str(" as ");
            generated.push_str(&parameter.name);
            generated.push('\n');
        }
        generated.push_str(&source[header_node.range().1 as usize..]);
        Some(generated)
    }

    fn instantiate(&mut self, parent: usize, application: &ModuleApplication) {
        let name = format!("{}.{}", self.modules[parent].name, application.name);
        let path = PathBuf::from(format!(
            "{}#{}",
            self.modules[parent].path.display(),
            application.name
        ));
        if let Some(&existing) = self.by_name.get(&name) {
            if self.modules[existing].path != path {
                self.diagnostics.push(Diagnostic::error(
                    codes::R002,
                    format!(
                        "virtual module `{name}` conflicts with `{}`",
                        self.host.display(&self.modules[existing].path)
                    ),
                    application.span,
                ));
            }
            return;
        }
        let Some(template) =
            self.find_module(parent, &application.template, application.template_span)
        else {
            return;
        };
        let Some(header) = self.modules[template].syntax.header.clone() else {
            return;
        };
        if header.parameters.is_empty() || header.parameters.len() != application.arguments.len() {
            self.diagnostics.push(Diagnostic::error(
                codes::T002,
                format!(
                    "`{}` expects {} static module argument(s), found {}",
                    application.template,
                    header.parameters.len(),
                    application.arguments.len()
                ),
                application.span,
            ));
            return;
        }
        for (argument, span) in application
            .arguments
            .iter()
            .zip(&application.argument_spans)
        {
            if self.find_module(parent, argument, *span).is_none() {
                return;
            }
        }
        let Some(generated) = self.virtual_source(parent, template, &name, &header, application)
        else {
            return;
        };
        let file = self
            .sources
            .add(format!("<parameterized module {name}>"), generated);
        let generated = &self.sources.get(file).text;
        let parse = Arc::new(syntax::parse(generated, file));
        let syntax = Arc::new(resolve::module_syntax(&parse.tree, generated, file));
        let index = self.modules.len();
        self.by_name.insert(name.clone(), index);
        self.modules.push(Loaded {
            path,
            file,
            root: self.modules[parent].root.clone(),
            parse,
            syntax,
            name,
            expansion: None,
            editing: Vec::new(),
            loader: None,
            virtual_of: Some(Virtual {
                at: application.span,
                parameters: header
                    .parameters
                    .iter()
                    .zip(&application.arguments)
                    .map(|(parameter, argument)| {
                        (
                            parameter.name.clone(),
                            parameter.contract.clone(),
                            argument.clone(),
                        )
                    })
                    .collect(),
            }),
        });
    }

    pub(super) fn import_cycles(&mut self, edges: &[Vec<usize>]) {
        let mut order: Vec<usize> = (0..self.modules.len()).collect();
        order.sort_by(|a, b| self.modules[*a].name.cmp(&self.modules[*b].name));
        let mut reported = vec![false; self.modules.len()];
        for start in order {
            if reported[start] {
                continue;
            }
            let Some(cycle) = resolve::shortest_cycle(start, edges) else {
                continue;
            };
            for &i in &cycle {
                reported[i] = true;
            }
            let next = cycle.get(1).copied().unwrap_or(start);
            let chain: Vec<&str> = cycle
                .iter()
                .chain([&start])
                .map(|&i| self.modules[i].name.as_str())
                .collect();
            let module = &self.modules[start];
            let span = module
                .syntax
                .imports
                .iter()
                .find(|i| i.module == self.modules[next].name)
                .map(|i| i.span);
            self.diagnostics.push(
                Diagnostic::error(
                    codes::R006,
                    format!("import cycle: {}", chain.join(" -> ")),
                    span.expect("edge has an import"),
                )
                .with_fix(|| {
                    (
                        "move what the modules share into a module that both import",
                        Vec::new(),
                    )
                }),
            );
        }
    }
}
