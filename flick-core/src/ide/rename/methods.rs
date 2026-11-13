//! The class identity for renaming, independent of navigation's choice of
//! a concrete instance. No dictionary spelling is used to guess a class.

use std::{collections::HashSet, ptr};

use super::{Ide, Occurrence, Symbol, tree};
use crate::diagnostics::Severity;
use crate::iface::{Decl, Entity, Interface, Namespace};
use crate::project::Analyzed;
use crate::resolve::declarations;
use crate::source::FileId;
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::{SyntaxKind as K, keyword};
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

    pub(super) fn check_method_name(&self, e: &Entity, new_name: &str) -> Result<(), String> {
        if keyword(new_name).is_some() || new_name == "_" {
            return Err(format!("`{new_name}` is not a method name"));
        }
        if new_name == e.name {
            return Ok(());
        }
        for m in &self.analysis.modules {
            // The containing file also retains child namespaces' names.
            // Only unqualified calls in this lexical module can be captured.
            let calls: HashSet<_> = m.typing.names.iter().filter_map(|(range, r)| {
                if !matches!(r, Ref::Method { module, name } if module == &e.module && name == &e.name)
                    || self.method_module(m.file, range.0).is_none_or(|owner| owner.name != m.name)
                { return None; }
                let node = tree::find(&m.tree, *range).and_then(name_expr)?;
                (!node.has_token(K::Dot)).then_some(*range)
            }).collect();
            // C(..) and (..) imports gain the new name even without a
            // written occurrence of the old one in this module.
            let imported = self.imports_value(m, &|entity| entity == e)
                || self.inherited_value(m, &|entity| entity == e);
            let exports = m.interface.exports.iter().any(|export| export.entity == *e);
            // An opening can gain the new name even when this module has
            // never imported or called the old method unqualified.
            if self.opening_captures(m, &calls, e, new_name) {
                return Err(format!(
                    "`{new_name}` would capture a reference in an opened module"
                ));
            }
            if m.name != e.module && calls.is_empty() && !imported && !exports {
                continue;
            }
            if self.existing_method_name(m, new_name) {
                return Err(format!("`{new_name}` is already in scope in `{}`", m.name));
            }
            for (start, _) in &calls {
                if super::super::locals::in_scope(&m.tree, *start, self.text(m.file))
                    .iter()
                    .any(|(name, _)| name == new_name)
                {
                    return Err(format!("`{new_name}` would capture a method reference"));
                }
            }
        }
        Ok(())
    }

    fn existing_method_name(&self, m: &Analyzed, name: &str) -> bool {
        declarations(&m.tree, self.text(m.file), m.file)
            .iter()
            .any(|d| d.namespace == Namespace::Value && d.name == name)
            || self.in_scope(m).values.iter().any(|(n, _)| n == name)
            || m.interface
                .exports
                .iter()
                .any(|e| e.entity.namespace == Namespace::Value && e.entity.name == name)
            || self.inherited_value(m, &|entity| entity.name == name)
    }

    fn imports_value(&self, m: &Analyzed, matches: &dyn Fn(&Entity) -> bool) -> bool {
        m.tree
            .nodes()
            .filter(|n| n.kind == K::ImportDecl)
            .any(|import| {
                let Some(path) = import.nodes().find(|n| n.kind == K::ModulePath) else {
                    return false;
                };
                let module: String = path.text(self.text(m.file)).split_whitespace().collect();
                self.interface(&module).is_some_and(|interface| {
                    super::super::complete::exposed(import, interface, self.text(m.file)).any(
                        |export| {
                            export.entity.namespace == Namespace::Value && matches(&export.entity)
                        },
                    )
                })
            })
    }

    fn inherited_value(&self, m: &Analyzed, matches: &dyn Fn(&Entity) -> bool) -> bool {
        // Inline children inherit imports and preceding declarations of
        // lexical parents, not siblings or declarations after the child.
        // These synthetic imports are absent from the child's source tree.
        self.analysis
            .modules
            .iter()
            .filter(|parent| {
                parent.file == m.file
                    && m.name.starts_with(&format!("{}.", parent.name))
                    && parent.tree.range().0 <= m.tree.range().0
                    && m.tree.range().1 <= parent.tree.range().1
            })
            .any(|parent| {
                self.imports_value(parent, matches)
                    || declarations(&parent.tree, self.text(parent.file), parent.file)
                        .into_iter()
                        .chain(parent.expansion.iter().flat_map(|e| e.declarations()))
                        .any(|d| {
                            d.namespace == Namespace::Value
                                && d.span.start < m.tree.range().0
                                && matches(&Entity {
                                    module: parent.name.clone(),
                                    namespace: d.namespace,
                                    name: d.name,
                                })
                        })
            })
    }

    fn method_module(&self, file: FileId, offset: u32) -> Option<&Analyzed> {
        self.analysis
            .modules
            .iter()
            .filter(|m| {
                let (start, end) = m.tree.range();
                m.file == file && start <= offset && offset <= end
            })
            .min_by_key(|m| m.tree.range().1 - m.tree.range().0)
    }

    fn opening_captures(
        &self,
        m: &Analyzed,
        calls: &HashSet<(u32, u32)>,
        e: &Entity,
        new_name: &str,
    ) -> bool {
        fn walk(
            node: &Node,
            text: &str,
            calls: &HashSet<(u32, u32)>,
            existing: &HashSet<(u32, u32)>,
            shadowed: (bool, bool),
            conflicts: &dyn Fn(&str) -> (bool, bool),
        ) -> bool {
            if node.kind == K::NameExpr
                && !node.has_token(K::Dot)
                && ((calls.contains(&node.range()) && shadowed.0)
                    || (existing.contains(&node.range()) && shadowed.1))
            {
                return true;
            }
            let body = (node.kind == K::OpenExpr)
                .then(|| node.nodes().filter(|n| n.kind != K::NameType).last())
                .flatten();
            let opening = body.map_or((false, false), |_| {
                let module = node
                    .child_tokens()
                    .filter(|t| t.kind == K::Con)
                    .map(|t| t.text(text))
                    .collect::<Vec<_>>()
                    .join(".");
                conflicts(&module)
            });
            // Plugin-generated parents may have ranges beyond source holes:
            // descend structurally rather than pruning by source position.
            node.nodes().any(|child| {
                walk(
                    child,
                    text,
                    calls,
                    existing,
                    if body.is_some_and(|body| ptr::eq(body, child)) {
                        (shadowed.0 || opening.0, shadowed.1 || opening.1)
                    } else {
                        shadowed
                    },
                    conflicts,
                )
            })
        }
        let existing: HashSet<_> = m
            .typing
            .names
            .iter()
            .filter_map(|(range, r)| {
                matches!(r, Ref::Value { name, .. } | Ref::Method { name, .. } if name == new_name)
                    .then_some(*range)
            })
            .collect();
        if calls.is_empty() && existing.is_empty() {
            return false;
        }
        let (root, text) = m
            .expansion
            .as_ref()
            .map_or((&m.tree, self.text(m.file)), |e| (&e.tree, e.text.as_str()));
        walk(root, text, calls, &existing, (false, false), &|module| {
            self.opening_interface(m, module).map_or((true, true), |i| {
                (
                    i.find(Namespace::Value, new_name).is_some(),
                    i.exports.iter().any(|export| export.entity == *e),
                )
            })
        })
    }

    pub(super) fn opening_interface<'b>(
        &'b self,
        m: &'b Analyzed,
        path: &str,
    ) -> Option<&'b Interface> {
        let parts: Vec<_> = path.split('.').collect();
        if let Some(local) = parts
            .iter()
            .try_fold(&m.interface, |i, part| i.modules.get(*part))
        {
            return Some(local);
        }
        for count in (1..=parts.len()).rev() {
            let module = self.unalias(m, parts[..count].join("."));
            let Some(interface) = self.interface(&module) else {
                continue;
            };
            if let Some(child) = parts[count..]
                .iter()
                .try_fold(interface, |i, name| i.modules.get(*name))
            {
                return Some(child);
            }
        }
        None
    }
}

fn name_expr(node: &Node) -> Option<&Node> {
    if node.kind == K::NameExpr {
        return Some(node);
    }
    node.nodes()
        .filter(|child| child.range() == node.range())
        .find_map(name_expr)
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
