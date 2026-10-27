//! Hover: the source type of what a position names, as Flick writes it,
//! with its rows, captures and constraints, never a TypeScript helper.

use std::fmt::Write;

use super::{Ide, Symbol, Target, navigate::Source, tree};
use crate::doc;
use crate::iface::{Entity, Namespace};
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Form, TypeDef, component_instance, show_type};

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
        let ty = sig.nodes().next()?;
        Some(
            ty.text(&source.text)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        )
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
            K::CodecKw => ("codec … { … }", "A codec built field by field."),
            K::CliKw => ("cli … { … }", "A command-line interface."),
            K::SchemaKw => ("schema …", "The `Schema` of a type that derives it."),
            K::AwaitKw => {
                let ran = match ty.spine() {
                    (h, args) if h.is_prim("Eff") && args.len() == 3 => show_type(args[2]),
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
                if ty.spine().0.is_prim("Eff") {
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
                    format!("schema {name}"),
                    "Defines the witness `schema T`, the description of the type that \
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

    fn type_hover(
        &self,
        file: FileId,
        entity: &Entity,
        source: Option<&Source>,
        definition: Option<&TypeDef>,
        range: &mut (u32, u32),
    ) -> Option<String> {
        let local_alias = self
            .analysis
            .module(&entity.module)
            .is_some_and(|m| m.typing.aliases.contains_key(&entity.name));
        let public_alias = definition.is_some_and(|d| matches!(&d.form, Form::Alias { .. }));
        if (local_alias || public_alias)
            && let Some(source) = source
            && let Some(decl) = source.tree.nodes().find(|d| {
                d.kind == K::TypeDecl
                    && tree::name_of(d, K::Con, &source.text) == Some(entity.name.as_str())
            })
        {
            if source.target == Target::File(file)
                && decl.child_tokens().any(|t| (t.start, t.end) == *range)
            {
                range.1 = decl
                    .nodes()
                    .filter(|n| n.kind == K::TypeParam)
                    .last()
                    .map_or(range.1, |n| n.range().1);
            }
            let (start, end) = decl.range();
            return Some(
                source.text[start as usize..end as usize]
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
        let definition = definition?;
        Some(match &definition.form {
            Form::Alias { params, body, .. } => {
                let params = params.iter().fold(String::new(), |mut acc, p| {
                    let _ = write!(acc, " {p}");
                    acc
                });
                format!("type alias {}{params} = {}", entity.name, show_type(body))
            }
            Form::Error => format!("type error {}", entity.name),
            Form::Service { .. } => format!("service {}", entity.name),
            _ => format!("type {} : {}", entity.name, definition.kind),
        })
    }

    /// Markdown for the symbol at `offset`, and the range it covers.
    pub fn hover(&self, file: FileId, offset: u32) -> Option<(String, (u32, u32))> {
        if let Some((code, note, range)) = self.keyword(file, offset) {
            return Some((format!("```flick\n{code}\n```\n\n{note}"), range));
        }
        if let Some(hover) = self.embedded_hover(file, offset) {
            return Some(hover);
        }
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
                let export = interface.find(e.namespace, &e.name);
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
                let doc = source.and_then(|source| doc::comments(&source.text).remove(&e.name));
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
            } => {
                let source = self.source(module)?;
                let decl = source.tree.nodes().find(|d| {
                    d.kind == K::ServiceDecl
                        && tree::name_of(d, K::Con, &source.text) == Some(service.as_str())
                })?;
                let sig = decl
                    .nodes()
                    .filter(|n| n.kind == K::Members)
                    .flat_map(Node::nodes)
                    .find(|s| tree::name_of(s, K::Ident, &source.text) == Some(name.as_str()))?;
                let ty = sig.nodes().next()?.text(&source.text);
                let ty = ty.split_whitespace().collect::<Vec<_>>().join(" ");
                (
                    format!("{service}.{name} : {ty}"),
                    Some(format!("an operation of `{service}`")),
                )
            }
            Symbol::InstanceMethod {
                module,
                instance,
                method,
            } => (
                method.clone(),
                Some(format!("from `instance {instance}` in `{module}`")),
            ),
        };
        let mut text = format!("```flick\n{code}\n```");
        if let Some(note) = note {
            let _ = write!(text, "\n\n{note}");
        }
        Some((text, range))
    }
}
