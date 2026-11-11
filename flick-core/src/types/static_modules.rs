//! Static parameter environments and result abstraction for source modules.
use super::{Checker, Form, Kind, ModuleSignature, Type, TypeDef, module_local_dictionaries};
use std::collections::HashMap;

use crate::diagnostics::codes;
use crate::iface::Interface;
use crate::resolve::{self, FunctorParam, Header, Import, Item};
use crate::resolve::{ItemKind, Module};
use crate::source::Span;
use crate::syntax::cst::Element;
use crate::syntax::{cst::Node, kind::SyntaxKind as K};

impl Checker<'_> {
    pub(super) fn prepare_static_parameters(
        &mut self,
        module: &Module,
    ) -> Vec<(usize, String, ModuleSignature)> {
        let mut static_parameters = Vec::new();
        if let Some(header) = &module.syntax.header {
            let mut group_end = 0;
            let mut group_mark = self.classes.local.len();
            for parameter in &header.parameters {
                let index = static_parameters.len();
                if index == group_end {
                    group_mark = self.classes.local.len();
                    group_end = header
                        .parameter_groups
                        .iter()
                        .scan(0, |start, size| {
                            *start += size;
                            Some(*start)
                        })
                        .find(|end| *end > index)
                        .unwrap_or(index + 1);
                }
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
                        if self.classes.local[group_mark..]
                            .iter()
                            .any(|(known, _)| known == &head)
                        {
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
                static_parameters.push((group_end - index, binder, signature));
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
            let concrete = self
                .env
                .qualified_module(&application.name)
                .and_then(|(_, interface)| interface.cloned());
            let path = format!("{}.{}", module.name, application.name);
            // Project elaboration supplies a concrete interface. Standalone
            // checking instead checks the preceding source without executing
            // it, so local signatures and values have the same lexical scope.
            let prefix = concrete
                .is_none()
                .then(|| self.static_prefix(module, application.span.start));
            let saved = prefix.as_ref().map(|p| self.env.static_lexical(p));
            let mut result = concrete
                .or_else(|| self.static_module_expression(&application.expression, module, &path));
            if let Some(actual) = &result
                && let Some(annotation) = &application.annotation
            {
                let signature = self.static_module_signature(annotation);
                if signature.accepts(actual) {
                    result = Some(signature.seal(actual, &path));
                } else {
                    self.error(
                        codes::T061,
                        self.span(annotation),
                        "module expression does not satisfy its annotation",
                    );
                    result = None;
                }
            }
            if let Some(saved) = saved {
                self.env
                    .restore_static_lexical(prefix.as_ref().expect("lexical prefix"), saved);
            }
            if let Some(result) = result {
                self.env.static_parameter(&application.name, result.clone());
                static_results.push((application.name.clone(), result));
            }
        }
        static_results
    }

    fn static_prefix(&self, parent: &Module, before: u32) -> Interface {
        let tree = Node {
            kind: K::SourceFile,
            children: parent
                .tree
                .nodes()
                .filter(|node| node.kind == K::ImportDecl || node.range().1 <= before)
                .cloned()
                .map(Element::Node)
                .collect(),
        };
        let mut syntax = resolve::module_syntax(&tree, self.text, self.file);
        syntax.header.clone_from(&parent.syntax.header);
        if let Some(header) = &mut syntax.header {
            header.private = true;
            header.exposing = None;
            header.result = None;
            header.body = None;
            header.materialized_body = None;
        }
        syntax.imports.clone_from(&parent.syntax.imports);
        let mut captured = HashMap::new();
        for import in &syntax.imports {
            if let Some((_, Some(interface))) = self.env.qualified_module(import.qualifier()) {
                captured.insert(import.module.clone(), interface.clone());
            }
        }
        for application in &syntax.applications {
            if let Some((_, Some(interface))) = self.env.qualified_module(&application.name) {
                let name = format!("{}.{}", parent.name, application.name);
                captured.insert(name.clone(), interface.clone());
                syntax.imports.push(Import {
                    module: name,
                    alias: Some(application.name.clone()),
                    exposing: Vec::new(),
                    plugins: Vec::new(),
                    span: application.span,
                });
            }
        }
        let env = &self.env;
        let lookup = |name: &str| {
            captured
                .get(name)
                .cloned()
                .or_else(|| env.static_known(name))
        };
        let module = Module {
            name: parent.name,
            file: self.file,
            text: self.text,
            tree: &tree,
            syntax: &syntax,
            generated: parent.generated,
        };
        let mut interface = Interface {
            module: parent.name.to_owned(),
            ..Interface::default()
        };
        let (_, typing) = super::check_typed_with_static_context(
            &module,
            &lookup,
            super::builtins_of(self.options.stages_by_class),
            self.options,
            &mut interface,
            super::StaticContext {
                dictionaries: &self.classes.local,
                witnesses: self.static_witnesses,
            },
        );
        (*typing.declarations).clone()
    }
    pub(super) fn static_declarations(
        &self,
        module: &Module,
        interface: &Interface,
        static_results: &[(String, Interface)],
    ) -> Interface {
        let mut declarations = self.declarations();
        for (name, child) in static_results {
            declarations.modules.insert(name.clone(), child.clone());
        }
        if let Some(header) = &module.syntax.header {
            let mut signature = interface.functor.as_ref();
            for parameter in &header.parameters {
                if let Some(ModuleSignature::Functor {
                    binder,
                    parameter: input,
                    result,
                    ..
                }) = signature
                {
                    declarations.modules.insert(
                        parameter.name.clone(),
                        input.interface(&format!("$module:{binder}")),
                    );
                    signature = Some(result);
                }
            }
        }
        declarations
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
            self.normalize_static_interface(interface);
            let mut result = module
                .syntax
                .header
                .as_ref()
                .and_then(|h| h.result.as_ref())
                .map_or_else(
                    || ModuleSignature::from_interface(interface),
                    |n| self.static_module_signature(n),
                );
            result.map_types(&mut |ty| self.static_type(ty));
            if module
                .syntax
                .header
                .as_ref()
                .is_some_and(|h| h.result.is_some())
            {
                self.check_static_result(module, interface, &result);
                self.check_static_exposing(module, interface, &result);
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
                interface.signature_explicit = module
                    .syntax
                    .header
                    .as_ref()
                    .is_some_and(|h| h.result.is_some());
            }
        }
        self.check_static_public_signature(module, interface);
    }

    fn check_static_result(
        &mut self,
        module: &Module,
        interface: &Interface,
        result: &ModuleSignature,
    ) {
        let valid = if self.static_witnesses.is_empty() {
            result.accepts(interface)
        } else {
            let mut result = result.clone();
            let mut interface = interface.clone();
            for witness in self.static_witnesses {
                result.normalize_in(witness);
                super::normalize_module_interface(&mut interface, witness);
            }
            result.accepts(&interface)
        };
        if !valid {
            self.error(
                codes::T061,
                module.syntax.header.as_ref().expect("header").span,
                "functor result does not satisfy its declared module signature",
            );
        }
    }

    fn check_static_exposing(
        &mut self,
        module: &Module,
        interface: &Interface,
        result: &ModuleSignature,
    ) {
        if let Some(header) = &module.syntax.header
            && let Some(exposing) = &header.exposing
        {
            let visible = result.project(interface);
            let hidden = interface
                .exports
                .iter()
                .find(|export| {
                    visible
                        .find(export.entity.namespace, &export.entity.name)
                        .is_none()
                })
                .map(|e| e.entity.name.clone())
                .or_else(|| {
                    interface
                        .modules
                        .keys()
                        .find(|name| !visible.modules.contains_key(*name))
                        .cloned()
                });
            if let Some(name) = hidden {
                let at = exposing
                    .iter()
                    .find(|item| item.kind == ItemKind::All || item.name == name)
                    .map_or(header.span, |item| item.span);
                self.error(
                    codes::T061,
                    at,
                    format!("exposing `{name}` disagrees with the declared result signature"),
                );
            }
        }
    }

    fn check_static_public_signature(&mut self, module: &Module, interface: &Interface) {
        if let Some(header) = &module.syntax.header
            && !header.private
        {
            if interface.functor.is_some() && !interface.signature_explicit {
                self.error(
                    codes::T067,
                    header.span,
                    "public functor requires an explicit result signature",
                );
            }
            for (name, child) in &interface.modules {
                if child.functor.is_some() && !child.signature_explicit {
                    let at = module
                        .syntax
                        .applications
                        .iter()
                        .find(|a| a.name == *name)
                        .map_or(header.span, |a| a.span);
                    self.error(
                        codes::T067,
                        at,
                        format!("public functor `{name}` requires an explicit result signature"),
                    );
                }
            }
        }
    }

    fn static_type(&self, ty: &Type) -> Type {
        fn expand(checker: &Checker<'_>, ty: &Type, fuel: usize) -> Type {
            if fuel == 0 {
                return ty.clone();
            }
            let (head, args) = ty.spine();
            if let Type::Con { module, name } = head
                && let Some(TypeDef {
                    form:
                        Form::Alias {
                            params,
                            body,
                            opaque: false,
                        },
                    ..
                }) = checker.env.type_def(module, name)
                && args.len() >= params.len()
            {
                let subst: Vec<_> = params
                    .iter()
                    .cloned()
                    .zip(args.iter().map(|ty| (*ty).clone()))
                    .collect();
                return expand(
                    checker,
                    &Type::apps(
                        body.substitute(&subst),
                        args[params.len()..].iter().map(|ty| (*ty).clone()),
                    ),
                    fuel - 1,
                );
            }
            ty.map_children(&mut |ty| expand(checker, ty, fuel - 1))
        }
        expand(self, ty, 64)
    }

    fn normalize_static_interface(&self, interface: &mut Interface) {
        for export in &mut interface.exports {
            if let Some(ty) = &mut export.scheme {
                *ty = self.static_type(ty);
            }
            if let Some(definition) = &mut export.definition {
                match &mut definition.form {
                    Form::Alias {
                        body,
                        opaque: false,
                        ..
                    } => *body = self.static_type(body),
                    Form::Class {
                        supers,
                        default_sigs,
                        ..
                    } => {
                        for ty in supers
                            .iter_mut()
                            .chain(default_sigs.iter_mut().map(|(_, ty)| ty))
                        {
                            *ty = self.static_type(ty);
                        }
                    }
                    Form::Service { operations, .. } => {
                        for operation in operations {
                            operation.ty = self.static_type(&operation.ty);
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(signature) = &mut interface.functor {
            signature.map_types(&mut |ty| self.static_type(ty));
        }
        for child in interface.modules.values_mut() {
            self.normalize_static_interface(child);
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
                let explicit = function.signature_explicit;
                let mut signature = function.functor?;
                if let ModuleSignature::Functor { group_size, .. } = &signature
                    && *group_size != node.nodes().skip(1).count()
                {
                    self.error(
                        codes::T002,
                        self.span(node),
                        format!("functor requires {group_size} arguments in this parameter group"),
                    );
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
                    result.instantiate_parameter(&binder, &actual);
                    signature = *result;
                }
                let mut result = signature.interface(path);
                result.signature_explicit = explicit;
                Some(result)
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

    fn static_functor_header(&self, node: &Node, path: &str, body: &Node) -> Header {
        let groups: Vec<_> = node
            .nodes()
            .filter(|n| n.kind == K::ModuleParamGroup)
            .collect();
        let span = self.span(node);
        Header {
            name: path.to_owned(),
            private: true,
            span,
            exposing: (!node.nodes().any(|n| n.kind == K::ModuleResult)).then(|| {
                vec![Item {
                    kind: ItemKind::All,
                    name: String::new(),
                    span,
                }]
            }),
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
        }
    }

    fn static_anonymous_functor(
        &mut self,
        node: &Node,
        parent: &Module,
        path: &str,
    ) -> Option<Interface> {
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
        let mut captured = HashMap::new();
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
        syntax.header = Some(self.static_functor_header(node, path, body));
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
        let env = &self.env;
        let lookup = |name: &str| {
            captured
                .get(name)
                .cloned()
                .or_else(|| env.static_known(name))
        };
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
        let (checked, _) = super::check_typed_with_static_context(
            &child,
            &lookup,
            super::builtins_of(self.options.stages_by_class),
            self.options,
            &mut interface,
            super::StaticContext {
                dictionaries: &dictionaries,
                witnesses: self.static_witnesses,
            },
        );
        self.diagnostics.extend(checked);
        Some(interface)
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::{Severity, codes};
    use crate::source::SourceMap;

    #[test]
    fn single_file_hom_captures_only_earlier_declarations() {
        let source = "module App.Factory exposing (Make)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\noffset : Int\nlet offset = 1\nmodule Make(K : Input) : Output =\n  answer : Int\n  let answer = K.seed + offset\n";
        let mut sources = SourceMap::default();
        let file = sources.add("App/Factory.fl", source);
        let diagnostics = crate::check(&sources, file);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");

        let late = source.replace("offset : Int\nlet offset = 1\n", "");
        let file = sources.add(
            "App/Late.fl",
            format!("{late}offset : Int\nlet offset = 1\n"),
        );
        let diagnostics = crate::check(&sources, file);
        assert!(
            diagnostics.iter().any(|d| d.severity == Severity::Error
                && d.code == codes::T001
                && d.message.contains("offset")),
            "{diagnostics:?}"
        );
    }
}
