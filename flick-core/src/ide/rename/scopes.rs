//! Name collisions and lexical capture checks for class-method renames.

use std::{collections::HashSet, ptr};

use super::{Ide, tree};
use crate::iface::{Entity, Interface, Namespace};
use crate::project::Analyzed;
use crate::resolve::declarations;
use crate::syntax::cst::Node;
use crate::syntax::kind::{SyntaxKind as K, keyword};
use crate::types::Ref;

impl Ide<'_> {
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
