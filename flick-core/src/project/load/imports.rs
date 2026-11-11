//! Following a module's imports: loading the modules they name, from the
//! project's roots and its source packages, and the cycles among them.

use std::path::PathBuf;
use std::sync::Arc;

use super::{
    BindingOrigin, CalleeOrigin, Loaded, Loader, Root, StaticBinding, StaticCapture, StaticMember,
    Virtual, module_path,
};
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
            self.materialize_expression_body(i);
            self.scope_imports(i);
            let mut targets = Vec::new();
            for import in self.modules[i]
                .syntax
                .imports
                .iter()
                .chain(&self.modules[i].syntax.inline_imports)
                .cloned()
                .collect::<Vec<_>>()
            {
                if import.module.starts_with("$module:") {
                    continue;
                }
                if self.modules[i].syntax.header.as_ref().is_some_and(|h| {
                    !h.parameters.is_empty()
                        && self.modules[i].syntax.applications.iter().any(|a| {
                            a.expression.kind != K::ModuleFunctorExpr
                                && import.module == format!("{}.{}", h.name, a.name)
                        })
                }) {
                    continue;
                }
                if let Some(target) = self.find_module(i, &import.module, import.span) {
                    targets.push(target);
                }
            }
            if let Some(virtual_of) = self.modules[i].virtual_of.clone() {
                let mut dependencies: Vec<_> = virtual_of
                    .capture
                    .bindings
                    .iter()
                    .map(|b| b.origin.module.clone())
                    .collect();
                dependencies.extend(
                    virtual_of
                        .capture
                        .bindings
                        .iter()
                        .map(|b| b.argument.clone()),
                );
                match &virtual_of.capture.callee {
                    Some(CalleeOrigin::Module(name)) => dependencies.push(name.clone()),
                    Some(CalleeOrigin::Member(member)) => dependencies.push(member.parent.clone()),
                    Some(CalleeOrigin::Parameter { binding, .. }) => {
                        dependencies.push(binding.module.clone());
                    }
                    None => {}
                }
                for dependency in dependencies {
                    if let Some(target) = self.find_module(i, &dependency, virtual_of.at) {
                        targets.push(target);
                    }
                }
            }
            if self.modules[i].scope_of.is_none() {
                for application in self.modules[i].syntax.applications.clone() {
                    let symbolic = self.modules[i]
                        .syntax
                        .header
                        .as_ref()
                        .is_some_and(|h| !h.parameters.is_empty());
                    if !symbolic || application.expression.kind == K::ModuleFunctorExpr {
                        self.instantiate(i, &application);
                    }
                }
            }
            targets.sort_by(|a, b| self.modules[*a].name.cmp(&self.modules[*b].name));
            targets.dedup();
            edges.push(targets);
            i += 1;
        }
        for (i, targets) in edges.iter_mut().enumerate() {
            for import in self.modules[i]
                .syntax
                .imports
                .iter()
                .chain(&self.modules[i].syntax.inline_imports)
            {
                if let Some(&j) = self.by_name.get(&import.module)
                    && let Some(member) = &self.modules[j].member_of
                    && !self.modules[i].can_access_member(member)
                    && let Some(&parent) = self.by_name.get(&member.parent)
                {
                    targets.push(parent);
                }
            }
            targets.sort_unstable();
            targets.dedup();
        }
        edges
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn find_module(&mut self, importer: usize, name: &str, span: Span) -> Option<usize> {
        if let Some(expanded) = self.imported_module(importer, name) {
            return self.find_module(importer, &expanded, span);
        }
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
                #[cfg(feature = "native-project")]
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
            && let Some((parent_name, _)) = name.rsplit_once('.')
        {
            let mut prefix = parent_name;
            while !self.by_name.contains_key(prefix) {
                for dir in &dirs {
                    let _ = self.load(&dir.join(module_path(prefix)));
                }
                if self.by_name.contains_key(prefix) {
                    break;
                }
                let Some((parent, _)) = prefix.rsplit_once('.') else {
                    break;
                };
                prefix = parent;
            }
            self.materialize_members(name);
            if !self.by_name.contains_key(name) && self.report_inline_import(&dirs, name, span) {
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
            .is_some_and(|r| self.modules[importer].can_access(r, name));
        let member = local
            .and_then(|j| self.modules[j].member_of.clone())
            .or_else(|| {
                self.from_packages
                    .get(name)
                    .and_then(|i| i.member_of.clone())
            });
        let through_parent = member
            .as_ref()
            .is_some_and(|m| !self.modules[importer].can_access_member(m));
        if let Some(member) = member.filter(|_| through_parent) {
            self.find_module(importer, &member.parent, span);
        }
        if private && !same_package && !through_parent {
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

    /// Resolve static members from the closest loaded ancestor outward. An
    /// importer can request `Result.Make` before `Result` itself was visited.
    fn materialize_members(&mut self, name: &str) {
        let mut pending = Vec::new();
        let mut prefix = name;
        while !self.by_name.contains_key(prefix) {
            pending.push(prefix);
            let Some((parent, _)) = prefix.rsplit_once('.') else {
                return;
            };
            prefix = parent;
        }
        for name in pending.into_iter().rev() {
            let (parent, child) = name.rsplit_once('.').expect("member path");
            let Some(&parent) = self.by_name.get(parent) else {
                return;
            };
            let Some(application) = self.modules[parent]
                .syntax
                .applications
                .iter()
                .find(|app| app.name == child)
                .cloned()
            else {
                return;
            };
            // Availability is checked through the parent's advertised view.
            self.instantiate(parent, &application);
        }
    }

    fn imported_module(&self, importer: usize, name: &str) -> Option<String> {
        let (head, tail) = name.split_once('.').unwrap_or((name, ""));
        let import = self.modules[importer]
            .syntax
            .imports
            .iter()
            .find(|i| i.alias.as_deref() == Some(head) && i.module != head)?;
        Some(if tail.is_empty() {
            import.module.clone()
        } else {
            format!("{}.{tail}", import.module)
        })
    }

    /// A module value may be a source node or a checked external structure.
    /// External/built-in arguments retain their canonical identity without a
    /// synthetic source module or a duplicate runtime definition.
    pub(super) fn module_reference(
        &mut self,
        importer: usize,
        name: &str,
        span: Span,
    ) -> Option<String> {
        if let Some(expanded) = self.imported_module(importer, name) {
            return self.module_reference(importer, &expanded, span);
        }
        self.find_module(importer, name, span)
            .map(|index| self.modules[index].name.clone())
            .or_else(|| self.from_packages.get(name).map(|i| i.module.clone()))
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
                .map(|i| i.span)
                .or_else(|| module.virtual_of.as_ref().map(|v| v.at));
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

include!("imports_application.rs");
