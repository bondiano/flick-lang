//! Static parameter environments and result abstraction for source modules.
use super::{Checker, Form, Kind, ModuleSignature, TypeDef, module_local_dictionaries};
use crate::diagnostics::codes;
use crate::iface::Interface;
use crate::resolve::{ItemKind, Module};

impl Checker<'_> {
    pub(super) fn prepare_static_parameters(
        &mut self,
        module: &Module,
    ) -> Vec<(String, ModuleSignature)> {
        let mut static_parameters = Vec::new();
        if let Some(header) = &module.syntax.header {
            for parameter in &header.parameters {
                let signature = self.static_module_signature(&parameter.signature);
                let abstract_interface =
                    signature.interface(&format!("$module:{}", parameter.name));
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
                static_parameters.push((parameter.name.clone(), signature));
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
            if let Some((_, Some(provided))) = self.env.qualified_module(&application.functor) {
                let mut provided = provided.functor.clone();
                for argument in &application.arguments {
                    let Some(ModuleSignature::Functor {
                        binder,
                        parameter,
                        mut result,
                    }) = provided
                    else {
                        provided = None;
                        break;
                    };
                    if let Some((_, Some(actual))) = self.env.qualified_module(argument) {
                        if !parameter.accepts(actual) {
                            self.error(codes::T061, application.span, format!("module argument `{argument}` does not satisfy the factory parameter"));
                            provided = None;
                            break;
                        }
                        result.rename_parameter(&binder, &actual.module);
                        provided = Some(*result);
                    } else {
                        provided = None;
                        break;
                    }
                }
                if let Some(signature) = provided {
                    let result =
                        signature.interface(&format!("{}.{}", module.name, application.name));
                    self.env.static_parameter(&application.name, result.clone());
                    static_results.push((application.name.clone(), result));
                }
            }
        }
        static_results
    }
    pub(super) fn finish_static_module(
        &mut self,
        module: &Module,
        imported: &dyn Fn(&str) -> Option<Interface>,
        interface: &mut Interface,
        static_parameters: Vec<(String, ModuleSignature)>,
        static_results: &[(String, Interface)],
    ) {
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
            for (binder, parameter) in static_parameters.into_iter().rev() {
                result = ModuleSignature::Functor {
                    binder,
                    parameter: Box::new(parameter),
                    result: Box::new(result),
                };
            }
            if !matches!(result, ModuleSignature::Structure { .. }) {
                interface.functor = Some(result);
            }
        }
    }
}
