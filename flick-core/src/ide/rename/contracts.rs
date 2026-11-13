//! Structural module fields have their own identity. Until those fields
//! can be mapped, a rename must not invalidate an ascribed module result.

use super::Ide;
use crate::iface::{Entity, Interface};
use crate::project::Analyzed;
use crate::resolve::module_syntax;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Ide<'_> {
    pub(super) fn check_method_contracts(&self, e: &Entity) -> Result<(), String> {
        for m in self.analysis.modules.iter().filter(|m| !m.inline) {
            if self.method_contract_in(&m.tree, &m.name, m.file, e) {
                return Err("static module fields have no exact mapping to class methods".into());
            }
        }
        Ok(())
    }

    fn method_contract_in(&self, root: &Node, module: &str, file: FileId, e: &Entity) -> bool {
        let text = self.text(file);
        let syntax = module_syntax(root, text, file);
        if syntax
            .header
            .as_ref()
            .is_some_and(|h| h.result.is_some() || h.body.as_ref().is_some_and(ascribed))
            && self.interface(module).is_some_and(|i| contains(i, e))
        {
            return true;
        }
        if let Some(body) = syntax.header.as_ref().and_then(|h| h.body.as_ref())
            && self
                .analysis
                .module(module)
                .is_some_and(|scope| self.method_argument_in(body, scope, e, false))
        {
            return true;
        }
        for app in syntax.applications {
            // A parameter contract may require a method field even when
            // the functor's result does not re-export that method.
            if self
                .analysis
                .module(module)
                .is_some_and(|scope| self.method_argument_in(&app.expression, scope, e, false))
            {
                return true;
            }
            if (app.annotation.is_some() || ascribed(&app.expression))
                && self
                    .interface(module)
                    .and_then(|i| {
                        app.name
                            .split('.')
                            .try_fold(i, |i, part| i.modules.get(part))
                    })
                    .is_some_and(|i| contains(i, e))
            {
                return true;
            }
        }
        for decl in root.nodes().filter(|n| n.kind == K::InlineModuleDecl) {
            let Some(path) = decl.nodes().find(|n| n.kind == K::ModulePath) else {
                continue;
            };
            let local: String = path.text(text).split_whitespace().collect();
            let child = format!("{module}.{local}");
            if decl.nodes().any(|n| n.kind == K::ModuleResult)
                && self.interface(&child).is_some_and(|i| contains(i, e))
            {
                return true;
            }
            if decl
                .nodes()
                .filter(|n| n.kind == K::Members)
                .any(|n| self.method_contract_in(n, &child, file, e))
            {
                return true;
            }
        }
        false
    }

    fn method_argument_in(
        &self,
        node: &Node,
        scope: &Analyzed,
        e: &Entity,
        argument: bool,
    ) -> bool {
        match node.kind {
            K::ModulePath if argument => {
                let path: String = node
                    .text(self.text(scope.file))
                    .split_whitespace()
                    .collect();
                self.opening_interface(scope, &path)
                    .is_some_and(|i| contains(i, e))
            }
            K::ModuleApplyExpr => node
                .nodes()
                .enumerate()
                .any(|(i, child)| self.method_argument_in(child, scope, e, argument || i > 0)),
            K::ModuleParenExpr | K::ModuleAnnotExpr => node
                .nodes()
                .next()
                .is_some_and(|child| self.method_argument_in(child, scope, e, argument)),
            K::ModuleParamGroup | K::ModuleResult => false,
            _ => node
                .nodes()
                .any(|child| self.method_argument_in(child, scope, e, argument)),
        }
    }
}

fn ascribed(node: &Node) -> bool {
    node.kind == K::ModuleAnnotExpr || node.nodes().any(ascribed)
}

fn contains(interface: &Interface, e: &Entity) -> bool {
    interface.exports.iter().any(|export| export.entity == *e)
        || interface.modules.values().any(|i| contains(i, e))
}
