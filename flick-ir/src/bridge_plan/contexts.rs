//! Static declaration contexts. Alias links never inherit an invocation context.
use std::collections::{BTreeMap, BTreeSet};

use super::{BridgePlan, BridgeValidationError, Requirement, Requirements, SemanticKind, error};
use crate::bridge_plan::{
    BridgeBinder, BridgeConstructorOrigin, BridgeKind, BridgeKindId, BridgeNode, BridgeWitnessId,
    BridgeWitnessScope,
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Root {
    Entry(usize),
    Alias(usize),
    Detached(usize),
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BindingId {
    Entry(usize, usize),
    GlobalFree(usize, usize),
    Param(usize, usize),
    Forall(usize, usize, usize),
}
#[derive(Clone)]
struct Binding {
    id: BindingId,
    name: String,
    kind: BridgeKindId,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Context {
    root: Root,
    ordinary: usize,
    witnesses: usize,
}
struct Frames {
    ordinary: Vec<Option<(usize, Binding)>>,
    ordinary_ids: BTreeMap<(usize, BindingId), usize>,
    lookups: BTreeMap<(usize, String), Option<Binding>>,
    witnesses: Vec<Option<(usize, BridgeWitnessId)>>,
    witness_ids: BTreeMap<(usize, BridgeWitnessId), usize>,
    contexts: Vec<Context>,
    context_ids: BTreeMap<Context, usize>,
}
impl Frames {
    fn new() -> Self {
        Self {
            ordinary: vec![None],
            ordinary_ids: BTreeMap::new(),
            lookups: BTreeMap::new(),
            witnesses: vec![None],
            witness_ids: BTreeMap::new(),
            contexts: vec![],
            context_ids: BTreeMap::new(),
        }
    }
    fn context(&mut self, context: Context) -> usize {
        let next = self.contexts.len();
        *self.context_ids.entry(context).or_insert_with(|| {
            self.contexts.push(context);
            next
        })
    }
    fn empty(&mut self, root: Root) -> usize {
        self.context(Context {
            root,
            ordinary: 0,
            witnesses: 0,
        })
    }
    fn bind(&mut self, context: usize, binding: Binding) -> usize {
        let mut cx = self.contexts[context];
        let next = self.ordinary.len();
        cx.ordinary = *self
            .ordinary_ids
            .entry((cx.ordinary, binding.id))
            .or_insert_with(|| {
                self.ordinary.push(Some((cx.ordinary, binding)));
                next
            });
        self.context(cx)
    }
    fn witness(&mut self, context: usize, witness: BridgeWitnessId) -> usize {
        let mut cx = self.contexts[context];
        let next = self.witnesses.len();
        cx.witnesses = *self
            .witness_ids
            .entry((cx.witnesses, witness))
            .or_insert_with(|| {
                self.witnesses.push(Some((cx.witnesses, witness)));
                next
            });
        self.context(cx)
    }
    fn lookup(&mut self, context: usize, name: &str) -> Option<Binding> {
        let mut current = self.contexts[context].ordinary;
        let mut path = Vec::new();
        let result = loop {
            if let Some(cached) = self.lookups.get(&(current, name.to_owned())) {
                break cached.clone();
            }
            path.push(current);
            let Some((parent, binding)) = &self.ordinary[current] else {
                break None;
            };
            if binding.name == name {
                break Some(binding.clone());
            }
            current = *parent;
        };
        for frame in path {
            self.lookups
                .insert((frame, name.to_owned()), result.clone());
        }
        result
    }
    fn nearest(&self, context: usize) -> Option<BridgeWitnessId> {
        self.witnesses[self.contexts[context].witnesses]
            .as_ref()
            .map(|(_, id)| *id)
    }
    fn contains(&self, context: usize, witness: BridgeWitnessId) -> bool {
        let mut current = self.contexts[context].witnesses;
        while let Some((parent, id)) = self.witnesses[current] {
            if id == witness {
                return true;
            }
            current = parent;
        }
        false
    }
    fn accessible(&self, plan: &BridgePlan, context: usize, witness: BridgeWitnessId) -> bool {
        let mut current = self.contexts[context].witnesses;
        while let Some((parent, id)) = self.witnesses[current] {
            if plan.witnesses[id.0 as usize].binder == plan.witnesses[witness.0 as usize].binder {
                return id == witness;
            }
            current = parent;
        }
        false
    }
}

struct Check<'a> {
    plan: &'a BridgePlan,
    edges: &'a [Vec<(usize, String)>],
    kinds: &'a [SemanticKind],
    kind_needs: &'a [BTreeSet<BridgeWitnessId>],
    free: &'a [Requirements],
    has_owner: Vec<bool>,
    before: Vec<Option<usize>>,
    aliases: Vec<bool>,
    owned_constructors: Vec<Vec<usize>>,
    owned_aliases: Vec<Vec<usize>>,
    frames: Frames,
    work: Vec<(usize, usize)>,
    seen: BTreeSet<(usize, usize)>,
}

pub(super) fn validate(
    plan: &BridgePlan,
    edges: &[Vec<(usize, String)>],
    order: &[usize],
    kinds: &[SemanticKind],
    kind_needs: &[BTreeSet<BridgeWitnessId>],
    free: &[Requirements],
) -> Result<(), BridgeValidationError> {
    let mut has_owner = vec![false; plan.nodes.len()];
    for &id in order {
        has_owner[id] = matches!(plan.nodes[id].node, BridgeNode::Exists { .. })
            || edges[id].iter().any(|(child, _)| has_owner[*child]);
    }
    let mut owned_constructors = vec![vec![]; plan.witnesses.len()];
    let mut owned_aliases = vec![vec![]; plan.witnesses.len()];
    for (id, ctor) in plan.constructors.iter().enumerate() {
        if let BridgeConstructorOrigin::Witness(witness) = ctor.origin {
            owned_constructors[witness.0 as usize].push(id);
        }
    }
    for (id, alias) in plan.aliases.iter().enumerate() {
        if let BridgeConstructorOrigin::Witness(witness) =
            plan.constructors[alias.constructor.0 as usize].origin
        {
            owned_aliases[witness.0 as usize].push(id);
        }
    }
    let mut check = Check {
        plan,
        edges,
        kinds,
        kind_needs,
        free,
        has_owner,
        before: vec![None; plan.witnesses.len()],
        aliases: vec![false; plan.aliases.len()],
        owned_constructors,
        owned_aliases,
        frames: Frames::new(),
        work: vec![],
        seen: BTreeSet::new(),
    };
    check.seed()?;
    check.drain()?;
    check.detached()?;
    check.drain()?;
    for (id, cx) in check.before.iter().enumerate() {
        if cx.is_none() {
            return Err(error(
                format!("witnesses[{id}].scope"),
                "unresolved declaration context",
            ));
        }
    }
    for (id, checked) in check.aliases.iter().enumerate() {
        if !checked {
            return Err(error(
                format!("aliases[{id}].body"),
                "unresolved alias declaration context",
            ));
        }
    }
    Ok(())
}

impl Check<'_> {
    fn seed(&mut self) -> Result<(), BridgeValidationError> {
        for (id, ctor) in self.plan.constructors.iter().enumerate() {
            if ctor.origin == BridgeConstructorOrigin::Global {
                let cx = self.frames.empty(Root::Detached(self.plan.nodes.len()));
                self.kind(ctor.kind, cx, &format!("constructors[{id}].kind"))?;
            }
        }
        for (id, alias) in self.plan.aliases.iter().enumerate() {
            if self.plan.constructors[alias.constructor.0 as usize].origin
                == BridgeConstructorOrigin::Global
            {
                self.alias(id, None)?;
            }
        }
        for (id, entry) in self.plan.entries.iter().enumerate() {
            let path = format!("entries[{id}].root");
            super::reference(entry.root.0, self.plan.nodes.len(), &path)?;
            self.plan
                .binders(&entry.free, &format!("entries[{id}].free"))?;
            let mut cx = self.frames.empty(Root::Entry(id));
            self.binder_kinds(&entry.free, cx, &format!("entries[{id}].free"))?;
            for (slot, binder) in entry.free.iter().enumerate() {
                cx = self.frames.bind(
                    cx,
                    Binding {
                        id: BindingId::Entry(id, slot),
                        name: binder.name.clone(),
                        kind: binder.kind,
                    },
                );
            }
            let path = format!("entries[{id}].root");
            self.plan
                .expect_kind(entry.root, &BridgeKind::Type, &path)?;
            self.root(entry.root.0 as usize, cx, &path)?;
        }
        Ok(())
    }
    fn root(&mut self, node: usize, cx: usize, path: &str) -> Result<(), BridgeValidationError> {
        self.requirements(&self.free[node], cx, path)?;
        if self.has_owner[node] {
            self.work.push((node, cx));
        }
        Ok(())
    }
    fn kind(&self, kind: BridgeKindId, cx: usize, path: &str) -> Result<(), BridgeValidationError> {
        for id in &self.kind_needs[kind.0 as usize] {
            if !self.frames.contains(cx, *id) {
                return Err(error(path, format!("unbound kind witness {id:?}")));
            }
        }
        Ok(())
    }
    fn binder_kinds(
        &self,
        binders: &[BridgeBinder],
        cx: usize,
        path: &str,
    ) -> Result<(), BridgeValidationError> {
        for (slot, binder) in binders.iter().enumerate() {
            self.kind(binder.kind, cx, &format!("{path}[{slot}].kind"))?;
        }
        Ok(())
    }
    fn requirements(
        &mut self,
        needed: &Requirements,
        cx: usize,
        path: &str,
    ) -> Result<(), BridgeValidationError> {
        for (requirement, kind) in needed {
            match requirement {
                Requirement::Variable(name) => {
                    let Some(binding) = self.frames.lookup(cx, name) else {
                        return Err(error(path, format!("unbound variable {name:?}")));
                    };
                    if let Some(kind) = kind {
                        BridgePlan::same_kind(binding.kind, *kind, self.kinds, path)?;
                    }
                }
                Requirement::Witness(id) if !self.frames.accessible(self.plan, cx, *id) => {
                    return Err(error(path, format!("unbound or shadowed witness {id:?}")));
                }
                Requirement::KindWitness(id) if !self.frames.contains(cx, *id) => {
                    return Err(error(path, format!("unbound kind witness {id:?}")));
                }
                Requirement::Witness(_) | Requirement::KindWitness(_) => {}
            }
        }
        Ok(())
    }
    fn alias(&mut self, id: usize, owner: Option<usize>) -> Result<(), BridgeValidationError> {
        if self.aliases[id] {
            return Ok(());
        }
        let alias = &self.plan.aliases[id];
        let mut cx = self.frames.empty(Root::Alias(id));
        if let Some(owner) = owner {
            let original = self.frames.contexts[owner];
            cx = self.frames.context(Context {
                root: Root::Alias(id),
                ordinary: 0,
                witnesses: original.witnesses,
            });
            let BridgeConstructorOrigin::Witness(witness) =
                self.plan.constructors[alias.constructor.0 as usize].origin
            else {
                unreachable!("witness alias");
            };
            let before = self.before[witness.0 as usize].expect("owner before");
            for (slot, free) in alias.free.iter().enumerate() {
                let path = format!("aliases[{id}].free[{slot}]");
                self.kind(free.kind, owner, &format!("{path}.kind"))?;
                let Some(binding) = self.frames.lookup(before, &free.name) else {
                    return Err(error(&path, "missing declaration capture"));
                };
                BridgePlan::same_kind(
                    binding.kind,
                    free.kind,
                    self.kinds,
                    &format!("{path}.kind"),
                )?;
                cx = self.frames.bind(cx, binding);
            }
        } else {
            self.binder_kinds(&alias.free, cx, &format!("aliases[{id}].free"))?;
            for (slot, free) in alias.free.iter().enumerate() {
                cx = self.frames.bind(
                    cx,
                    Binding {
                        id: BindingId::GlobalFree(id, slot),
                        name: free.name.clone(),
                        kind: free.kind,
                    },
                );
            }
        }
        self.binder_kinds(&alias.params, cx, &format!("aliases[{id}].params"))?;
        for (slot, param) in alias.params.iter().enumerate() {
            cx = self.frames.bind(
                cx,
                Binding {
                    id: BindingId::Param(id, slot),
                    name: param.name.clone(),
                    kind: param.kind,
                },
            );
        }
        self.aliases[id] = true;
        self.root(alias.body.0 as usize, cx, &format!("aliases[{id}].body"))
    }
    fn drain(&mut self) -> Result<(), BridgeValidationError> {
        while let Some((id, cx)) = self.work.pop() {
            if !self.seen.insert((id, cx)) {
                continue;
            }
            match &self.plan.nodes[id].node {
                BridgeNode::Exists { witness, body } => {
                    self.owner(id, *witness, body.0 as usize, cx)?;
                }
                BridgeNode::Forall { vars, .. } => {
                    self.binder_kinds(vars, cx, &format!("nodes[{id}].vars"))?;
                    let mut inner = cx;
                    for (slot, binder) in vars.iter().enumerate() {
                        inner = self.frames.bind(
                            inner,
                            Binding {
                                id: BindingId::Forall(cx, id, slot),
                                name: binder.name.clone(),
                                kind: binder.kind,
                            },
                        );
                    }
                    for (child, _) in &self.edges[id] {
                        if self.has_owner[*child] {
                            self.work.push((*child, inner));
                        }
                    }
                }
                _ => {
                    for (child, _) in &self.edges[id] {
                        if self.has_owner[*child] {
                            self.work.push((*child, cx));
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn owner(
        &mut self,
        node: usize,
        witness: BridgeWitnessId,
        body: usize,
        cx: usize,
    ) -> Result<(), BridgeValidationError> {
        let id = witness.0 as usize;
        if let Some(before) = self.before[id] {
            if before != cx {
                return Err(error(
                    format!("nodes[{node}].witness"),
                    "owning Exists has incompatible static contexts",
                ));
            }
            return Ok(());
        }
        let declaration = &self.plan.witnesses[id];
        let expected = match declaration.scope {
            BridgeWitnessScope::Root => None,
            BridgeWitnessScope::Within(parent) => Some(parent),
        };
        if self.frames.nearest(cx) != expected {
            return Err(error(
                format!("witnesses[{id}].scope"),
                "declaration parent disagrees with lexical context",
            ));
        }
        self.before[id] = Some(cx);
        self.requirements(
            &self.free[declaration.contract.0 as usize],
            cx,
            &format!("witnesses[{id}].contract"),
        )?;
        if self.has_owner[declaration.contract.0 as usize] {
            self.work.push((declaration.contract.0 as usize, cx));
        }
        let inside = self.frames.witness(cx, witness);
        for (slot, member) in declaration.members.iter().enumerate() {
            self.kind(
                member.kind,
                inside,
                &format!("witnesses[{id}].members[{slot}].kind"),
            )?;
        }
        for &ctor in &self.owned_constructors[id] {
            self.kind(
                self.plan.constructors[ctor].kind,
                inside,
                &format!("constructors[{ctor}].kind"),
            )?;
        }
        for alias in self.owned_aliases[id].clone() {
            self.alias(alias, Some(inside))?;
        }
        self.work.push((body, inside));
        Ok(())
    }
    fn detached(&mut self) -> Result<(), BridgeValidationError> {
        let mut incoming = vec![false; self.plan.nodes.len()];
        for edges in self.edges {
            for (child, _) in edges {
                incoming[*child] = true;
            }
        }
        let roots: BTreeSet<_> = self
            .plan
            .entries
            .iter()
            .map(|e| e.root.0 as usize)
            .chain(self.plan.aliases.iter().map(|a| a.body.0 as usize))
            .collect();
        let mut blocked = vec![false; self.plan.nodes.len()];
        let mut work: Vec<_> = self
            .plan
            .aliases
            .iter()
            .filter(|a| {
                matches!(
                    self.plan.constructors[a.constructor.0 as usize].origin,
                    BridgeConstructorOrigin::Witness(_)
                )
            })
            .map(|a| a.body.0 as usize)
            .collect();
        while let Some(id) = work.pop() {
            if blocked[id] {
                continue;
            }
            blocked[id] = true;
            work.extend(self.edges[id].iter().map(|(child, _)| *child));
        }
        for id in 0..incoming.len() {
            if !incoming[id] && !roots.contains(&id) && !blocked[id] && self.has_owner[id] {
                let cx = self.frames.empty(Root::Detached(id));
                self.root(id, cx, &format!("nodes[{id}]"))?;
            }
        }
        Ok(())
    }
}
