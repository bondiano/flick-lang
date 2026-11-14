//! Lexical owner evidence retained across higher-order value boundaries.
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::mem;

use super::{OpenScopes, OwnerTypeKind, OwnerTypeTemplates, Range, Typing};
use crate::types::env::Form;
use crate::types::lang::{self, Lang};
use crate::types::regions::scope;
use crate::types::{Checker, Effects, Stage, Type, stage_of};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageKey {
    Arrow { node: Range, index: usize },
    Task { node: Range },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Owner {
    Scope(Range),
    Entry(StageKey),
}

#[derive(Debug, Clone)]
pub(in crate::types) struct OwnerStage {
    pub key: StageKey,
    pub services: Type,
    pub source_depth: usize,
}

#[derive(Debug, Clone)]
pub(in crate::types) struct OwnerContext {
    pub scopes: OpenScopes,
    pub stages: Vec<StageKey>,
}

/// Only original foreign executable stages consume raw Scope evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignOwnerPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub templates: Option<OwnerTypeTemplates>,
    pub root: usize,
    pub nodes: Vec<ForeignOwnerNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ForeignOwnerNode {
    Identity,
    Arrow {
        scoped: bool,
        stage: Stage,
        result: usize,
    },
    Task {
        scoped: bool,
        result: usize,
    },
    Forall {
        dicts: usize,
        result: usize,
    },
}

impl Checker<'_> {
    pub(in crate::types) fn owner_context(&self) -> OwnerContext {
        OwnerContext {
            scopes: self
                .cx
                .regions
                .open
                .iter()
                .map(|&i| {
                    let scope = &self.cx.regions.scopes[i];
                    (scope.region, scope.node)
                })
                .collect(),
            stages: self.typing.owner_stack.iter().map(|s| s.key).collect(),
        }
    }

    pub(in crate::types) fn enter_owner_stage(&mut self, key: StageKey, services: Type) {
        let stage = OwnerStage {
            key,
            services,
            source_depth: self.cx.regions.open.len(),
        };
        self.typing.owner_stages.insert(key, stage.clone());
        self.typing.owner_stack.push(stage);
    }

    pub(in crate::types) fn enter_owner_arrows(
        &mut self,
        node: Range,
        stages: &[(Type, Option<Box<Effects>>)],
        last: &Type,
    ) -> usize {
        let mark = self.typing.owner_stack.len();
        for (index, (_, effects)) in stages.iter().enumerate() {
            let services = if index + 1 == stages.len() {
                Some(last.clone())
            } else {
                effects.as_ref().map(|e| e.services.clone())
            };
            if let Some(services) = services {
                self.enter_owner_stage(StageKey::Arrow { node, index }, services);
            }
        }
        mark
    }

    pub(in crate::types) fn foreign_scoped_stages(&self, body: &Type) -> Vec<bool> {
        let mut stages = Vec::new();
        let mut original = body.clone();
        loop {
            match self.whnf_captured(&original).0 {
                Type::Forall { body, .. } => original = *body,
                Type::Fun {
                    effects, result, ..
                } => {
                    stages.push(
                        effects.as_ref().is_some_and(|e| {
                            self.store.normalize(&e.services).get(scope()).is_some()
                        }),
                    );
                    original = *result;
                }
                _ => break,
            }
        }
        stages
    }

    /// A termination witness for executable-head observation only. Calling
    /// this on an inferred type does not select an owner or authorize raw
    /// activation; those policies still use original foreign declarations.
    pub(in crate::types) fn productive_type_head(&self, ty: &Type) -> bool {
        let templates = self.owner_type_templates(ty);
        super::owner_productivity::certificate(&templates)
            .is_some_and(|certificate| certificate.values[certificate.root].productive)
    }

    pub(in crate::types) fn foreign_owner_policy(
        &self,
        ty: &Type,
    ) -> Result<Option<Box<ForeignOwnerPolicy>>, ()> {
        let mut templates = self.owner_type_templates(ty);
        if templates.has_meta_views() {
            return Err(());
        }
        let has_scope = templates.exprs.iter().any(|node| match &node.kind {
            OwnerTypeKind::Con { module, name } => lang::is(Lang::Scope, module, name),
            OwnerTypeKind::Row { labels, .. } => {
                labels.iter().any(|label| label.name() == Some(scope()))
            }
            _ => false,
        });
        if !has_scope {
            return Ok(None);
        }
        let cyclic: HashSet<_> = templates
            .cyclic_aliases
            .iter()
            .map(|&id| {
                (
                    templates.aliases[id].module.clone(),
                    templates.aliases[id].name.clone(),
                )
            })
            .collect();
        let mut nodes = Vec::new();
        let Ok(root) = self.foreign_owner_node(ty, &mut nodes, &cyclic, &HashSet::new()) else {
            let certificate = super::owner_productivity::certificate(&templates).ok_or(())?;
            if !certificate.values[certificate.root].productive {
                return Ok(None);
            }
            templates.productivity = Some(certificate);
            return Ok(Some(Box::new(ForeignOwnerPolicy {
                root: 0,
                nodes: Vec::new(),
                templates: Some(templates),
            })));
        };
        Ok(nodes
            .iter()
            .any(|node| {
                matches!(
                    node,
                    ForeignOwnerNode::Arrow { scoped: true, .. }
                        | ForeignOwnerNode::Task { scoped: true, .. }
                )
            })
            .then(|| {
                Box::new(ForeignOwnerPolicy {
                    root,
                    nodes,
                    templates: None,
                })
            }))
    }

    fn foreign_owner_node(
        &self,
        ty: &Type,
        nodes: &mut Vec<ForeignOwnerNode>,
        cyclic: &HashSet<(String, String)>,
        path: &HashSet<(String, String)>,
    ) -> Result<usize, ()> {
        let (mut ty, mut aliases) = self.owner_head(ty);
        if !matches!(ty, Type::Fun { .. } | Type::Forall { .. }) {
            let (head, more) = self.owner_head(&ty);
            ty = head;
            aliases.extend(more);
        }
        let (head, args) = ty.spine();
        let task = match args[..] {
            [services, errors, result] if head.is_lang(Lang::Eff) => {
                Some((services.clone(), errors.clone(), result.clone()))
            }
            _ => None,
        };
        if !matches!(ty, Type::Fun { .. } | Type::Forall { .. }) && task.is_none() {
            // The ordinary checker head observer is bounded. A residual alias
            // is not evidence that the original declaration has no execution
            // boundary: use the finite certificate and exact lazy interpreter.
            if matches!(ty, Type::Captured { .. } | Type::Bounded { .. })
                || matches!(head, Type::Con { module, name }
                    if self.env.type_def(module, name).is_some_and(|definition|
                        matches!(&definition.form, Form::Alias { params, .. } if args.len() >= params.len())))
            {
                return Err(());
            }
            let id = nodes.len();
            nodes.push(ForeignOwnerNode::Identity);
            return Ok(id);
        }
        aliases.retain(|alias| cyclic.contains(alias));
        if aliases.iter().any(|alias| path.contains(alias)) {
            return Err(());
        }
        let mut path = path.clone();
        path.extend(aliases);
        let id = nodes.len();
        nodes.push(ForeignOwnerNode::Identity);
        nodes[id] = match ty {
            Type::Forall {
                constraints, body, ..
            } => ForeignOwnerNode::Forall {
                dicts: constraints
                    .iter()
                    .filter(|c| super::passes_dictionary(c))
                    .count(),
                result: self.foreign_owner_node(&body, nodes, cyclic, &path)?,
            },
            Type::Fun {
                effects, result, ..
            } => ForeignOwnerNode::Arrow {
                scoped: effects
                    .as_ref()
                    .is_some_and(|e| self.store.normalize(&e.services).get(scope()).is_some()),
                stage: stage_of(effects.as_deref()),
                result: self.foreign_owner_node(&result, nodes, cyclic, &path)?,
            },
            _ => {
                let (services, _, result) = task.expect("observed an executable task");
                ForeignOwnerNode::Task {
                    scoped: self.store.normalize(&services).get(scope()).is_some(),
                    result: self.foreign_owner_node(&result, nodes, cyclic, &path)?,
                }
            }
        };
        Ok(id)
    }

    fn scope_region(&self, services: &Type) -> Option<Type> {
        let row = self.store.normalize(&self.zonk(services));
        let needed = row.get(scope())?;
        match needed.spine().1[..] {
            [Type::Skolem(id)] => Some(Type::Skolem(*id)),
            _ => None,
        }
    }

    fn select_owner(
        &self,
        services: &Type,
        context: &OwnerContext,
        stages: &HashMap<StageKey, OwnerStage>,
    ) -> Option<Owner> {
        if let Some(region) = self.scope_region(services) {
            if let Some((_, node)) = context
                .scopes
                .iter()
                .find(|(r, _)| Type::Skolem(*r) == region)
            {
                return Some(Owner::Scope(*node));
            }
            if let Some(key) = context
                .stages
                .iter()
                .find(|key| self.scope_region(&stages[key].services).as_ref() == Some(&region))
            {
                return Some(Owner::Entry(*key));
            }
        }
        let needed = self.store.normalize(&self.zonk(services));
        if needed.tails.is_empty() {
            return None;
        }
        // Attribute an abstract subrow to its saved source stage, not a newer
        // higher-rank callback's dynamic candidate.
        context.stages.iter().find_map(|key| {
            let entry = self.store.normalize(&self.zonk(&stages[key].services));
            needed
                .tails
                .iter()
                .all(|tail| {
                    entry
                        .tails
                        .iter()
                        .any(|saved| row_source(tail) == row_source(saved))
                })
                .then_some(Owner::Entry(*key))
        })
    }

    pub(in crate::types) fn finish_owner_typing(&mut self, typing: &mut Typing) {
        for (at, services, context) in mem::take(&mut typing.pending_owners) {
            if let Some(owner) = self.select_owner(&services, &context, &typing.owner_stages) {
                typing.owners.insert(at, owner);
            }
        }
        let mut entries = HashSet::new();
        let mut needed = |owner| {
            if let Owner::Entry(key) = owner {
                entries.insert(key);
            }
        };
        for &owner in typing.owners.values() {
            needed(owner);
        }
        typing.owner_entries = entries;
    }
}

/// Subtracting a service changes row membership, not the source of any
/// remaining abstract Scope obligation.
fn row_source(mut row: &Type) -> &Type {
    while let Type::Without { row: inner, .. } = row {
        row = inner;
    }
    row
}
