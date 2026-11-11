//! Elaboration of module expressions. Applications walk the expression tree;
//! names in module bodies are resolved through imports, never text-replaced.

use std::path::PathBuf;
use std::sync::Arc;

use super::{CalleeOrigin, Loaded, Loader, StaticArgument, StaticCapture, StaticMember, Virtual};
use crate::diagnostics::{Diagnostic, codes};
use crate::resolve::{self, Import, Item, ItemKind, ModuleApplication};
use crate::source::Span;
use crate::syntax::{self, cst::Node, kind::SyntaxKind as K};

impl Loader<'_> {
    fn expression_name(&mut self, parent: usize, desired: Option<&str>) -> String {
        if let Some(desired) = desired {
            return desired.to_owned();
        }
        loop {
            let name = format!("HomExpression{}", self.expression_serial);
            self.expression_serial += 1;
            if !self
                .by_name
                .contains_key(&format!("{}.{}", self.modules[parent].name, name))
                && !self.modules[parent]
                    .syntax
                    .applications
                    .iter()
                    .any(|a| a.name == name)
            {
                return name;
            }
        }
    }

    pub(super) fn expression_visibility(
        &self,
        parent: usize,
        desired: Option<&str>,
    ) -> &'static str {
        if self.modules[parent]
            .syntax
            .header
            .as_ref()
            .is_some_and(|h| h.private)
        {
            return "private ";
        }
        let exported = desired.is_some_and(|name| {
            self.modules[parent]
                .syntax
                .header
                .as_ref()
                .and_then(|h| h.exposing.as_ref())
                .is_some_and(|items| {
                    items
                        .iter()
                        .any(|item| item.kind == ItemKind::All || item.name == name)
                })
        });
        if exported { "" } else { "private " }
    }

    pub(super) fn select_expression_body(&mut self, module: usize, target: String, at: Span) {
        let syntax = Arc::make_mut(&mut self.modules[module].syntax);
        syntax.header.as_mut().expect("header").materialized_body = Some(target.clone());
        syntax.imports.push(Import {
            module: target,
            alias: None,
            exposing: vec![Item {
                kind: ItemKind::All,
                name: String::new(),
                span: at,
            }],
            plugins: Vec::new(),
            span: at,
        });
    }

    fn module_expression(
        &mut self,
        parent: usize,
        node: &Node,
        desired: Option<&str>,
        site: &ModuleApplication,
    ) -> Option<String> {
        match node.kind {
            K::ModulePath => {
                let name: String = node
                    .text(&self.sources.get(self.modules[parent].file).text)
                    .split_whitespace()
                    .collect();
                let target = self.module_reference(parent, &name, site.span)?;
                let origin = self.callee_origin(parent, node, &target);
                let is_parameter = matches!(&origin, CalleeOrigin::Parameter { .. });
                if desired.is_some() || is_parameter {
                    return self.alias_expression(
                        parent,
                        desired,
                        &target,
                        site.annotation.as_ref(),
                        site.span,
                        is_parameter.then_some(origin),
                    );
                }
                Some(target)
            }
            K::ModuleParenExpr => {
                self.module_expression(parent, node.nodes().next()?, desired, site)
            }
            K::ModuleApplyExpr => {
                let mut children = node.nodes();
                let callee_node = children.next()?;
                let callee = self.module_expression(parent, callee_node, None, site)?;
                let origin = self.callee_origin(parent, callee_node, &callee);
                let mut arguments = Vec::new();
                for argument in children {
                    arguments.push(self.module_expression(parent, argument, None, site)?);
                }
                let local = self.expression_name(parent, desired);
                let mut application = site.clone();
                application.name.clone_from(&local);
                application.template = callee;
                application.argument_spans = vec![site.span; arguments.len()];
                application.arguments = arguments;
                self.instantiate_named(parent, &application, origin);
                let full = format!("{}.{}", self.modules[parent].name, local);
                self.by_name.contains_key(&full).then_some(full)
            }
            K::ModuleParamExpr => self.parameterized_expression(parent, node, desired),
            K::ModuleAnnotExpr => {
                let mut children = node.nodes();
                let target = self.module_expression(parent, children.next()?, None, site)?;
                let annotation = children.next()?;
                self.alias_expression(parent, desired, &target, Some(annotation), site.span, None)
            }
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    codes::T002,
                    "expected a static module expression",
                    site.span,
                ));
                None
            }
        }
    }

    fn parameterized_expression(
        &mut self,
        parent: usize,
        node: &Node,
        desired: Option<&str>,
    ) -> Option<String> {
        let local = self.expression_name(parent, desired);
        let source = &self.sources.get(self.modules[parent].file).text;
        let groups = node
            .nodes()
            .filter(|n| n.kind == K::ModuleParamGroup)
            .map(|n| n.text(source))
            .collect::<String>();
        let body = node
            .nodes()
            .find(|n| !matches!(n.kind, K::ModuleParamGroup | K::ModuleResult))?;
        let full = format!("{}.{}", self.modules[parent].name, local);
        let (imports, inherited_imports) = self.expression_captures(parent, Some(node));
        let result = node
            .nodes()
            .find(|n| n.kind == K::ModuleResult)
            .and_then(|n| n.nodes().next())
            .map_or(String::new(), |n| format!(" : {}", n.text(source)));
        let visibility = self.expression_visibility(parent, desired);
        let exposing = if result.is_empty() {
            " exposing (..)"
        } else {
            ""
        };
        let generated = if body.kind == K::Members {
            format!(
                "{visibility}module {full}{groups}{result}{exposing}\n{imports}{}\n",
                body_source(body, source)
            )
        } else {
            format!(
                "{visibility}module {full}{groups}{result}{exposing} = {}\n{imports}",
                body.text(source)
            )
        };
        self.register_expression(
            parent,
            &local,
            generated,
            inherited_imports,
            Some(node.range().0),
        );
        Some(full)
    }

    pub(super) fn alias_expression(
        &mut self,
        parent: usize,
        desired: Option<&str>,
        target: &str,
        annotation: Option<&Node>,
        at: Span,
        origin: Option<CalleeOrigin>,
    ) -> Option<String> {
        let local = self.expression_name(parent, desired);
        let visibility = self.expression_visibility(parent, desired);
        let full = format!("{}.{}", self.modules[parent].name, local);
        if full == target {
            self.diagnostics
                .push(Diagnostic::error(codes::R006, "cyclic module alias", at));
            return None;
        }
        let source = &self.sources.get(self.modules[parent].file).text;
        let lexical_at = annotation.map(|_| at.start);
        let annotation = annotation.map_or(String::new(), |n| format!(" : {}", n.text(source)));
        let exposing = if annotation.is_empty() {
            " exposing (..)"
        } else {
            ""
        };
        let (imports, inherited_imports) = self.expression_captures(parent, None);
        let generated =
            format!("{visibility}module {full}{annotation}{exposing} = {target}\n{imports}");
        self.register_expression(parent, &local, generated, inherited_imports, lexical_at);
        self.select_expression_body(*self.by_name.get(&full)?, target.to_owned(), at);
        if let Some(context) = self.modules[*self.by_name.get(&full)?].virtual_of.as_mut() {
            context.capture.callee = origin;
        }
        Some(full)
    }

    fn register_expression(
        &mut self,
        parent: usize,
        local: &str,
        generated: String,
        inherited_imports: usize,
        lexical_at: Option<u32>,
    ) {
        let name = format!("{}.{}", self.modules[parent].name, local);
        if self.by_name.contains_key(&name) {
            return;
        }
        let scope = lexical_at.and_then(|at| self.capture_scope(parent, at));
        let file = self.sources.add(format!("<module {name}>"), generated);
        let text = &self.sources.get(file).text;
        let parse = Arc::new(syntax::parse(text, file));
        let syntax = Arc::new(resolve::module_syntax(&parse.tree, text, file));
        self.by_name.insert(name.clone(), self.modules.len());
        let inherited = self.modules[parent].virtual_of.as_ref();
        let mut capture = inherited.map_or_else(StaticCapture::default, |v| v.capture.clone());
        capture.callee = None;
        capture.arguments.clear();
        capture.inherited_imports = inherited_imports;
        if let Some(scope) = scope {
            capture.scopes.push(scope);
        }
        capture.template = None;
        if !capture.access_modules.contains(&self.modules[parent].name) {
            capture
                .access_modules
                .push(self.modules[parent].name.clone());
        }
        let virtual_of = Some(Virtual {
            at: self.modules[parent]
                .syntax
                .header
                .as_ref()
                .map_or(Span::new(file, 0, 0), |h| h.span),
            capture,
        });
        self.modules.push(Loaded {
            path: PathBuf::from(format!("{}#{local}", self.modules[parent].path.display())),
            file,
            root: self.modules[parent].root.clone(),
            parse,
            syntax,
            name,
            expansion: None,
            editing: Vec::new(),
            loader: None,
            virtual_of,
            member_of: Some(StaticMember {
                parent: self.modules[parent].name.clone(),
                name: local.to_owned(),
            }),
            scope_of: None,
        });
    }

    fn callee_origin(&self, parent: usize, expression: &Node, actual: &str) -> CalleeOrigin {
        fn path(node: &Node, source: &str) -> Option<String> {
            match node.kind {
                K::ModulePath => Some(node.text(source).split_whitespace().collect()),
                K::ModuleParenExpr => path(node.nodes().next()?, source),
                _ => None,
            }
        }
        let source = &self.sources.get(self.modules[parent].file).text;
        let mut reference = path(expression, source);
        let mut seen = Vec::new();
        while let Some(name) = reference.take() {
            if seen.contains(&name) {
                break;
            }
            seen.push(name.clone());
            let (head, tail) = name.split_once('.').unwrap_or((&name, ""));
            if let Some(context) = &self.modules[parent].virtual_of
                && let Some(binding) = context
                    .capture
                    .bindings
                    .iter()
                    .rev()
                    .find(|b| b.alias == head)
            {
                return CalleeOrigin::Parameter {
                    binding: binding.origin.clone(),
                    members: tail
                        .split('.')
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect(),
                };
            }
            reference = self.modules[parent]
                .syntax
                .applications
                .iter()
                .find(|a| a.name == head && a.annotation.is_none())
                .and_then(|a| path(&a.expression, source))
                .map(|target| {
                    if tail.is_empty() {
                        target
                    } else {
                        format!("{target}.{tail}")
                    }
                });
        }
        if let Some(member) = self
            .by_name
            .get(actual)
            .and_then(|&i| self.modules[i].member_of.as_ref())
            && !self.modules[parent].can_access_member(member)
        {
            return CalleeOrigin::Member(StaticArgument {
                module: actual.to_owned(),
                access_modules: self.modules[parent].namespace_access(),
            });
        }
        CalleeOrigin::Module(actual.to_owned())
    }

    fn expression_captures(&self, parent: usize, parameterized: Option<&Node>) -> (String, usize) {
        let loaded = &self.modules[parent];
        let source = &self.sources.get(loaded.file).text;
        let tokens: Vec<_> = parameterized
            .into_iter()
            .flat_map(Node::tokens)
            .filter(|t| !t.kind.is_trivia())
            .map(|t| t.text(source))
            .collect();
        let mut shadowed: Vec<_> = parameterized
            .into_iter()
            .flat_map(Node::nodes)
            .filter(|n| n.kind == K::ModuleParamGroup)
            .flat_map(Node::nodes)
            .filter_map(|p| p.child_tokens().find(|t| t.kind == K::Con))
            .map(|t| t.text(source).to_owned())
            .collect();
        if let Some(body) = parameterized
            .into_iter()
            .flat_map(Node::nodes)
            .find(|n| n.kind == K::Members)
        {
            shadowed.extend(
                resolve::module_syntax(body, source, loaded.file)
                    .imports
                    .into_iter()
                    .map(|i| i.qualifier().to_owned()),
            );
        }
        let mut imports = String::new();
        let mut count = 0;
        for import in &loaded.syntax.imports {
            if import.module.starts_with('$') || shadowed.iter().any(|q| q == import.qualifier()) {
                continue;
            }
            // Written imports also bring lexical instances. Inferred signature
            // and module-expression imports are dependencies only when used;
            // copying the enclosing declaration's own binding creates a cycle.
            let written = loaded.parse.tree.nodes().any(|n| {
                let (start, end) = n.range();
                n.kind == K::ImportDecl && start <= import.span.start && import.span.end <= end
            });
            if !written {
                let qualifier = import
                    .qualifier()
                    .split('.')
                    .collect::<Vec<_>>()
                    .join(" . ");
                let path: Vec<_> = qualifier.split_whitespace().collect();
                if !tokens
                    .windows(path.len())
                    .enumerate()
                    .any(|(i, ts)| ts == path && (i == 0 || tokens[i - 1] != "."))
                {
                    continue;
                }
            }
            imports.push_str("import ");
            imports.push_str(&import.module);
            if let Some(alias) = &import.alias {
                imports.push_str(" as ");
                imports.push_str(alias);
            }
            let items: Vec<_> = import
                .exposing
                .iter()
                .map(|item| match item.kind {
                    ItemKind::All => "..".to_owned(),
                    ItemKind::ConAll => format!("{}(..)", item.name),
                    _ => item.name.clone(),
                })
                .chain(
                    import
                        .plugins
                        .iter()
                        .map(|item| format!("plugin {}", item.name)),
                )
                .collect();
            if !items.is_empty() {
                imports.push_str(" exposing (");
                imports.push_str(&items.join(", "));
                imports.push(')');
            }
            imports.push('\n');
            count += 1;
        }
        (imports, count)
    }
}

fn body_source(node: &Node, source: &str) -> String {
    let (start, end) = node.range();
    let indent = source[..start as usize]
        .rsplit('\n')
        .next()
        .map_or(0, str::len);
    source[start as usize..end as usize]
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                line
            } else {
                let spaces = line.bytes().take_while(|b| *b == b' ').count().min(indent);
                &line[spaces..]
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

include!("expressions_materialize.rs");
