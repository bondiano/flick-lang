//! Elaboration of module expressions. Applications walk the expression tree;
//! names in module bodies are resolved through imports, never text-replaced.

use std::path::PathBuf;
use std::sync::Arc;

use super::{CalleeOrigin, Loaded, Loader, StaticCapture, StaticMember, Virtual};
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
        if let Some(annotation) = &application.annotation {
            // Ascription follows evaluation of the underlying expression. In
            // particular, a parameter alias must first acquire its restricted
            // parameter view before its own result annotation is checked.
            let mut unannotated = application.clone();
            unannotated.annotation = None;
            if let Some(target) =
                self.module_expression(parent, &application.expression, None, &unannotated)
            {
                self.alias_expression(
                    parent,
                    Some(&application.name),
                    &target,
                    Some(annotation),
                    application.span,
                    None,
                );
            }
        } else {
            self.module_expression(
                parent,
                &application.expression,
                Some(&application.name),
                application,
            );
        }
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
            name: String::new(),
            template: String::new(),
            template_span: header.span,
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
        self.select_expression_body(module, target, header.span);
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

    pub(super) fn callable_template(
        &mut self,
        mut parameterized: usize,
        at: Span,
    ) -> Option<usize> {
        let mut aliases = Vec::new();
        loop {
            if aliases.contains(&parameterized) {
                self.diagnostics.push(Diagnostic::error(
                    codes::R006,
                    "cyclic parameterized module alias",
                    at,
                ));
                return None;
            }
            aliases.push(parameterized);
            self.materialize_expression_body(parameterized);
            let Some(target) = self.modules[parameterized]
                .syntax
                .header
                .as_ref()
                .and_then(|h| h.materialized_body.clone())
            else {
                break;
            };
            let actual = self.find_module(parameterized, &target, at)?;
            parameterized = actual;
        }
        Some(parameterized)
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
        let index = *self.by_name.get(&full)?;
        self.select_expression_body(index, target.to_owned(), at);
        if let Some(context) = self.modules[index].virtual_of.as_mut() {
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
}

include!("expression_captures.rs");
