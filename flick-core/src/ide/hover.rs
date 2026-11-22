//! Hover: the source type of what a position names, as Flick writes it,
//! with its rows, captures and constraints, never a TypeScript helper.

use std::collections::HashSet;
use std::fmt::Write;
use std::ptr;

use super::{Ide, Symbol, navigate::Source, tree};
use crate::doc;
use crate::iface::Namespace;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Form, Ref, TypeDef, decl_param_names, format_markdown, show_signature};

mod attributes;
#[cfg(test)]
mod attributes_tests;
pub(super) mod companions;
mod exposing;
mod keywords;
mod kinds;
mod operators;
mod types;
mod written;

use companions::enum_literal;
use kinds::kind_description;
use written::{as_written, signature_as_written};

fn local_code(name: &str, ty: &str, mutable: bool) -> String {
    let mut code = String::from(if mutable { "mut " } else { "" });
    code.push_str(name);
    code.push_str(" : ");
    code.push_str(ty);
    code
}

pub(super) fn documentation(source: &Source, name: &str) -> Option<String> {
    source
        .tree
        .nodes()
        .filter(|n| n.kind == K::ClassDecl)
        .find_map(|decl| member_documentation(decl, name, &source.text))
        .or_else(|| source.docs().remove(name))
}

fn member_documentation(decl: &Node, name: &str, text: &str) -> Option<String> {
    let members = decl.nodes().find(|n| n.kind == K::Members)?;
    let signature = members
        .nodes()
        .find(|n| n.kind == K::Signature && tree::name_of(n, K::Ident, text) == Some(name))?;
    doc::leading_doc(members, signature, text).or_else(|| {
        // The first member's leading trivia precedes the Members node.
        if ptr::eq(members.nodes().next()?, signature) {
            doc::leading_doc(decl, members, text)
        } else {
            None
        }
    })
}

impl Ide<'_> {
    fn instance_method_hover(
        &self,
        file: FileId,
        offset: u32,
        module: &str,
        instance: &str,
        method: &str,
    ) -> Option<(String, Option<String>)> {
        let m = self.module(file)?;
        // at a use the class is the method's; at the instance's own name, the instance's
        let owner = match tree::innermost(m.typing.names.iter(), offset) {
            Some((_, Ref::Method { module, .. })) => module,
            _ => {
                &m.typing
                    .instances
                    .iter()
                    .find(|i| i.name == instance)?
                    .class
                    .0
            }
        };
        let ty = match tree::innermost(m.typing.types.iter(), offset) {
            Some((_, ty)) => show_signature(ty),
            None => self.instance_signature(m, instance, method)?,
        };
        let mut note = self
            .source(owner)
            .and_then(|source| documentation(&source, method))
            .map_or_else(String::new, |doc| format!("{doc}\n\n"));
        let _ = write!(note, "from `instance {instance}` in `{module}`");
        Some((format!("{method} : {ty}"), Some(note)))
    }

    fn operation_hover(
        &self,
        module: &str,
        service: &str,
        name: &str,
    ) -> Option<(String, Option<String>)> {
        let source = self.source(module)?;
        let decl = source.tree.nodes().find(|d| {
            d.kind == K::ServiceDecl && tree::name_of(d, K::Con, &source.text) == Some(service)
        })?;
        let sig = decl
            .nodes()
            .filter(|n| n.kind == K::Members)
            .flat_map(Node::nodes)
            .find(|s| tree::name_of(s, K::Ident, &source.text) == Some(name))?;
        let ty = signature_as_written(sig.nodes().next()?, &source.text, &HashSet::new());
        let note = member_documentation(decl, name, &source.text)
            .map_or_else(String::new, |doc| format!("{doc}\n\n"));
        Some((
            format!("{service}.{name} : {ty}"),
            Some(format!("{note}an operation of `{service}`")),
        ))
    }

    /// The class `class` named at `at` in a `deriving` clause: the instance
    /// it makes, as `instance Show a => Show (Tree a)`, and what that does.
    fn derived(
        &self,
        file: FileId,
        at: (u32, u32),
        module: &str,
        class: &str,
    ) -> Option<(String, String)> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, at.0);
        let i = path.iter().rposition(|n| n.kind == K::DerivingClause)?;
        let decl = path.get(i.checked_sub(1)?)?;
        let name = tree::name_of(decl, K::Con, text)?;
        // the instance as the checker derived it: its context follows the kinds of the parameters
        let instance = m.typing.instances.iter().find(|i| {
            i.span.0 <= at.0 && at.1 <= i.span.1 && i.class.0 == module && i.class.1 == class
        });
        let note = match class {
            "Eq" => "Two values are equal when they have the same constructor and equal fields.",
            "Ord" => {
                "Orders by constructor, in the order they are declared, then by the fields \
                 from left to right; derived only together with `Eq`."
            }
            "Hash" => {
                "Hashes the constructor and its fields, so that equal values hash alike; \
                 derived only together with `Eq`."
            }
            "Show" => "Shows the constructor and its fields, as in `Circle 2`.",
            "Value" => {
                "Parses the name of a constructor without fields in kebab case \
                 (`GreenLight` is `green-light`), or the base of a refined type, \
                 checking its predicate."
            }
            "Schema" => {
                return Some((
                    format!("{name}.schema"),
                    "Defines the witness `T.schema`, the description of the type that \
                     codecs such as `Codec.json` are built from."
                        .to_owned(),
                ));
            }
            _ => return None,
        };
        let instance = instance?;
        let context = match instance.context.as_slice() {
            [] => String::new(),
            [c] => format!("{c} => "),
            cs => format!("({}) => ", cs.join(", ")),
        };
        Some((
            format!("instance {context}{}", instance.name),
            format!("Derived by the compiler. {note}"),
        ))
    }

    fn typed_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let (range, ty) = tree::innermost(m.typing.types.iter(), offset)?;
        let name = &self.text(file)[range.0 as usize..range.1 as usize];
        Some((
            format!("```flick\n{name} : {}\n```", m.typing.shown(range, ty)),
            range,
        ))
    }

    /// Markdown for the symbol at `offset`, and the range it covers.
    pub fn hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        if let Some(hover) = self.attribute_hover(file, offset) {
            return Some(hover);
        }
        if let Some(hover) = self.kind_hover(file, offset) {
            return Some(hover);
        }
        if let Some(hover) = self.head_hover(file, offset) {
            return Some(hover);
        }
        if let Some((code, note, range)) = self.keyword(file, offset) {
            return Some((format!("```flick\n{code}\n```\n\n{note}"), range));
        }
        if let Some(hover) = self.operator_hover(file, offset) {
            return Some(hover);
        }
        if let Some(hover) = self.embedded_hover(file, offset) {
            return Some(hover);
        }
        if let Some(hover) = self.exposing_hover(file, offset) {
            return Some(hover);
        }
        if let Some(hover) = self.companion_hover(file, offset) {
            return Some(hover);
        }
        self.symbol_hover(file, offset)
    }

    fn symbol_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let Some((symbol, mut range)) = self.symbol_at(file, offset) else {
            return self.typed_hover(file, offset);
        };
        let (code, note) = match &symbol {
            Symbol::Module(module) => {
                let source = self.source(module)?;
                (
                    format!("module {module}"),
                    doc::module_comment(&source.text),
                )
            }
            Symbol::Local { name, binder, .. } => {
                let m = self.module(file)?;
                let (at, ty) = tree::innermost(m.typing.types.iter(), offset)
                    .or_else(|| tree::innermost(m.typing.types.iter(), binder.0))?;
                let mutable = tree::path_to(&m.tree, binder.0).iter().any(|decl| {
                    decl.kind == K::LetDecl
                        && decl.has_token(K::MutKw)
                        && decl.nodes().next().is_some_and(|p| p.range() == *binder)
                });
                (local_code(name, &m.typing.shown(at, ty), mutable), None)
            }
            Symbol::Entity(e) => {
                let interface = self.interface(&e.module)?;
                let export = self
                    .analysis
                    .module(&e.module)
                    .filter(|m| m.file == file)
                    .and_then(|m| m.typing.declarations.find(e.namespace, &e.name))
                    .or_else(|| interface.find(e.namespace, &e.name));
                let source = self.source(&e.module);
                let code = match e.namespace {
                    Namespace::Type => self.type_hover(
                        file,
                        e,
                        source.as_ref(),
                        export.and_then(|e| e.definition.as_ref()),
                        &mut range,
                    )?,
                    _ => match (
                        self.signature(&e.module, &e.name),
                        export.and_then(|e| e.scheme.as_ref()),
                    ) {
                        (Some(written), _) => format!("{} : {written}", e.name),
                        (None, Some(scheme)) => format!("{} : {}", e.name, show_signature(scheme)),
                        (None, None) => {
                            let m = self.module(file)?;
                            let (_, ty) = tree::innermost(m.typing.types.iter(), offset)?;
                            format!("{} : {}", e.name, show_signature(ty))
                        }
                    },
                };
                // a constructor of an enum shows the literal it is
                let literal = (e.namespace == Namespace::Constructor)
                    .then(|| export.and_then(|x| x.parent.as_deref()))
                    .flatten()
                    .and_then(|ty| interface.find(Namespace::Type, ty))
                    .and_then(|t| enum_literal(t.definition.as_ref()?, &e.name));
                let code = match literal {
                    Some(literal) => format!("{code}\n{} = {literal}", e.name),
                    None => code,
                };
                let variance = (e.namespace == Namespace::Type)
                    .then(|| {
                        entity_variance(
                            &e.name,
                            export.and_then(|item| item.definition.as_ref()),
                            source.as_ref(),
                        )
                    })
                    .flatten();
                let doc = source
                    .and_then(|source| documentation(&source, &e.name))
                    .or_else(|| kind_note(export.and_then(|item| item.definition.as_ref())));
                let note = join_notes(doc, variance, &e.module);
                match self.derived(file, range, &e.module, &e.name) {
                    Some((instance, what)) => (instance, Some(format!("{what}\n\n{note}"))),
                    None => (code, Some(note)),
                }
            }
            Symbol::Operation {
                module,
                service,
                name,
            } => self.operation_hover(module, service, name)?,
            Symbol::InstanceMethod {
                module,
                instance,
                method,
            } => self.instance_method_hover(file, offset, module, instance, method)?,
        };
        let mut text = format!("```flick\n{code}\n```");
        if let Some(note) = note {
            let _ = write!(text, "\n\n{note}");
        }
        Some((text, range))
    }
}

fn entity_variance(
    name: &str,
    definition: Option<&TypeDef>,
    source: Option<&Source>,
) -> Option<String> {
    let definition = definition?;
    if definition.variance.is_empty() {
        return None;
    }
    let names = match &definition.form {
        Form::Class { params, .. } => Some(params.clone()),
        _ => source.and_then(|s| decl_param_names(&s.tree, &s.text, name)),
    };
    format_markdown(names.as_deref(), &definition.variance)
        .map(|shown| format!("Variance: {shown}"))
}

fn kind_note(definition: Option<&TypeDef>) -> Option<String> {
    match definition {
        Some(TypeDef {
            kind,
            form: Form::Kind,
            ..
        }) => Some(kind_description(kind).to_owned()),
        Some(TypeDef {
            kind,
            form: Form::KindMember,
            ..
        }) => Some(format!("A member of the finite kind `{kind}`.")),
        _ => None,
    }
}

fn join_notes(doc: Option<String>, variance: Option<String>, module: &str) -> String {
    let mut parts = Vec::new();
    if let Some(doc) = doc {
        parts.push(doc);
    }
    if let Some(variance) = variance {
        parts.push(variance);
    }
    parts.push(format!("from `{module}`"));
    parts.join("\n\n")
}
