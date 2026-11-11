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

    fn virtual_source(
        &self,
        parent: usize,
        functor: usize,
        name: &str,
        header: &resolve::Header,
        application: &ModuleApplication,
    ) -> Option<String> {
        let source = self.sources.get(self.modules[functor].file).text.clone();
        let header_node = self.modules[functor]
            .parse
            .tree
            .nodes()
            .find(|node| matches!(node.kind, K::ModuleHeader | K::ModuleApplyDecl))?;
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
        let visibility = self.expression_visibility(parent, Some(&application.name));
        let result = header
            .result
            .as_ref()
            .map_or(String::new(), |r| format!(" : {}", r.text(&source)));
        let groups = header_node
            .nodes()
            .filter(|n| n.kind == K::ModuleParamGroup)
            .skip(application.argument_groups.len())
            .map(|n| n.text(&source))
            .collect::<String>();
        let expression_body = header
            .body
            .as_ref()
            .map_or(String::new(), |body| format!(" = {}", body.text(&source)));
        let mut generated =
            format!("{visibility}module {name}{groups}{result}{exposed}{expression_body}\n");
        for (parameter, argument) in header.parameters.iter().zip(&application.arguments) {
            generated.push_str("import ");
            generated.push_str(argument);
            generated.push_str(" as ");
            generated.push_str(&parameter.name);
            generated.push('\n');
        }
        let suffix = &source[header_node.range().1 as usize..];
        if header_node.has_token(K::Eq) && header.body.is_none() {
            let indent = self.modules[functor]
                .parse
                .tree
                .nodes()
                .find(|node| node.range().0 > header_node.range().1)
                .map_or(0, |node| {
                    source[..node.range().0 as usize]
                        .rsplit('\n')
                        .next()
                        .map_or(0, str::len)
                });
            for line in suffix.split_inclusive('\n') {
                let spaces = line.bytes().take_while(|b| *b == b' ').count().min(indent);
                generated.push_str(&line[spaces..]);
            }
        } else {
            generated.push_str(suffix);
        }
        Some(generated)
    }

    pub(super) fn instantiate_named(
        &mut self,
        parent: usize,
        application: &ModuleApplication,
        callee: CalleeOrigin,
    ) {
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
        let Some(functor) =
            self.find_module(parent, &application.functor, application.functor_span)
        else {
            return;
        };
        let Some(functor) = self.callable_template(functor, application.span) else {
            return;
        };
        let Some(header) = self.modules[functor].syntax.header.clone() else {
            return;
        };
        if header.parameters.is_empty()
            || application.argument_groups.is_empty()
            || !header
                .parameter_groups
                .starts_with(&application.argument_groups)
            || application.arguments.len() != application.argument_groups.iter().sum::<usize>()
        {
            self.diagnostics.push(Diagnostic::error(
                codes::T002,
                format!(
                    "`{}` expects {} static module argument(s), found {}",
                    application.functor,
                    header.parameters.len(),
                    application.arguments.len()
                ),
                application.span,
            ));
            return;
        }
        let Some((template, trace)) = self.expansion_trace(parent, functor, application.span)
        else {
            return;
        };
        let mut normalized = application.clone();
        for (argument, span) in normalized
            .arguments
            .iter_mut()
            .zip(&application.argument_spans)
        {
            let Some(name) = self.module_reference(parent, argument, *span) else {
                return;
            };
            *argument = name;
        }
        let application = &normalized;
        let Some(generated) = self.virtual_source(parent, functor, &name, &header, application)
        else {
            return;
        };
        let file = self.sources.add(format!("<functor {name}>"), generated);
        let generated = &self.sources.get(file).text;
        let parse = Arc::new(syntax::parse(generated, file));
        let syntax = Arc::new(resolve::module_syntax(&parse.tree, generated, file));
        let index = self.modules.len();
        self.by_name.insert(name.clone(), index);
        let context =
            self.application_context(functor, &header, application, callee, template, trace);
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
            virtual_of: Some(context),
            member_of: Some(StaticMember {
                parent: self.modules[parent].name.clone(),
                name: application.name.clone(),
            }),
            scope_of: None,
        });
    }

    fn application_context(
        &self,
        functor: usize,
        header: &resolve::Header,
        application: &ModuleApplication,
        callee: CalleeOrigin,
        template: String,
        trace: Vec<String>,
    ) -> Virtual {
        let inherited = self.modules[functor].virtual_of.as_ref();
        let mut capture = inherited.map_or_else(StaticCapture::default, |v| v.capture.clone());
        if !capture.access_modules.contains(&self.modules[functor].name) {
            capture
                .access_modules
                .push(self.modules[functor].name.clone());
        }
        for (index, (parameter, argument)) in header
            .parameters
            .iter()
            .zip(&application.arguments)
            .enumerate()
        {
            capture.bindings.push(StaticBinding {
                alias: parameter.name.clone(),
                argument: argument.clone(),
                origin: BindingOrigin {
                    module: self.modules[functor].name.clone(),
                    parameter: index,
                    group: header
                        .parameter_groups
                        .iter()
                        .scan(0, |start, size| {
                            *start += size;
                            Some(*start)
                        })
                        .position(|end| end > index)
                        .unwrap_or(0),
                    prefix: application.arguments[..index].to_vec(),
                },
            });
        }
        if let Root::Package(package) = &self.modules[functor].root
            && !capture.access_packages.contains(package)
        {
            capture.access_packages.push(package.clone());
        }
        capture.definition_package = self.modules[functor]
            .definition_package()
            .map(str::to_owned);
        capture.callee = Some(callee);
        capture.inherited_imports += application.arguments.len();
        capture.arguments.clone_from(&application.arguments);
        capture.template = Some(template);
        capture.trace = trace;
        Virtual {
            at: application.span,
            capture,
        }
    }

    fn expansion_trace(
        &mut self,
        parent: usize,
        functor: usize,
        at: Span,
    ) -> Option<(String, Vec<String>)> {
        let context = self.modules[functor].virtual_of.as_ref();
        let template = context
            .and_then(|v| v.capture.template.clone())
            .unwrap_or_else(|| self.modules[functor].name.clone());
        let mut trace = self.modules[parent]
            .virtual_of
            .as_ref()
            .map_or_else(Vec::new, |v| v.capture.trace.clone());
        if let Some(context) = context {
            let mut captured = context.capture.trace.as_slice();
            // A partial application resumes its own last expansion. Earlier
            // captured expansions remain active in its lexical environment.
            if context.capture.template.is_some() && captured.last() == Some(&template) {
                captured = &captured[..captured.len() - 1];
            }
            trace.extend_from_slice(captured);
        }
        if trace.contains(&template) {
            trace.push(template);
            self.diagnostics.push(Diagnostic::error(
                codes::R006,
                format!("recursive static module expansion: {}", trace.join(" -> ")),
                at,
            ));
            return None;
        }
        trace.push(template.clone());
        Some((template, trace))
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
