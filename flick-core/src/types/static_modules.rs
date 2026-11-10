//! Static parameter environments and result abstraction for source modules.
use super::{Checker, Form, Kind, ModuleSignature, TypeDef, module_local_dictionaries};
use crate::diagnostics::codes;
use crate::iface::Interface;
use crate::resolve::{self, FunctorParam, Header, Import, Item};
use crate::resolve::{ItemKind, Module};
use crate::source::Span;
use crate::syntax::{cst::Node, kind::SyntaxKind as K};

impl Checker<'_> {
    pub(super) fn prepare_static_parameters(
        &mut self,
        module: &Module,
    ) -> Vec<(usize, String, ModuleSignature)> {
        let mut static_parameters = Vec::new();
        if let Some(header) = &module.syntax.header {
            for parameter in &header.parameters {
                let signature = self.static_module_signature(&parameter.signature);
                let binder = self.static_binder(&parameter.name, parameter.span.start);
                let abstract_interface = signature.interface(&format!("$module:{binder}"));
                if let ModuleSignature::Structure { requirements } = &signature {
                    let contract = TypeDef {
                        kind: Kind::Module,
                        form: Form::ModuleContract {
                            requirements: requirements.clone(),
                        },
                    };
                    for (head, dictionary) in
                        module_local_dictionaries(&contract, &abstract_interface)
                    {
                        if self.classes.local.iter().any(|(known, _)| known == &head) {
                            self.error(
                                codes::T066,
                                parameter.span,
                                "more than one static parameter supplies the same local dictionary",
                            );
                        } else {
                            self.classes.local.push((head, dictionary));
                        }
                    }
                }
                self.env
                    .static_parameter(&parameter.name, abstract_interface);
                let index = static_parameters.len();
                let consumed = header.parameter_groups.iter().scan(0, |start, size| {
                    let end = *start + size; *start = end; Some(end)
                }).find(|end| *end > index).unwrap_or(index + 1);
                static_parameters.push((consumed - index, binder, signature));
            }
        }
        static_parameters
    }
    pub(super) fn prepare_static_applications(
        &mut self,
        module: &Module,
    ) -> Vec<(String, Interface)> {
        let mut static_results = Vec::new();
        for application in &module.syntax.applications {
            if let Some((_, Some(concrete))) = self.env.qualified_module(&application.name) {
                static_results.push((application.name.clone(), concrete.clone()));
                continue;
            }
            if let Some(result) = self.static_module_expression(
                &application.expression,
                module,
                &format!("{}.{}", module.name, application.name),
            ) {
                self.env.static_parameter(&application.name, result.clone());
                static_results.push((application.name.clone(), result));
            }
        }
        static_results
    }
    pub(super) fn finish_static_module(
        &mut self,
        module: &Module,
        imported: &dyn Fn(&str) -> Option<Interface>,
        interface: &mut Interface,
        static_parameters: Vec<(usize, String, ModuleSignature)>,
        static_results: &[(String, Interface)],
    ) {
        if let Some(header) = &module.syntax.header
            && let Some(body) = &header.body
        {
            let value = header
                .materialized_body
                .as_ref()
                .and_then(|name| imported(name))
                .or_else(|| self.static_module_expression(body, module, module.name));
            if let Some(mut value) = value {
                module.name.clone_into(&mut value.module);
                *interface = value;
            }
        }
        for application in &module.syntax.applications {
            if module
                .syntax
                .header
                .as_ref()
                .and_then(|h| h.exposing.as_ref())
                .is_some_and(|items| {
                    items
                        .iter()
                        .any(|i| i.kind == ItemKind::All || i.name == application.name)
                })
            {
                let result = static_results
                    .iter()
                    .find(|(name, _)| name == &application.name)
                    .map(|(_, i)| i.clone())
                    .or_else(|| imported(&format!("{}.{}", module.name, application.name)));
                if let Some(result) = result {
                    interface.modules.insert(application.name.clone(), result);
                }
            }
        }
        if !static_parameters.is_empty()
            || module
                .syntax
                .header
                .as_ref()
                .is_some_and(|h| h.result.is_some())
        {
            if module
                .syntax
                .header
                .as_ref()
                .is_some_and(|h| h.result.is_some() && h.exposing.is_none() && h.body.is_none())
            {
                let mut declared = self.declarations();
                for (name, child) in static_results {
                    declared.modules.insert(name.clone(), child.clone());
                }
                *interface = declared;
            }
            let mut result = module
                .syntax
                .header
                .as_ref()
                .and_then(|h| h.result.as_ref())
                .map_or_else(
                    || ModuleSignature::from_interface(interface),
                    |n| self.static_module_signature(n),
                );
            if module
                .syntax
                .header
                .as_ref()
                .is_some_and(|h| h.result.is_some())
            {
                if !result.accepts(interface) {
                    self.error(
                        codes::T061,
                        module.syntax.header.as_ref().expect("header").span,
                        "functor result does not satisfy its declared module signature",
                    );
                }
                *interface = result.seal(interface, module.name);
            }
            for (group_size, binder, parameter) in static_parameters.into_iter().rev() {
                result = ModuleSignature::Functor {
                    group_size,
                    binder,
                    parameter: Box::new(parameter),
                    result: Box::new(result),
                };
            }
            if !matches!(result, ModuleSignature::Structure { .. }) {
                interface.exports.clear();
                interface.modules.clear();
                interface.functor = Some(result);
            }
        }
    }

    fn static_module_expression(
        &mut self,
        node: &Node,
        module: &Module,
        path: &str,
    ) -> Option<Interface> {
        match node.kind {
            K::ModulePath => {
                let name: String = node.text(self.text).split_whitespace().collect();
                self.env
                    .qualified_module(&name)
                    .and_then(|(_, interface)| interface.cloned())
            }
            K::ModuleParenExpr => self.static_module_expression(node.nodes().next()?, module, path),
            K::ModuleApplyExpr => {
                let mut nodes = node.nodes();
                let function = self.static_module_expression(nodes.next()?, module, path)?;
                let mut signature = function.functor?;
                if let ModuleSignature::Functor { group_size, .. } = &signature
                    && *group_size != node.nodes().skip(1).count()
                {
                    self.error(codes::T002, self.span(node), format!("functor requires {group_size} arguments in this parameter group"));
                    return None;
                }
                for argument in nodes {
                    let actual = self.static_module_expression(
                        argument,
                        module,
                        &format!("{path}.Argument{}", argument.range().0),
                    )?;
                    let ModuleSignature::Functor {
                        binder,
                        parameter,
                        mut result,
                        ..
                    } = signature
                    else {
                        self.error(
                            codes::T002,
                            self.span(node),
                            "module expression is not a functor",
                        );
                        return None;
                    };
                    if !parameter.accepts(&actual) {
                        self.error(
                            codes::T061,
                            self.span(argument),
                            "module argument does not satisfy the functor signature",
                        );
                        return None;
                    }
                    result.rename_parameter(&binder, &actual.module);
                    signature = *result;
                }
                Some(signature.interface(path))
            }
            K::ModuleAnnotExpr => {
                let mut nodes = node.nodes();
                let actual = self.static_module_expression(nodes.next()?, module, path)?;
                let signature = self.static_module_signature(nodes.next()?);
                if !signature.accepts(&actual) {
                    self.error(
                        codes::T061,
                        self.span(node),
                        "module expression does not satisfy its annotation",
                    );
                    return None;
                }
                Some(signature.seal(&actual, path))
            }
            K::ModuleFunctorExpr => self.static_anonymous_functor(node, module, path),
            _ => None,
        }
    }

    fn static_anonymous_functor(
        &mut self,
        node: &Node,
        parent: &Module,
        path: &str,
    ) -> Option<Interface> {
        let groups: Vec<_> = node
            .nodes()
            .filter(|n| n.kind == K::ModuleParamGroup)
            .collect();
        let body = node
            .nodes()
            .find(|n| !matches!(n.kind, K::ModuleParamGroup | K::ModuleResult))?;
        let tree = Node {
            kind: K::SourceFile,
            children: if body.kind == K::Members {
                body.children.clone()
            } else {
                Vec::new()
            },
        };
        let mut syntax = resolve::module_syntax(&tree, self.text, self.file);
        let own_imports = syntax.imports.clone();
        syntax.imports.extend(parent.syntax.imports.clone());
        let mut captured = self.env.static_scope();
        for import in &syntax.imports {
            if let Some((_, Some(interface))) = self.env.qualified_module(import.qualifier()) {
                captured.insert(import.module.clone(), interface.clone());
            }
        }
        captured.insert(parent.name.to_owned(), self.declarations());
        let span = self.span(node);
        syntax.imports.push(Import {
            module: parent.name.to_owned(),
            alias: None,
            exposing: vec![Item {
                kind: ItemKind::All,
                name: String::new(),
                span,
            }],
            plugins: Vec::new(),
            span,
        });
        syntax.header = Some(Header {
            name: path.to_owned(),
            private: true,
            span,
            exposing: Some(vec![Item {
                kind: ItemKind::All,
                name: String::new(),
                span,
            }]),
            parameters: groups
                .iter()
                .flat_map(|g| g.nodes())
                .filter_map(|p| {
                    let name = p.child_tokens().find(|t| t.kind == K::Con)?;
                    let signature = p.nodes().next()?;
                    Some(FunctorParam {
                        name: name.text(self.text).to_owned(),
                        contract: signature.text(self.text).split_whitespace().collect(),
                        span: Span::new(self.file, name.start, name.end),
                        signature: signature.clone(),
                    })
                })
                .collect(),
            parameter_groups: groups.iter().map(|g| g.nodes().count()).collect(),
            result: node
                .nodes()
                .find(|n| n.kind == K::ModuleResult)
                .and_then(|n| n.nodes().next())
                .cloned(),
            body: (body.kind != K::Members).then(|| body.clone()),
            materialized_body: None,
        });
        let parameters = &syntax.header.as_ref().expect("anonymous header").parameters;
        let names: Vec<_> = parameters.iter().map(|p| p.name.as_str()).collect();
        syntax
            .imports
            .retain(|import| !names.contains(&import.qualifier()));
        for parameter in parameters {
            syntax.imports.push(Import {
                module: format!("$module:{}", parameter.name),
                alias: Some(parameter.name.clone()),
                exposing: Vec::new(),
                plugins: Vec::new(),
                span: parameter.span,
            });
        }
        for application in &syntax.applications {
            syntax.imports.push(Import {
                module: format!("{path}.{}", application.name),
                alias: Some(application.name.clone()),
                exposing: Vec::new(),
                plugins: Vec::new(),
                span: application.span,
            });
        }
        let lookup = |name: &str| captured.get(name).cloned();
        let child = Module {
            name: path,
            file: self.file,
            text: self.text,
            tree: &tree,
            syntax: &syntax,
            generated: parent.generated,
        };
        let (mut interface, resolved) = resolve::resolve(&child, &lookup);
        self.diagnostics.extend(resolved.into_iter().filter(|d| {
            d.code != codes::R012 || own_imports.iter().any(|i| Some(i.span) == d.span)
        }));
        let dictionaries = self.classes.local.clone();
        let (checked, _) = super::check_typed_with_module_dictionaries(
            &child,
            &lookup,
            super::builtins_of(self.options.stages_by_class),
            self.options,
            &mut interface,
            &dictionaries,
        );
        self.diagnostics.extend(checked);
        Some(interface)
    }
}
