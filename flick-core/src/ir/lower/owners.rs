//! Carry inert lexical evidence at execution boundaries; activate only raw scopes.
use super::Lower;
use crate::ir::{Expr, ExprKind, Foreign, Item, ItemKind, Param, Pat, Stage, Stmt};
use crate::source::Span;
use crate::types::{Dict, ForeignOwnerNode, ForeignOwnerPolicy, Owner, StageKey};

impl Lower<'_> {
    pub(super) fn register_owner_entry(&mut self, key: StageKey) {
        if self.typing.owner_entries.contains(&key) {
            let name = self.hidden("$entry");
            self.entry_owners.insert(key, name);
        }
    }

    pub(super) fn owner_entry(&self, key: StageKey, body: Expr) -> Expr {
        let Some(name) = self.entry_owners.get(&key) else {
            return body;
        };
        let span = body.span;
        let current = Expr::new(ExprKind::ReadOwnerEvidence, span);
        Expr::new(
            ExprKind::Block(
                vec![Stmt::Let(Pat::Bind(name.clone()), current)],
                Box::new(body),
            ),
            span,
        )
    }

    pub(super) fn owner_name(&self, owner: Owner) -> String {
        match owner {
            Owner::Scope(scope) => self.owners[&scope].clone(),
            Owner::Entry(key) => self.entry_owners[&key].clone(),
        }
    }

    pub(super) fn evidence_call(&mut self, owner: Expr, call: Expr) -> Expr {
        let span = call.span;
        let mut stmts = Vec::new();
        let mut spill = |this: &mut Self, value: Expr| {
            if matches!(value.kind, ExprKind::Local(_) | ExprKind::Lit(_)) {
                value
            } else {
                let name = this.hidden("$operand");
                stmts.push(Stmt::Let(Pat::Bind(name.clone()), value));
                Expr::new(ExprKind::Local(name), span)
            }
        };
        let body = match call.kind {
            ExprKind::App {
                func,
                arg,
                stage,
                arg_first,
            } if stage != Stage::Pure => {
                let (func, arg) = if arg_first {
                    let arg = spill(self, *arg);
                    (spill(self, *func), arg)
                } else {
                    let func = spill(self, *func);
                    (func, spill(self, *arg))
                };
                Expr::new(
                    ExprKind::App {
                        func: Box::new(func),
                        arg: Box::new(arg),
                        stage,
                        arg_first,
                    },
                    span,
                )
            }
            ExprKind::Await(value) => {
                Expr::new(ExprKind::Await(Box::new(spill(self, *value))), span)
            }
            _ => return call,
        };
        let bracket = Expr::new(
            ExprKind::WithOwnerEvidence(Box::new(owner), Box::new(body)),
            span,
        );
        if stmts.is_empty() {
            bracket
        } else {
            Expr::new(ExprKind::Block(stmts, Box::new(bracket)), span)
        }
    }
    pub(super) fn scoped_foreign(
        &mut self,
        mut foreign: Foreign,
        scoped: &[bool],
        result: Option<&ForeignOwnerPolicy>,
        span: Span,
        items: &mut Vec<Item>,
    ) {
        let name = foreign.name.clone();
        let raw = self.hidden(&format!("$raw_{}", foreign.name));
        foreign.source_name = Some(name.clone());
        foreign.name.clone_from(&raw);
        let stages = foreign.stages.clone();
        let taken = foreign.taken();
        let dicts: Vec<_> = (0..foreign.dicts).map(|_| self.hidden("$dict")).collect();
        items.push(Item {
            kind: ItemKind::Foreign(foreign),
            span,
        });
        let params: Vec<_> = (0..taken).map(|_| self.hidden("$arg")).collect();
        let mut call = Expr::new(
            ExprKind::Global {
                module: self.module.to_owned(),
                name: raw,
            },
            span,
        );
        if !dicts.is_empty() {
            call = Expr::new(
                ExprKind::Dicts(
                    Box::new(call),
                    dicts.iter().cloned().map(Dict::Given).collect(),
                ),
                span,
            );
        }
        for (index, name) in params.iter().enumerate() {
            call = Expr::new(
                ExprKind::App {
                    func: Box::new(call),
                    arg: Box::new(Expr::new(ExprKind::Local(name.clone()), span)),
                    stage: if stages.get(index) == Some(&true) {
                        Stage::Runtime
                    } else {
                        Stage::Pure
                    },
                    arg_first: false,
                },
                span,
            );
        }
        // Raw host activation is inside the callable shim, so aliases and
        // stored functions retain exactly the same policy as direct calls.
        if scoped.get(taken.saturating_sub(1)) == Some(&true) {
            let owner = self.hidden("$rawOwner");
            call = Expr::new(
                ExprKind::Block(
                    vec![Stmt::Let(
                        Pat::Bind(owner.clone()),
                        Expr::new(ExprKind::ReadOwnerEvidence, span),
                    )],
                    Box::new(Expr::new(ExprKind::Within(owner, Box::new(call)), span)),
                ),
                span,
            );
        }
        if let Some(result) = result {
            call = self.foreign_result_owner(call, result);
        }
        for (index, arg) in params.into_iter().enumerate().rev() {
            call = Expr::new(
                ExprKind::Lambda {
                    params: vec![Param {
                        pat: Pat::Bind(arg),
                        stage: if stages.get(index) == Some(&true) {
                            Stage::Runtime
                        } else {
                            Stage::Pure
                        },
                        ty: None,
                    }],
                    body: Box::new(call),
                },
                span,
            );
        }
        items.push(Item {
            kind: ItemKind::Value {
                name,
                dicts,
                body: call,
            },
            span,
        });
    }
    fn foreign_result_owner(&mut self, value: Expr, policy: &ForeignOwnerPolicy) -> Expr {
        if let Some(templates) = &policy.templates {
            return self.owner_descriptor(value, templates);
        }
        self.foreign_tree_value(value, policy.root, &policy.nodes)
    }

    /// An acyclic declared policy needs no dynamic dispatcher: retaining the
    /// foreign value in a let also retains its declared calling convention.
    fn foreign_tree_value(&mut self, value: Expr, id: usize, nodes: &[ForeignOwnerNode]) -> Expr {
        let span = value.span;
        if matches!(nodes[id], ForeignOwnerNode::Identity) {
            return value;
        }
        let name = self.hidden("$returnedRaw");
        let raw = Expr::new(ExprKind::Local(name.clone()), span);
        let executable = match &nodes[id] {
            ForeignOwnerNode::Identity => unreachable!("handled before binding"),
            ForeignOwnerNode::Forall { dicts, result } => {
                let names: Vec<_> = (0..*dicts).map(|_| self.hidden("$dict")).collect();
                let instantiated = if names.is_empty() {
                    raw
                } else {
                    Expr::new(
                        ExprKind::Dicts(
                            Box::new(raw),
                            names.iter().cloned().map(Dict::Given).collect(),
                        ),
                        span,
                    )
                };
                let adapted = self.foreign_tree_value(instantiated, *result, nodes);
                if names.is_empty() {
                    adapted
                } else {
                    Expr::new(ExprKind::Abstract(names, Box::new(adapted)), span)
                }
            }
            ForeignOwnerNode::Arrow {
                scoped,
                stage,
                result,
            } => {
                let argument = self.hidden("$arg");
                let call = Expr::new(
                    ExprKind::App {
                        func: Box::new(raw),
                        arg: Box::new(Expr::new(ExprKind::Local(argument.clone()), span)),
                        stage: *stage,
                        arg_first: false,
                    },
                    span,
                );
                let call = self.foreign_raw_activation(call, *scoped);
                let body = self.foreign_tree_value(call, *result, nodes);
                Expr::new(
                    ExprKind::Lambda {
                        params: vec![Param {
                            pat: Pat::Bind(argument),
                            stage: *stage,
                            ty: None,
                        }],
                        body: Box::new(body),
                    },
                    span,
                )
            }
            ForeignOwnerNode::Task { scoped, result } => {
                let call = Expr::new(ExprKind::Await(Box::new(raw)), span);
                let call = self.foreign_raw_activation(call, *scoped);
                let body = self.foreign_tree_value(call, *result, nodes);
                Expr::new(ExprKind::Task(Box::new(body)), span)
            }
        };
        Expr::new(
            ExprKind::Block(
                vec![Stmt::Let(Pat::Bind(name), value)],
                Box::new(executable),
            ),
            span,
        )
    }

    fn foreign_raw_activation(&mut self, call: Expr, scoped: bool) -> Expr {
        if !scoped {
            return call;
        }
        let span = call.span;
        let owner = self.hidden("$rawOwner");
        Expr::new(
            ExprKind::Block(
                vec![Stmt::Let(
                    Pat::Bind(owner.clone()),
                    Expr::new(ExprKind::ReadOwnerEvidence, span),
                )],
                Box::new(Expr::new(ExprKind::Within(owner, Box::new(call)), span)),
            ),
            span,
        )
    }
}
