//! Elaboration of module expressions. Applications walk the expression tree;
//! names in module bodies are resolved through imports, never text-replaced.

use std::path::PathBuf;
use std::sync::Arc;

use super::{CalleeOrigin, Loaded, Loader, Virtual};
use crate::diagnostics::{Diagnostic, codes};
use crate::resolve::{self, Import, Item, ItemKind, ModuleApplication};
use crate::source::Span;
use crate::syntax::{self, cst::Node, kind::SyntaxKind as K};

impl Loader<'_> {
    pub(super) fn instantiate(&mut self, parent: usize, application: &ModuleApplication) {
        let name = format!("{}.{}", self.modules[parent].name, application.name);
        if let Some(&existing) = self.by_name.get(&name) {
            let expected = PathBuf::from(format!(
                "{}#{}",
                self.modules[parent].path.display(),
                application.name
            ));
            if self.modules[existing].path != expected {
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
        if !self.expressions_active.insert(name.clone()) {
            self.diagnostics.push(Diagnostic::error(
                codes::R006,
                "cyclic module expression",
                application.span,
            ));
            return;
        }
        self.module_expression(
            parent,
            &application.expression,
            Some(&application.name),
            application,
        );
        self.expressions_active.remove(&name);
    }

    pub(super) fn materialize_expression_body(&mut self, module: usize) {
        let Some(header) = self.modules[module].syntax.header.clone() else {
            return;
        };
        if !header.parameters.is_empty() || header.materialized_body.is_some() {
            return;
        }
        let Some(body) = header.body else {
            return;
        };
        // Mark entry before resolving the expression, so a self-reference is
        // diagnosed rather than recursively materialized.
        Arc::make_mut(&mut self.modules[module].syntax)
            .header
            .as_mut()
            .expect("header")
            .materialized_body = Some(self.modules[module].name.clone());
        let application = ModuleApplication {
            expression: body.clone(),
            annotation: None,
            argument_groups: Vec::new(),
            name: String::new(),
            functor: String::new(),
            functor_span: header.span,
            arguments: Vec::new(),
            argument_spans: Vec::new(),
            span: header.span,
        };
        let Some(target) = self.module_expression(module, &body, None, &application) else {
            return;
        };
        if target == self.modules[module].name {
            self.diagnostics.push(Diagnostic::error(
                codes::R006,
                "cyclic module expression",
                header.span,
            ));
            return;
        }
        let syntax = Arc::make_mut(&mut self.modules[module].syntax);
        syntax.header.as_mut().expect("header").materialized_body = Some(target.clone());
        syntax.imports.push(Import {
            module: target,
            alias: None,
            exposing: vec![Item {
                kind: ItemKind::All,
                name: String::new(),
                span: header.span,
            }],
            plugins: Vec::new(),
            span: header.span,
        });
    }

    pub(super) fn callable_template(&mut self, mut functor: usize, at: Span) -> Option<usize> {
        let mut aliases = Vec::new();
        loop {
            if aliases.contains(&functor) {
                self.diagnostics
                    .push(Diagnostic::error(codes::R006, "cyclic functor alias", at));
                return None;
            }
            aliases.push(functor);
            self.materialize_expression_body(functor);
            let Some(target) = self.modules[functor]
                .syntax
                .header
                .as_ref()
                .and_then(|h| h.materialized_body.clone())
            else {
                break;
            };
            let actual = self.find_module(functor, &target, at)?;
            functor = actual;
        }
        Some(functor)
    }

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
                let index = self.find_module(parent, &name, site.span)?;
                let target = self.modules[index].name.clone();
                if let Some(local) = desired {
                    return self.alias_expression(
                        parent,
                        local,
                        &target,
                        site.annotation.as_ref(),
                        site.span,
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
                application.functor = callee;
                application.argument_groups = vec![arguments.len()];
                application.argument_spans = vec![site.span; arguments.len()];
                application.arguments = arguments;
                self.instantiate_named(parent, &application, origin);
                let full = format!("{}.{}", self.modules[parent].name, local);
                self.by_name.contains_key(&full).then_some(full)
            }
            K::ModuleFunctorExpr => {
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
                let imports = self.expression_captures(parent, body);
                let result = node
                    .nodes()
                    .find(|n| n.kind == K::ModuleResult)
                    .and_then(|n| n.nodes().next())
                    .map_or(String::new(), |n| format!(" : {}", n.text(source)));
                let generated = if body.kind == K::Members {
                    format!(
                        "module {full}{groups}{result} exposing (..)\n{imports}{}\n",
                        body_source(body, source)
                    )
                } else {
                    format!(
                        "module {full}{groups}{result} exposing (..) = {}\n{imports}",
                        body.text(source)
                    )
                };
                self.register_expression(parent, &local, generated);
                Some(full)
            }
            K::ModuleAnnotExpr => {
                let mut children = node.nodes();
                let target = self.module_expression(parent, children.next()?, None, site)?;
                let annotation = children.next()?;
                let local = self.expression_name(parent, desired);
                self.alias_expression(parent, &local, &target, Some(annotation), site.span)
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

    fn alias_expression(
        &mut self,
        parent: usize,
        local: &str,
        target: &str,
        annotation: Option<&Node>,
        at: Span,
    ) -> Option<String> {
        let full = format!("{}.{}", self.modules[parent].name, local);
        if full == target {
            self.diagnostics
                .push(Diagnostic::error(codes::R006, "cyclic module alias", at));
            return None;
        }
        let source = &self.sources.get(self.modules[parent].file).text;
        let annotation = annotation.map_or(String::new(), |n| format!(" : {}", n.text(source)));
        let generated = format!("module {full}{annotation} exposing (..) = {target}\n");
        self.register_expression(parent, local, generated);
        Some(full)
    }

    fn register_expression(&mut self, parent: usize, local: &str, generated: String) {
        let name = format!("{}.{}", self.modules[parent].name, local);
        if self.by_name.contains_key(&name) {
            return;
        }
        let file = self.sources.add(format!("<module {name}>"), generated);
        let text = &self.sources.get(file).text;
        let parse = Arc::new(syntax::parse(text, file));
        let syntax = Arc::new(resolve::module_syntax(&parse.tree, text, file));
        self.by_name.insert(name.clone(), self.modules.len());
        let inherited = self.modules[parent].virtual_of.as_ref();
        let virtual_of = Some(Virtual {
            at: self.modules[parent]
                .syntax
                .header
                .as_ref()
                .map_or(Span::new(file, 0, 0), |h| h.span),
            parameters: inherited.map_or_else(Vec::new, |v| v.parameters.clone()),
            bindings: inherited.map_or_else(Vec::new, |v| v.bindings.clone()),
            callee: None,
            arguments: Vec::new(),
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
                && let Some(index) = context
                    .parameters
                    .iter()
                    .rposition(|(alias, _, _)| alias == head)
                && let Some(binding) = context.bindings.get(index)
            {
                return CalleeOrigin::Parameter {
                    binding: binding.clone(),
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
                .find(|a| a.name == head)
                .and_then(|a| path(&a.expression, source))
                .map(|target| {
                    if tail.is_empty() {
                        target
                    } else {
                        format!("{target}.{tail}")
                    }
                });
        }
        CalleeOrigin::Module(actual.to_owned())
    }

    fn expression_captures(&self, parent: usize, expression: &Node) -> String {
        let source = &self.sources.get(self.modules[parent].file).text;
        let tokens: Vec<_> = expression
            .tokens()
            .into_iter()
            .filter(|t| !t.kind.is_trivia())
            .map(|t| t.text(source))
            .collect();
        let mut imports = String::new();
        for import in &self.modules[parent].syntax.imports {
            if import.module.starts_with('$') {
                continue;
            }
            let qualifier = import
                .qualifier()
                .split('.')
                .collect::<Vec<_>>()
                .join(" . ");
            let parts: Vec<_> = qualifier.split_whitespace().collect();
            if tokens.windows(parts.len()).any(|window| window == parts) {
                imports.push_str("import ");
                imports.push_str(&import.module);
                if let Some(alias) = &import.alias {
                    imports.push_str(" as ");
                    imports.push_str(alias);
                }
                if !import.exposing.is_empty() {
                    imports.push_str(" exposing (..)");
                }
                imports.push('\n');
            }
        }
        imports
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
