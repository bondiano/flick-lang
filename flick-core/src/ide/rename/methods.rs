//! The class identity for renaming, independent of navigation's choice of
//! a concrete instance. No dictionary spelling is used to guess a class.

use super::{Ide, Occurrence, Symbol, tree};
use crate::diagnostics::Severity;
use crate::iface::{Decl, Entity, Namespace};
use crate::project::Analyzed;
use crate::resolve::declarations;
use crate::source::FileId;
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Ref, Type};

type MethodAt = (Symbol, (u32, u32));

impl Ide<'_> {
    pub(super) fn method_class(&self, e: &Entity) -> Result<Option<String>, String> {
        let Some(m) = self.analysis.module(&e.module) else {
            return Ok(None);
        };
        let ds: Vec<_> = declarations(&m.tree, self.text(m.file), m.file)
            .into_iter()
            .filter(|d| d.namespace == e.namespace && d.name == e.name)
            .collect();
        if !ds.iter().any(|d| d.decl == Decl::Method) {
            return Ok(None);
        }
        match ds.as_slice() {
            [d] => Ok(d.parent.clone()),
            _ => Err(format!("`{}` has an ambiguous declaration", e.name)),
        }
    }

    pub(super) fn method_at(&self, file: FileId, offset: u32) -> Result<Option<MethodAt>, String> {
        // Inline namespaces share the file but keep their own declarations
        // and instances. The file's first analysis is not necessarily owner.
        let Some(m) = self.method_module(file, offset) else {
            return Ok(None);
        };
        if let Some((range, Ref::Method { module, name })) =
            tree::innermost(m.typing.names.iter(), offset)
            && let Some(t) = tree::find(&m.tree, range).and_then(tree::last_name)
            && t.start <= offset
            && offset <= t.end
        {
            return Ok(Some((
                super::super::entity(module, Namespace::Value, name),
                (t.start, t.end),
            )));
        }
        let mut exposed = None;
        for parent in self
            .analysis
            .modules
            .iter()
            .filter(|m| m.file == file && !m.inline)
        {
            self.inline_exposures(&parent.tree, &parent.name, file, &mut |t, e| {
                if t.start <= offset && offset <= t.end {
                    exposed = Some((e.clone(), (t.start, t.end)));
                }
            });
        }
        if let Some((e, at)) = exposed
            && self.method_class(&e)?.is_some()
        {
            return Ok(Some((Symbol::Entity(e), at)));
        }
        let path = tree::path_to(&m.tree, offset);
        let member = (|| {
            let i = path.iter().rposition(|n| n.kind == K::Members)?;
            Some((
                *path.get(i.checked_sub(1)?)?,
                member_name(path.get(i + 1)?)?,
            ))
        })();
        let Some((decl, t)) = member else {
            return Ok(None);
        };
        if !(t.start <= offset && offset <= t.end) {
            return Ok(None);
        }
        let (module, class) = match decl.kind {
            K::ClassDecl => (
                m.name.as_str(),
                tree::name_of(decl, K::Con, self.text(file)).unwrap_or_default(),
            ),
            K::InstanceDecl => {
                let mut infos = m.typing.instances.iter().filter(|i| i.span == decl.range());
                let info = infos.next().ok_or("this instance has no resolved class")?;
                if infos.next().is_some() {
                    return Err("this instance has an ambiguous class".into());
                }
                (info.class.0.as_str(), info.class.1.as_str())
            }
            _ => return Ok(None),
        };
        let e = Entity {
            module: module.to_owned(),
            namespace: Namespace::Value,
            name: t.text(self.text(file)).to_owned(),
        };
        if self.analysis.module(module).is_some()
            && self.method_class(&e)?.as_deref() != Some(class)
        {
            return Err("this member has no unambiguous class method declaration".into());
        }
        Ok(Some((Symbol::Entity(e), (t.start, t.end))))
    }

    pub(super) fn method_references(&self, e: &Entity, class: &str) -> Vec<Occurrence> {
        let mut out = Vec::new();
        for m in &self.analysis.modules {
            let text = self.text(m.file);
            for (range, r) in &m.typing.names {
                if matches!(r, Ref::Method { module, name } if module == &e.module && name == &e.name)
                    && let Some(t) = tree::find(&m.tree, *range).and_then(tree::last_name)
                {
                    out.push((m.file, (t.start, t.end)));
                }
            }
            for decl in m.tree.nodes() {
                let own = match decl.kind {
                    K::ClassDecl => {
                        m.name == e.module && tree::name_of(decl, K::Con, text) == Some(class)
                    }
                    K::InstanceDecl => m.typing.instances.iter().any(|i| {
                        i.span == decl.range() && i.class.0 == e.module && i.class.1 == class
                    }),
                    _ => false,
                };
                if own {
                    for t in decl
                        .nodes()
                        .filter(|n| n.kind == K::Members)
                        .flat_map(Node::nodes)
                        .filter_map(member_name)
                        .filter(|t| t.text(text) == e.name)
                    {
                        out.push((m.file, (t.start, t.end)));
                    }
                }
            }
            if !m.inline {
                self.inline_exposures(&m.tree, &m.name, m.file, &mut |t, export| {
                    if export == e {
                        out.push((m.file, (t.start, t.end)));
                    }
                });
            }
        }
        out
    }

    pub(super) fn check_method_mapping(&self, e: &Entity, class: &str) -> Result<(), String> {
        // Failed resolution can omit uses entirely. In particular, an
        // ambiguous import must not turn a rename into a partial edit.
        if self
            .analysis
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
        {
            return Err(
                "class method references may be incomplete or ambiguous; fix analysis errors first"
                    .into(),
            );
        }
        self.check_method_contracts(e)?;
        if self.analysis.modules.iter().any(|m| {
            m.tree.nodes().any(|n| {
                n.kind == K::InstanceDecl && !m.typing.instances.iter().any(|i| i.span == n.range())
            })
        }) {
            return Err("an instance has no exact class mapping".into());
        }
        if self.analysis.modules.iter().any(|m| {
            m.typing.instances.iter().any(|i| {
                i.class.0 == e.module
                    && i.class.1 == class
                    && !m
                        .tree
                        .nodes()
                        .any(|n| n.kind == K::InstanceDecl && n.range() == i.span)
            })
        }) {
            return Err("a generated instance has no exact source mapping for its methods".into());
        }
        for m in &self.analysis.modules {
            if m.typing
                .packs
                .values()
                .flatten()
                .any(|(_, module, name)| module == &e.module && name == &e.name)
            {
                return Err("packed module fields have no exact mapping to class methods".into());
            }
            for (range, r) in &m.typing.names {
                if matches!(r, Ref::Method { module, name } if module == &e.module && name == &e.name)
                    && tree::find(&m.tree, *range)
                        .and_then(tree::last_name)
                        .is_none_or(|t| {
                            self.text(m.file).get(t.start as usize..t.end as usize)
                                != Some(e.name.as_str())
                        })
                {
                    return Err("a method reference has no exact source mapping".into());
                }
            }
        }
        // An explicit dictionary is a structural record, not a set of
        // resolved method references. Refuse until labels have an exact
        // mapping, including records passed indirectly to C.dictionary.
        let dictionary = format!("{class}$Dict");
        if self.analysis.modules.iter().any(|m| {
            m.typing.dictionary_constructors.keys().any(|range| {
                m.typing.types.get(range).is_none_or(|ty| ty.contains(&mut |ty|
                    matches!(ty, Type::Con { module, name } if module == &e.module && name == &dictionary)))
            })
        })
        {
            return Err("explicit dictionary fields have no exact mapping to class methods".into());
        }
        Ok(())
    }

    /// Inline analyses omit their header. Its written exposures remain in
    /// the enclosing source tree; their interface supplies defining identity.
    fn inline_exposures(
        &self,
        root: &Node,
        module: &str,
        file: FileId,
        f: &mut dyn FnMut(Token, &Entity),
    ) {
        let text = self.text(file);
        for decl in root.nodes().filter(|n| n.kind == K::InlineModuleDecl) {
            let Some(path) = decl.nodes().find(|n| n.kind == K::ModulePath) else {
                continue;
            };
            let local: String = path.text(text).split_whitespace().collect();
            let child = format!("{module}.{local}");
            if let Some(interface) = self.interface(&child) {
                for item in decl
                    .nodes()
                    .filter(|n| n.kind == K::Exposing)
                    .flat_map(Node::nodes)
                {
                    if let Some(t) = item.child_tokens().find(|t| t.kind == K::Ident)
                        && let Some(export) = interface.find(Namespace::Value, t.text(text))
                    {
                        f(t, &export.entity);
                    }
                }
            }
            for members in decl.nodes().filter(|n| n.kind == K::Members) {
                self.inline_exposures(members, &child, file, f);
            }
        }
    }

    pub(super) fn method_module(&self, file: FileId, offset: u32) -> Option<&Analyzed> {
        self.analysis
            .modules
            .iter()
            .filter(|m| {
                let (start, end) = m.tree.range();
                m.file == file && start <= offset && offset <= end
            })
            .min_by_key(|m| m.tree.range().1 - m.tree.range().0)
    }
}

fn member_name(member: &Node) -> Option<Token> {
    match member.kind {
        K::Signature => member.child_tokens().find(|t| t.kind == K::Ident),
        K::LetDecl => member
            .nodes()
            .next()?
            .child_tokens()
            .find(|t| t.kind == K::Ident),
        _ => None,
    }
}
