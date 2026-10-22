//! Lowering of a checked module to the typed IR. The checker has already
//! accepted the module, so every name has a recorded meaning; locals are
//! renamed apart within each top-level definition, so a name never needs
//! shadowing in the generated code.

mod expr;
mod pattern;

use std::collections::{HashMap, HashSet};
use std::iter;

use super::{Expr, ExprKind, Foreign, ForeignMode, Item, ItemKind, Lit, Module, Param};
use crate::diagnostics::{Diagnostic, codes};
use crate::resolve;
use crate::source::{FileId, Span};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use crate::types::{
    Constructor, Range, Typing, class_defaults, default_method, is_type, let_parts,
};

/// Lowers `module`, which checked without errors, with what its check recorded.
pub fn lower(module: &resolve::Module, typing: &Typing) -> Result<Module, Vec<Diagnostic>> {
    let mut lower = Lower {
        module: module.name,
        text: module.text,
        file: module.file,
        typing,
        scope: Vec::new(),
        used: HashMap::new(),
        consumed: HashSet::new(),
        reuse: None,
        owners: HashMap::new(),
        diagnostics: Vec::new(),
    };
    let mut items = Vec::new();
    for decl in module.tree.nodes() {
        lower.item(decl, &mut items);
    }
    for info in &typing.instances {
        let methods = match &info.derived {
            Some(_) => Vec::new(),
            None => module
                .tree
                .nodes()
                .find(|n| n.kind == K::InstanceDecl && n.range() == info.span)
                .map(|decl| lower.methods(decl))
                .unwrap_or_default(),
        };
        let span = Span::new(module.file, info.span.0, info.span.1);
        items.push(Item {
            kind: ItemKind::Instance {
                info: Box::new(info.clone()),
                methods,
            },
            span,
        });
    }
    if lower.diagnostics.is_empty() {
        Ok(Module {
            name: module.name.to_owned(),
            items,
        })
    } else {
        Err(lower.diagnostics)
    }
}

struct Lower<'a> {
    module: &'a str,
    text: &'a str,
    file: FileId,
    typing: &'a Typing,
    /// Locals in scope: the source name and its unique name.
    scope: Vec<(String, String)>,
    /// How often each source name was bound in the current definition.
    used: HashMap<String, u32>,
    /// Ranges whose lift or dictionaries are applied already: a block and
    /// its only expression share one.
    consumed: HashSet<(Range, bool)>,
    /// While a later alternative of an or-pattern is lowered: the names
    /// the first one bound.
    reuse: Option<Vec<(String, String)>>,
    /// Each `scope` some call acquires through: the locals of its owner and
    /// of the owner around it.
    owners: HashMap<Range, (Option<String>, Option<String>)>,
    diagnostics: Vec<Diagnostic>,
}

impl Lower<'_> {
    fn span(&self, node: &Node) -> Span {
        let (start, end) = node.range();
        Span::new(self.file, start, end)
    }

    fn token_text(&self, node: &Node, kind: K) -> Option<String> {
        node.child_tokens()
            .find(|t| t.kind == kind)
            .map(|t| t.text(self.text).to_owned())
    }

    /// Binds a source name to a fresh unique name.
    fn bind(&mut self, name: &str) -> String {
        let count = self.used.entry(name.to_owned()).or_insert(0);
        let unique = if *count == 0 {
            name.to_owned()
        } else {
            format!("{name}${count}")
        };
        *count += 1;
        self.scope.push((name.to_owned(), unique.clone()));
        unique
    }

    /// A fresh unique name no source name refers to.
    fn hidden(&mut self, name: &str) -> String {
        let unique = self.bind(name);
        self.scope.pop();
        unique
    }

    fn local(&self, name: &str) -> Option<&str> {
        self.scope
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, u)| u.as_str())
    }

    fn givens(&self, node: &Node) -> Vec<String> {
        self.typing
            .givens
            .get(&node.range())
            .cloned()
            .unwrap_or_default()
    }

    /// A definition starts with nothing in scope.
    fn fresh_definition(&mut self) {
        self.scope.clear();
        self.used.clear();
    }

    fn item(&mut self, decl: &Node, items: &mut Vec<Item>) {
        let span = self.span(decl);
        match decl.kind {
            K::LetDecl => {
                self.fresh_definition();
                let parts = let_parts(decl, self.text);
                let kind = match (&parts.name, parts.pattern) {
                    (Some(name), _) => ItemKind::Value {
                        name: name.clone(),
                        dicts: self.givens(decl),
                        body: self.definition(decl),
                    },
                    (None, Some(pattern)) => {
                        let body = self.definition(decl);
                        let pattern = self.pattern(pattern);
                        ItemKind::Destructure { pattern, body }
                    }
                    (None, None) => return,
                };
                items.push(Item { kind, span });
            }
            K::RecGroup => {
                for member in decl
                    .nodes()
                    .filter(|n| n.kind == K::Members)
                    .flat_map(Node::nodes)
                {
                    self.item(member, items);
                }
            }
            K::ForeignDecl => {
                if let Some(foreign) = self.foreign(decl) {
                    items.push(Item {
                        kind: ItemKind::Foreign(foreign),
                        span,
                    });
                }
            }
            K::TypeDecl | K::ErrorDecl => self.type_items(decl, span, items),
            K::ClassDecl => {
                let Some(class) = self.token_text(decl, K::Con) else {
                    return;
                };
                for (method, member) in class_defaults(decl, self.text) {
                    self.fresh_definition();
                    let kind = ItemKind::Value {
                        name: default_method(&class, &method),
                        dicts: self.givens(member),
                        body: self.definition(member),
                    };
                    items.push(Item {
                        kind,
                        span: self.span(member),
                    });
                }
            }
            K::ServiceDecl => {
                if let Some(name) = self.token_text(decl, K::Con) {
                    let key = format!("{}.{name}", self.module);
                    items.push(Item {
                        kind: ItemKind::Service { name, key },
                        span,
                    });
                }
            }
            _ => {}
        }
    }

    /// The constructors of a type or error, its refinement check and its
    /// schema witness.
    fn type_items(&mut self, decl: &Node, span: Span, items: &mut Vec<Item>) {
        let Some(ty) = self.token_text(decl, K::Con) else {
            return;
        };
        if let Some(predicate) = decl.nodes().find(|n| n.kind == K::Refinement)
            && let Some(check) = predicate.nodes().next()
        {
            self.fresh_definition();
            let check = self.expr(check);
            items.push(Item {
                kind: ItemKind::Refined {
                    name: ty.clone(),
                    check,
                },
                span,
            });
        }
        if let Some(schema) = self.typing.schemas.iter().find(|s| s.decl == decl.range()) {
            items.push(Item {
                kind: ItemKind::Schema(schema.clone()),
                span,
            });
        }
        for variant in decl.nodes().filter(|n| n.kind == K::Variant) {
            let Some(name) = self.token_text(variant, K::Con) else {
                continue;
            };
            let constructor = Constructor {
                module: self.module.to_owned(),
                ty: ty.clone(),
                name,
                arity: variant.nodes().filter(|n| is_type(n.kind)).count(),
            };
            items.push(Item {
                kind: ItemKind::Constructor(constructor),
                span: self.span(variant),
            });
        }
        // `type error E` without fields still has its constructor
        if decl.kind == K::ErrorDecl && !decl.nodes().any(|n| n.kind == K::Variant) {
            let constructor = Constructor {
                module: self.module.to_owned(),
                ty: ty.clone(),
                name: ty,
                arity: 0,
            };
            items.push(Item {
                kind: ItemKind::Constructor(constructor),
                span,
            });
        }
    }

    /// The value a `let` defines: a function of its parameters, or its body.
    fn definition(&mut self, decl: &Node) -> Expr {
        let parts = let_parts(decl, self.text);
        let span = self.span(decl);
        let Some(body) = parts.body else {
            return Expr::new(ExprKind::Lit(Lit::Unit), span);
        };
        if parts.params.is_empty() {
            return self.expr(body);
        }
        let stages = self.typing.stages.get(&decl.range()).cloned();
        self.function(&parts.params, body, stages, span)
    }

    fn function(
        &mut self,
        params: &[&Node],
        body: &Node,
        stages: Option<Vec<bool>>,
        span: Span,
    ) -> Expr {
        let mark = self.scope.len();
        let stages = stages.unwrap_or_else(|| vec![false; params.len()]);
        let params = params
            .iter()
            .zip(stages.iter().copied().chain(iter::repeat(false)))
            .map(|(p, runtime)| Param {
                pat: self.pattern(p),
                runtime,
            })
            .collect();
        let body = self.expr(body);
        self.scope.truncate(mark);
        Expr::new(
            ExprKind::Lambda {
                params,
                body: Box::new(body),
            },
            span,
        )
    }

    /// The methods an instance defines.
    fn methods(&mut self, decl: &Node) -> Vec<(String, Expr)> {
        decl.nodes()
            .filter(|n| n.kind == K::Members)
            .flat_map(Node::nodes)
            .filter(|n| n.kind == K::LetDecl)
            .filter_map(|member| {
                self.fresh_definition();
                let name = let_parts(member, self.text).name?;
                let dicts = self.givens(member);
                let value = self.definition(member);
                let value = if dicts.is_empty() {
                    value
                } else {
                    let span = value.span;
                    Expr::new(ExprKind::Abstract(dicts, Box::new(value)), span)
                };
                Some((name, value))
            })
            .collect()
    }

    fn foreign(&mut self, decl: &Node) -> Option<Foreign> {
        let idents: Vec<String> = decl
            .child_tokens()
            .filter(|t| t.kind == K::Ident)
            .map(|t| t.text(self.text).to_owned())
            .collect();
        let mode = match idents.first()?.as_str() {
            "pure" => ForeignMode::Pure,
            "sync" => ForeignMode::Sync,
            "async" => ForeignMode::Async,
            _ => ForeignMode::Effect,
        };
        let name = idents.get(1)?.clone();
        let source = decl.nodes().find(|n| n.kind == K::ForeignSource)?;
        let strings: Vec<String> = source
            .child_tokens()
            .filter(|t| t.kind == K::String)
            .map(|t| literal::string_value(t.text(self.text)))
            .collect();
        let (module, export) = match &strings[..] {
            [export] => (None, export.clone()),
            [module, export] => (Some(module.clone()), export.clone()),
            _ => return None,
        };
        let info = self
            .typing
            .foreigns
            .get(&decl.range())
            .cloned()
            .unwrap_or_default();
        let catch = info.catch.and_then(|at| {
            let node = decl
                .nodes()
                .filter(|n| n.kind == K::ForeignMeta)
                .flat_map(Node::nodes)
                .flat_map(Node::nodes)
                .find(|n| n.range() == at)?;
            Some(self.expr(node))
        });
        Some(Foreign {
            name,
            mode,
            module,
            export,
            stages: info.stages.clone(),
            dicts: info.dicts,
            arity: info.js_arity(),
            info,
            catch,
        })
    }

    fn unsupported(&mut self, node: &Node) {
        let what = match node.kind {
            K::SqlExpr => "`sql` blocks",
            K::UiExpr => "`ui` blocks",
            K::CliExpr => "`cli` blocks",
            _ => "`schema` expressions",
        };
        self.diagnostics.push(Diagnostic::error(
            codes::P012,
            format!("{what} are checked, but not compiled yet"),
            self.span(node),
        ));
    }
}
