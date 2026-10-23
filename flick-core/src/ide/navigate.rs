//! Go to definition: a project module's own source, the source of a
//! built-in module the compiler carries, the published `.fl` of a package
//! or its `.iface`. A companion (`T.parse`, `T.schema`, `T$table`) leads to
//! its type, an instance's method to the instance.

use std::fs;
use std::path::PathBuf;

use super::{Ide, Location, Symbol, Target, tree};
use crate::iface::Entity;
use crate::resolve::declarations;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::parse;
use crate::types::{builtin_sources, is_type};

/// A module's source: where it is, its text and its tree.
pub struct Source {
    pub target: Target,
    pub text: String,
    pub tree: Node,
}

impl Ide<'_> {
    /// The source of `module`, if the analysis, the compiler or a package has it.
    pub fn source(&self, module: &str) -> Option<Source> {
        if let Some(m) = self.analysis.module(module) {
            return Some(Source {
                target: Target::File(m.file),
                text: self.sources.get(m.file).text.clone(),
                tree: m.tree.clone(),
            });
        }
        if let Some((_, text)) = builtin_sources().iter().find(|(n, _)| *n == module) {
            return Some(Source {
                target: Target::Builtin(module.to_owned()),
                text: (*text).to_owned(),
                tree: parse(text, FileId::default()).tree,
            });
        }
        let path = self.package_source(module)?;
        let text = fs::read_to_string(&path).ok()?;
        let tree = parse(&text, FileId::default()).tree;
        Some(Source {
            target: Target::Path(path),
            text,
            tree,
        })
    }

    /// The published `.fl` of a package's module, when it is there.
    fn package_source(&self, module: &str) -> Option<PathBuf> {
        let dir = (self.package_dir)(self.package_of(module)?)?;
        let relative = self.interface(module)?.source?;
        let path = dir.join(relative);
        path.exists().then_some(path)
    }

    /// The `.iface` of a package's module.
    fn package_iface(&self, module: &str) -> Option<PathBuf> {
        let dir = (self.package_dir)(self.package_of(module)?)?;
        let manifest: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join("package.json")).ok()?).ok()?;
        let iface_dir = manifest
            .get("flick")?
            .get("iface-dir")?
            .as_str()?
            .to_owned();
        Some(
            dir.join(iface_dir)
                .join(format!("{}.iface", module.replace('.', "/"))),
        )
    }

    pub fn definition(&self, symbol: &Symbol) -> Option<Location> {
        match symbol {
            Symbol::Module(module) => {
                let source = self.source(module)?;
                let path = source
                    .tree
                    .nodes()
                    .find(|n| n.kind == K::ModuleHeader)?
                    .nodes()
                    .find(|n| n.kind == K::ModulePath)?;
                let (start, end) = path.range();
                Some(at(source.target, start, end))
            }
            Symbol::Local { file, binder, .. } => Some(Location {
                target: Target::File(*file),
                start: binder.0,
                end: binder.1,
            }),
            Symbol::Entity(entity) => self.entity_definition(entity),
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
                let token = sig.child_tokens().find(|t| t.kind == K::Ident)?;
                Some(at(source.target, token.start, token.end))
            }
            Symbol::InstanceMethod {
                module,
                instance,
                method,
            } => self.instance_method(module, instance, method).or_else(|| {
                let class = self.interface(module).and_then(|i| {
                    i.exports
                        .iter()
                        .find(|e| e.entity.name == *method)
                        .map(|e| e.entity.clone())
                })?;
                self.entity_definition(&class)
            }),
        }
    }

    fn entity_definition(&self, entity: &Entity) -> Option<Location> {
        if let Some(source) = self.source(&entity.module) {
            let file = match source.target {
                Target::File(f) => f,
                _ => FileId::default(),
            };
            let found = declarations(&source.tree, &source.text, file)
                .into_iter()
                .find(|d| d.namespace == entity.namespace && d.name == entity.name)?;
            return Some(at(source.target, found.span.start, found.span.end));
        }
        // a package without its sources: the declaration in its interface
        let path = self.package_iface(&entity.module)?;
        let text = fs::read_to_string(&path).ok()?;
        let needle = format!("\"name\": \"{}\"", entity.name);
        let start = u32::try_from(text.find(&needle)?).ok()?;
        let len = u32::try_from(needle.len()).ok()?;
        Some(at(Target::Path(path), start, start + len))
    }

    /// The `let` of `method` in the instance `instance`, or its `deriving`.
    fn instance_method(&self, module: &str, instance: &str, method: &str) -> Option<Location> {
        let source = self.source(module)?;
        let decl = match self.analysis.module(module) {
            Some(m) => {
                let info = m.typing.instances.iter().find(|i| i.name == instance)?;
                tree::find(&m.tree, info.span)?.clone()
            }
            None => source
                .tree
                .nodes()
                .find(|d| d.kind == K::InstanceDecl && head(d, &source.text) == instance)?
                .clone(),
        };
        let member = decl
            .nodes()
            .filter(|n| n.kind == K::Members)
            .flat_map(Node::nodes)
            .filter(|n| n.kind == K::LetDecl)
            .find_map(|l| {
                let name = l
                    .nodes()
                    .next()?
                    .child_tokens()
                    .find(|t| t.kind == K::Ident)?;
                (name.text(&source.text) == method).then_some(name)
            });
        Some(match member {
            Some(t) => at(source.target, t.start, t.end),
            None => at(source.target, decl.range().0, decl.range().1),
        })
    }
}

/// The head of an instance as written, spaces collapsed.
fn head(decl: &Node, text: &str) -> String {
    decl.nodes()
        .find(|n| is_type(n.kind))
        .map(|n| {
            n.text(text)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

fn at(target: Target, start: u32, end: u32) -> Location {
    Location { target, start, end }
}
