//! Hover: the source type of what a position names, as Flick writes it,
//! with its rows, captures and constraints, never a TypeScript helper.

use std::fmt::Write;
use std::ptr;

use super::{Ide, Symbol, navigate::Source, tree};
use crate::doc;
use crate::iface::{Decl, Namespace};
use crate::project::Analyzed;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Form, Ref, TypeDef, component_instance, show_type};

pub(super) mod companions;
mod kinds;
mod types;

use crate::types::lang::Lang;
use kinds::kind_description;

/// `node` as its source lays it out, one written line per line, its
/// continuation lines dedented by the indentation of its first line.
fn as_written(node: &Node, text: &str) -> String {
    let (start, end) = node.range();
    let line_start = text[..start as usize].rfind('\n').map_or(0, |i| i + 1);
    let indent = text[line_start..start as usize]
        .chars()
        .take_while(|c| c.is_whitespace())
        .count();
    let mut lines = text[start as usize..end as usize].lines();
    let first = lines.next().unwrap_or_default().trim_end().to_owned();
    lines.fold(first, |mut acc, line| {
        let cut = line.len() - line.trim_start().len();
        acc.push('\n');
        acc.push_str(line[cut.min(indent)..].trim_end());
        acc
    })
}

fn documentation(source: &Source, name: &str) -> Option<String> {
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
    /// The signature of a top-level value as its source writes it.
    fn signature(&self, module: &str, name: &str) -> Option<String> {
        let source = self.source(module)?;
        let sig = source.tree.nodes().find(|d| {
            d.kind == K::Signature
                && d.child_tokens()
                    .next()
                    .is_some_and(|t| t.text(&source.text) == name)
        })?;
        Some(as_written(sig.nodes().next()?, &source.text))
    }

    /// The signature of `method` as the instance `instance` of `m` writes it.
    fn instance_signature(&self, m: &Analyzed, instance: &str, method: &str) -> Option<String> {
        let info = m.typing.instances.iter().find(|i| i.name == instance)?;
        let text = self.text(m.file);
        let sig = tree::find(&m.tree, info.span)?
            .nodes()
            .filter(|n| n.kind == K::Members)
            .flat_map(Node::nodes)
            .find(|n| n.kind == K::Signature && tree::name_of(n, K::Ident, text) == Some(method))?;
        Some(as_written(sig.nodes().next()?, text))
    }

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
            Some((_, ty)) => show_type(ty),
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
        let ty = as_written(sig.nodes().next()?, &source.text);
        let note = member_documentation(decl, name, &source.text)
            .map_or_else(String::new, |doc| format!("{doc}\n\n"));
        Some((
            format!("{service}.{name} : {ty}"),
            Some(format!("{note}an operation of `{service}`")),
        ))
    }

    /// A keyword at `offset` that opens an expression: the type of the whole
    /// expression and what the keyword does; at `ignore` and `await` the
    /// type of the operand.
    fn keyword(&self, file: FileId, offset: u32) -> Option<(String, String, (u32, u32))> {
        let m = self.module(file)?;
        let parent = *tree::path_to(&m.tree, offset).last()?;
        let kw = parent
            .child_tokens()
            .find(|t| t.start <= offset && offset <= t.end && t.kind != K::Ident)?;
        // a component's instance is recorded at the start of its body
        let at = match kw.kind {
            K::ComponentKw => parent
                .nodes()
                .flat_map(Node::nodes)
                .find(|n| n.kind == K::ComponentExpr)
                .map_or((kw.start, kw.end), component_instance),
            _ => (kw.start, kw.end),
        };
        let ty = m.typing.types.get(&at)?;
        let shown = show_type(ty);
        let (head, note) = match kw.kind {
            K::TaskKw => (
                "task { … }",
                "Builds this computation without running it; `await` runs it.",
            ),
            K::ScopeKw => (
                "scope { … }",
                "Runs its body in a fresh region and releases what the body \
                 acquired when it ends; the value may not mention the region.",
            ),
            K::ProvideKw => (
                "provide … { … }",
                "Runs its body with the services the layer builds; they leave \
                 the row, and what building the layer needs is added.",
            ),
            K::TryKw => (
                "try … with",
                "Handles the errors of its body: an error leaves the row when an \
                 arm without a guard matches every value of it.",
            ),
            K::MatchKw => ("match … with", "The value of the arm that matches."),
            K::IfKw => ("if … then … else …", "The value of the branch taken."),
            K::FunKw => ("fun … -> …", "An anonymous function."),
            K::ComponentKw => (
                "component",
                "A view of its props. The statements of its body set up each \
                 instance once, in the instance's region `s`; its last \
                 expression renders it, again when an atom it read changes.",
            ),
            K::UiKw => (
                "ui { … }",
                "Describes a user interface; building it runs no handler.",
            ),
            K::SqlKw => ("sql { … }", "A typed SQL query."),
            K::AwaitKw => {
                let ran = match ty.spine() {
                    (h, args) if h.is_lang(Lang::Eff) && args.len() == 3 => show_type(args[2]),
                    _ => shown.clone(),
                };
                return Some((
                    format!("await … : {ran}"),
                    format!("Runs `{shown}` once in the current fiber."),
                    (kw.start, kw.end),
                ));
            }
            K::IgnoreKw => {
                let mut note = format!(
                    "Evaluates its operand and discards its value of type `{shown}`; \
                     its effects and errors are kept."
                );
                if ty.spine().0.is_lang(Lang::Eff) {
                    note.push_str(
                        " The computation is not run: `ignore (await …)` runs it \
                         and discards its result.",
                    );
                }
                return Some((
                    format!("ignore : {shown} -> Unit"),
                    note,
                    (kw.start, kw.end),
                ));
            }
            _ => return None,
        };
        Some((
            format!("{head} : {shown}"),
            note.to_owned(),
            (kw.start, kw.end),
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
        Some((format!("```flick\n{name} : {}\n```", show_type(ty)), range))
    }

    fn exposing_hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let item = path.iter().find(|n| n.kind == K::ExposeItem)?;
        let dots = item
            .child_tokens()
            .find(|t| t.kind == K::DotDot && t.start <= offset && offset < t.end)?;
        let name = tree::name_of(item, K::Con, text)?;
        let declaration = path
            .iter()
            .find(|n| matches!(n.kind, K::ImportDecl | K::ModuleHeader))?;
        let module: String = declaration
            .nodes()
            .find(|n| n.kind == K::ModulePath)?
            .text(text)
            .split_whitespace()
            .collect();
        let interface = self.interface(&module)?;
        let parent = interface.find(Namespace::Type, name)?;
        let members = match parent.decl {
            Decl::Class => "methods",
            Decl::Kind => "kind members",
            Decl::Adt | Decl::Error => "constructors",
            _ => return None,
        };
        let direction = if declaration.kind == K::ImportDecl {
            "imported"
        } else {
            "exported"
        };
        let mut markdown = format!("**`{name}(..)` — {direction} {members}**\n\nfrom `{module}`");
        let source = self.source(&parent.entity.module);
        let mut any = false;
        for member in interface.members(name) {
            any = true;
            let name = &member.entity.name;
            let code = match (&member.scheme, &member.definition) {
                (Some(scheme), _) => format!("{name} : {}", show_type(scheme)),
                (_, Some(definition)) => format!("{name} : {}", definition.kind),
                _ => name.clone(),
            };
            let _ = write!(markdown, "\n\n```flick\n{code}\n```");
            if let Some(doc) = source.as_ref().and_then(|s| documentation(s, name)) {
                let _ = write!(markdown, "\n\n{doc}");
            }
        }
        if !any {
            markdown.push_str("\n\nNo public members.");
        }
        Some((markdown, (dots.start, dots.end)))
    }

    /// Markdown for the symbol at `offset`, and the range it covers.
    pub fn hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        if let Some(hover) = self.kind_hover(file, offset) {
            return Some(hover);
        }
        if let Some((code, note, range)) = self.keyword(file, offset) {
            return Some((format!("```flick\n{code}\n```\n\n{note}"), range));
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
                let ty = tree::innermost(m.typing.types.iter(), offset)
                    .or_else(|| tree::innermost(m.typing.types.iter(), binder.0))?
                    .1;
                (format!("{name} : {}", show_type(ty)), None)
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
                        (None, Some(scheme)) => format!("{} : {}", e.name, show_type(scheme)),
                        (None, None) => {
                            let m = self.module(file)?;
                            let (_, ty) = tree::innermost(m.typing.types.iter(), offset)?;
                            format!("{} : {}", e.name, show_type(ty))
                        }
                    },
                };
                let doc = source
                    .and_then(|source| documentation(&source, &e.name))
                    .or_else(|| match export.and_then(|item| item.definition.as_ref()) {
                        Some(TypeDef {
                            kind,
                            form: Form::Kind,
                        }) => Some(kind_description(kind).to_owned()),
                        Some(TypeDef {
                            kind,
                            form: Form::KindMember,
                        }) => Some(format!("A member of the finite kind `{kind}`.")),
                        _ => None,
                    });
                let from = format!("from `{}`", e.module);
                let note = doc.map_or(from.clone(), |d| format!("{d}\n\n{from}"));
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
