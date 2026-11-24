//! References inside parameterized module expressions, inline modules, and
//! module signature arrows.

use super::{Local, Walker};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::patterns::{binders, is_pattern};

impl Walker<'_> {
    pub(super) fn module_parameterized(&mut self, node: &Node) {
        let mark = self.locals.len();
        let modules = self.module_locals.len();
        for parameter in node
            .nodes()
            .filter(|n| n.kind == K::ModuleParamGroup)
            .flat_map(Node::nodes)
        {
            for signature in parameter.nodes() {
                self.walk(signature);
            }
            if let Some(name) = parameter.child_tokens().find(|t| t.kind == K::Con) {
                self.module_locals.push(name.text(self.text));
            }
        }
        for result in node.nodes().filter(|n| n.kind == K::ModuleResult) {
            self.walk(result);
        }
        if let Some(body) = node.nodes().find(|n| {
            matches!(
                n.kind,
                K::Members
                    | K::ModuleApplyExpr
                    | K::ModuleParamExpr
                    | K::ModuleParenExpr
                    | K::ModuleAnnotExpr
            ) || (node.kind == K::ModuleParamExpr && n.kind == K::ModulePath)
        }) {
            for declaration in body.nodes().filter(|_| body.kind == K::Members) {
                if declaration.kind == K::ImportDecl {
                    let qualifier = declaration
                        .child_tokens()
                        .find(|t| t.kind == K::Con)
                        .or_else(|| {
                            declaration
                                .nodes()
                                .find(|n| n.kind == K::ModulePath)
                                .and_then(|p| p.child_tokens().find(|t| t.kind == K::Con))
                        });
                    if let Some(qualifier) = qualifier {
                        self.module_locals.push(qualifier.text(self.text));
                    }
                    continue;
                }
                let names = declaration
                    .child_tokens()
                    .filter(|t| matches!(t.kind, K::Ident | K::Con));
                self.locals.extend(names.map(|t| Local {
                    name: t.text(self.text),
                    generated: t.start >= self.generated,
                    binder: None,
                    mutable: None,
                    mut_fix: false,
                    used: false,
                }));
                if let Some(pattern) = declaration.nodes().find(|n| is_pattern(n.kind)) {
                    let mut names = Vec::new();
                    binders(pattern, &mut names);
                    self.locals.extend(names.into_iter().map(|t| Local {
                        name: t.text(self.text),
                        generated: t.start >= self.generated,
                        binder: None,
                        mutable: None,
                        mut_fix: false,
                        used: false,
                    }));
                }
            }
            self.walk(body);
        }
        self.module_locals.truncate(modules);
        self.locals.truncate(mark);
    }

    pub(super) fn module_signature(&mut self, node: &Node) {
        let mark = self.module_locals.len();
        let mut parts = node.nodes();
        if let Some(input) = parts.next() {
            if input.kind == K::ModuleSignatureParen
                && input.nodes().all(|n| n.kind == K::ModuleSignatureBinder)
            {
                for binder in input.nodes() {
                    for signature in binder.nodes() {
                        self.walk(signature);
                    }
                    if let Some(name) = binder.child_tokens().find(|t| t.kind == K::Con) {
                        self.module_locals.push(name.text(self.text));
                    }
                }
            } else {
                self.walk(input);
            }
        }
        for result in parts {
            self.walk(result);
        }
        self.module_locals.truncate(mark);
    }
}
