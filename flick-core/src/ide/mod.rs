//! Editor queries over one [`Analysis`]: what a position names, where it
//! is declared, its source type, its references, renames and completions.
//! Positions are UTF-8 byte offsets; the language server converts them.

mod actions;
mod cli;
mod complete;
mod enrich;
mod fields;
mod hints;
mod hover;
mod locals;
mod navigate;
mod rename;
mod sql;
mod sql_table;
mod symbols;
mod tokens;
mod tree;

pub use actions::Action;
pub use complete::{Completion, CompletionKind};
pub use hints::Help;
pub use symbols::Outline;
pub use tokens::TOKEN_TYPES;

use std::path::PathBuf;

use crate::iface::{Entity, Interface, Namespace};
use crate::project::{Analysis, Analyzed};
use crate::resolve::declarations;
use crate::source::{FileId, SourceMap};
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{self, Dict, Ref};

/// Where a declaration is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A file of the analysis.
    File(FileId),
    /// The source of a built-in module, which the compiler carries.
    Builtin(String),
    /// A file of an installed package: its `.fl` or its `.iface`.
    Path(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub target: Target,
    pub start: u32,
    pub end: u32,
}

/// What a name refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Symbol {
    Entity(Entity),
    /// A module qualifier or an import/module path.
    Module(String),
    /// An operation of the service `service` of `module`.
    Operation {
        module: String,
        service: String,
        name: String,
    },
    /// A method of the instance `instance` (its head as written) of `module`.
    InstanceMethod {
        module: String,
        instance: String,
        method: String,
    },
    /// A local, by its binder.
    Local {
        file: FileId,
        binder: (u32, u32),
        name: String,
    },
}

pub struct Ide<'a> {
    pub analysis: &'a Analysis,
    pub sources: &'a SourceMap,
    /// The directory of an installed package.
    pub package_dir: &'a dyn Fn(&str) -> Option<PathBuf>,
}

impl Ide<'_> {
    fn module(&self, file: FileId) -> Option<&Analyzed> {
        self.analysis.by_file(file)
    }

    fn text(&self, file: FileId) -> &str {
        &self.sources.get(file).text
    }

    /// The interface of any module the analysis knows.
    pub fn interface(&self, module: &str) -> Option<Interface> {
        if let Some(m) = self.analysis.module(module) {
            return Some(m.interface.clone());
        }
        self.analysis
            .compilation
            .package_modules
            .iter()
            .find(|m| m.module == module)
            .map(|m| m.interface.clone())
            .or_else(|| {
                types::builtins()
                    .iter()
                    .find(|i| i.module == module)
                    .cloned()
            })
    }

    /// The package that provides `module`.
    fn package_of(&self, module: &str) -> Option<&str> {
        self.analysis
            .compilation
            .package_modules
            .iter()
            .find(|m| m.module == module)
            .map(|m| m.package.as_str())
    }

    /// The symbol at `offset` of `file`, with the range of the name there.
    pub fn symbol_at(&self, file: FileId, offset: u32) -> Option<(Symbol, (u32, u32))> {
        let m = self.module(file)?;
        let text = self.text(file);
        if let Some(symbol) = self.module_at(m, offset) {
            return Some(symbol);
        }
        if let Some(found) = self.companion_owner_at(m, offset) {
            return Some(found);
        }
        if let Some((range, r)) = tree::innermost(m.typing.names.iter(), offset)
            && let Some(node) = tree::find(&m.tree, range)
            && let Some(token) = tree::last_name(node)
            // the keyword of `schema T` stands for `T` too
            && (token.start <= offset && offset <= token.end
                || tree::path_to(&m.tree, offset)
                    .iter()
                    .any(|n| n.kind == K::SchemaExpr && n.range() == range))
        {
            let at = (token.start, token.end);
            let symbol = match r {
                Ref::Local => {
                    let name = token.text(text).to_owned();
                    let binder = locals::binder(&m.tree, range.0, &name, text)?;
                    Symbol::Local { file, binder, name }
                }
                Ref::Value { module, name } => match name.split_once('$') {
                    Some((ty, _)) => entity(module, Namespace::Type, ty),
                    None => entity(module, Namespace::Value, name),
                },
                Ref::Method { module, name } => {
                    match m.typing.dicts.get(&range).and_then(|d| d.first()) {
                        Some(Dict::Instance {
                            module: im,
                            name: instance,
                            ..
                        }) => Symbol::InstanceMethod {
                            module: im.clone(),
                            instance: instance.clone(),
                            method: name.clone(),
                        },
                        _ => entity(module, Namespace::Value, name),
                    }
                }
                Ref::Constructor(c) => entity(&c.module, Namespace::Constructor, &c.name),
                Ref::Operation { service, name, .. } => {
                    let (module, service) = service.rsplit_once('.')?;
                    Symbol::Operation {
                        module: module.to_owned(),
                        service: service.to_owned(),
                        name: name.clone(),
                    }
                }
                Ref::ServiceTag(key) => {
                    let (module, name) = key.rsplit_once('.')?;
                    entity(module, Namespace::Type, name)
                }
            };
            return Some((symbol, at));
        }
        if let Some((range, (module, name))) = tree::innermost(m.typing.type_names.iter(), offset) {
            // a class of `deriving` is a bare token, without a node
            let at = tree::find(&m.tree, range)
                .and_then(tree::last_name)
                .map_or(range, |t| (t.start, t.end));
            return Some((entity(module, Namespace::Type, name), at));
        }
        self.declared_at(m, file, offset)
    }

    /// `Public` in `Api.Public.api`: the type that owns a companion, named
    /// just before it.
    fn companion_owner_at(&self, m: &Analyzed, offset: u32) -> Option<(Symbol, (u32, u32))> {
        let (range, r) = tree::innermost(m.typing.names.iter(), offset)?;
        let Ref::Value { module, name } = r else {
            return None;
        };
        let (owner, _) = name.split_once('$')?;
        let names: Vec<_> = tree::path_to(&m.tree, offset)
            .into_iter()
            .find(|n| n.kind == K::NameExpr && n.range() == range)?
            .child_tokens()
            .filter(|t| matches!(t.kind, K::Con | K::Ident))
            .collect();
        let token = names.iter().rev().nth(1)?;
        (token.start <= offset && offset <= token.end && token.text(self.text(m.file)) == owner)
            .then(|| {
                (
                    entity(module, Namespace::Type, owner),
                    (token.start, token.end),
                )
            })
    }

    fn module_at(&self, m: &Analyzed, offset: u32) -> Option<(Symbol, (u32, u32))> {
        let text = self.text(m.file);
        for node in tree::path_to(&m.tree, offset).into_iter().rev() {
            if !node
                .child_tokens()
                .any(|t| t.kind == K::Con && t.start <= offset && offset <= t.end)
            {
                continue;
            }
            if !matches!(
                node.kind,
                K::NameExpr
                    | K::NameType
                    | K::ConPat
                    | K::SchemaExpr
                    | K::ModulePath
                    | K::ImportDecl
            ) {
                continue;
            }
            let mut names: Vec<_> = node
                .child_tokens()
                .filter(|t| matches!(t.kind, K::Con | K::Ident))
                .collect();
            if matches!(
                node.kind,
                K::NameExpr | K::NameType | K::ConPat | K::SchemaExpr
            ) {
                names.pop();
            }
            if !names.iter().any(|t| t.start <= offset && offset <= t.end) {
                continue;
            }
            let module = if node.kind == K::ImportDecl {
                node.nodes()
                    .find(|n| n.kind == K::ModulePath)?
                    .text(text)
                    .split_whitespace()
                    .collect()
            } else {
                let qualifier = names
                    .iter()
                    .map(|t| t.text(text))
                    .collect::<Vec<_>>()
                    .join(".");
                if node.kind == K::ModulePath {
                    qualifier
                } else {
                    self.unalias(m, qualifier)
                }
            };
            if self.interface(&module).is_some() {
                return Some((
                    Symbol::Module(module),
                    (names.first()?.start, names.last()?.end),
                ));
            }
            // `Api.Users.api` on `Api`: the qualifier up to the cursor, when
            // the whole one is no module
            let upto = names
                .iter()
                .position(|t| offset <= t.end)
                .map_or(names.len(), |i| i + 1);
            if upto < names.len()
                && let Some(module) = self.qualifier_module(m, &names[..upto])
            {
                return Some((
                    Symbol::Module(module),
                    (names.first()?.start, names[upto - 1].end),
                ));
            }
        }
        None
    }

    /// The module the qualifier `names` stands for, by an import's alias or
    /// as written, if the analysis knows it.
    fn qualifier_module(&self, m: &Analyzed, names: &[Token]) -> Option<String> {
        let text = self.text(m.file);
        let qualifier = names
            .iter()
            .map(|t| t.text(text))
            .collect::<Vec<_>>()
            .join(".");
        let module = self.unalias(m, qualifier);
        self.interface(&module).is_some().then_some(module)
    }

    /// The module `qualifier` stands for in `m`: the one an import names
    /// with this alias, or `qualifier` as written.
    fn unalias(&self, m: &Analyzed, qualifier: String) -> String {
        let text = self.text(m.file);
        m.tree
            .nodes()
            .find(|n| {
                n.kind == K::ImportDecl
                    && tree::name_of(n, K::Con, text) == Some(qualifier.as_str())
            })
            .and_then(|n| n.nodes().find(|p| p.kind == K::ModulePath))
            .map_or(qualifier, |p| p.text(text).split_whitespace().collect())
    }

    /// A declaration's own name, a binder, or a name in `exposing`.
    fn declared_at(&self, m: &Analyzed, file: FileId, offset: u32) -> Option<(Symbol, (u32, u32))> {
        let text = self.text(file);
        for d in declarations(&m.tree, text, file) {
            if d.span.start <= offset && offset <= d.span.end {
                return Some((
                    entity(&m.name, d.namespace, &d.name),
                    (d.span.start, d.span.end),
                ));
            }
        }
        let path = tree::path_to(&m.tree, offset);
        let node = path.last()?;
        let token = tree::name_token_at(node, offset)?;
        let at = (token.start, token.end);
        let name = token.text(text);
        if let Some(symbol) = instance_member(m, &path, token.start, name) {
            return Some((symbol, at));
        }
        match node.kind {
            K::VarPat => Some((
                Symbol::Local {
                    file,
                    binder: at,
                    name: name.to_owned(),
                },
                at,
            )),
            K::ExposeItem => {
                let namespace = if token.kind == K::Con {
                    Namespace::Type
                } else {
                    Namespace::Value
                };
                let module = path
                    .iter()
                    .rev()
                    .find(|n| n.kind == K::ImportDecl)
                    .and_then(|i| i.nodes().find(|n| n.kind == K::ModulePath))
                    .map_or_else(
                        || m.name.clone(),
                        |p| p.text(text).split_whitespace().collect(),
                    );
                // an imported name is declared where its module says
                let owner = self
                    .interface(&module)
                    .and_then(|i| i.find(namespace, name).map(|e| e.entity.module.clone()))
                    .unwrap_or(module);
                Some((entity(&owner, namespace, name), at))
            }
            _ => None,
        }
    }
}

/// A method's own name in an `instance`: at its signature or its `let`.
fn instance_member(m: &Analyzed, path: &[&Node], start: u32, name: &str) -> Option<Symbol> {
    let i = path.iter().rposition(|n| n.kind == K::Members)?;
    let decl = path[i.checked_sub(1)?];
    let member = path.get(i + 1)?;
    let own = match member.kind {
        K::Signature => member.child_tokens().find(|t| t.kind == K::Ident),
        K::LetDecl => member
            .nodes()
            .next()?
            .child_tokens()
            .find(|t| t.kind == K::Ident),
        _ => None,
    }?;
    if decl.kind != K::InstanceDecl || own.start != start {
        return None;
    }
    let info = m.typing.instances.iter().find(|i| i.span == decl.range())?;
    Some(Symbol::InstanceMethod {
        module: m.name.clone(),
        instance: info.name.clone(),
        method: name.to_owned(),
    })
}

fn entity(module: &str, namespace: Namespace, name: &str) -> Symbol {
    Symbol::Entity(Entity {
        module: module.to_owned(),
        namespace,
        name: name.to_owned(),
    })
}
#[cfg(test)]
mod companion_tests;
#[cfg(test)]
mod hints_tests;
#[cfg(test)]
mod hover_tests;
#[cfg(test)]
mod kind_hover_tests;
#[cfg(test)]
mod pattern_tests;
#[cfg(test)]
mod tests;
