//! Ordinal-key summaries share immutable arena branches. Witness support
//! aggregates are computed once per new arena node, never by rescanning roots.
#[cfg(test)]
use crate::bridge_plan::metrics;
use std::collections::{BTreeMap, BTreeSet};

use super::forest::{Forest, Support};
use super::{
    BridgeKindId, BridgeNode, BridgePlan, BridgeValidationError, BridgeWitnessId, Requirement,
    SemanticKind,
};
use crate::bridge_plan::persistent::{Index, Node, Unions};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Removed {
    Absent,
    NoKind,
    Kind(BridgeKindId),
}

pub(super) struct Requirements {
    keys: Vec<Requirement>,
    ordinals: BTreeMap<Requirement, usize>,
    index: Index<Option<BridgeKindId>>,
    roots: Vec<usize>,
    kinds: Vec<usize>,
    supports: Vec<Support>,
    direct_start: usize,
    kind_start: usize,
    forest: Forest,
    classes: Box<[SemanticKind]>,
    unions: Unions,
}
impl Requirements {
    pub(super) fn new(plan: &BridgePlan, classes: &[SemanticKind]) -> Self {
        let mut keys = BTreeSet::new();
        for node in &plan.nodes {
            if let BridgeNode::Var(name) = &node.node {
                keys.insert(Requirement::Variable(name.clone()));
            }
        }
        for id in 0..plan.witnesses.len() {
            let witness = BridgeWitnessId(u32::try_from(id).expect("validated witness IDs"));
            keys.insert(Requirement::Witness(witness));
            keys.insert(Requirement::KindWitness(witness));
        }
        let keys: Vec<_> = keys.into_iter().collect();
        let ordinals = keys
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, key)| (key, i))
            .collect();
        let direct_start = keys.partition_point(|key| matches!(key, Requirement::Variable(_)));
        let kind_start = keys.partition_point(|key| !matches!(key, Requirement::KindWitness(_)));
        Self {
            index: Index::new(keys.len()),
            keys,
            ordinals,
            roots: vec![0; plan.nodes.len()],
            kinds: vec![0; plan.kinds.len()],
            supports: vec![Support::Empty],
            direct_start,
            kind_start,
            forest: Forest::new(plan),
            classes: classes.into(),
            unions: Unions::default(),
        }
    }
    pub(super) fn root(&self, node: usize) -> usize {
        self.roots[node]
    }
    pub(super) fn store(&mut self, node: usize, root: usize) {
        self.roots[node] = root;
    }
    pub(super) fn kind(&self, kind: BridgeKindId) -> usize {
        self.kinds[kind.0 as usize]
    }
    pub(super) fn store_kind(&mut self, id: usize, root: usize) {
        self.kinds[id] = root;
    }
    pub(super) fn ordinary(
        &self,
        root: usize,
    ) -> impl Iterator<Item = (&Requirement, Option<BridgeKindId>)> {
        self.index
            .range(root, 0, self.kind_start)
            .map(|(key, value)| (&self.keys[key], value))
    }
    pub(super) fn direct(&self, root: usize) -> impl Iterator<Item = &Requirement> {
        self.index
            .range(root, self.direct_start, self.kind_start)
            .map(|(key, _)| &self.keys[key])
    }
    pub(super) fn kind_witnesses(&self, root: usize) -> impl Iterator<Item = BridgeWitnessId> {
        self.index
            .range(root, self.kind_start, self.keys.len())
            .map(|(key, _)| {
                #[cfg(test)]
                metrics::kind_enumeration();
                let Requirement::KindWitness(witness) = self.keys[key] else {
                    unreachable!("kind namespace")
                };
                witness
            })
    }
    pub(super) fn permits(&self, root: usize, current: Option<BridgeWitnessId>) -> bool {
        #[cfg(test)]
        metrics::kind_check();
        self.forest.satisfied(self.supports[root], current)
    }
    pub(super) fn contains(
        &self,
        current: Option<BridgeWitnessId>,
        target: BridgeWitnessId,
    ) -> bool {
        current.is_some_and(|current| self.forest.contains(target, current))
    }
    pub(super) fn remove(&mut self, root: usize, key: &Requirement) -> (usize, Removed) {
        let result = self
            .ordinals
            .get(key)
            .map_or((root, None), |rank| self.index.remove(root, *rank));
        self.sync();
        (
            result.0,
            result.1.map_or(Removed::Absent, |kind| {
                kind.map_or(Removed::NoKind, Removed::Kind)
            }),
        )
    }
    pub(super) fn insert(
        &mut self,
        root: usize,
        key: &Requirement,
        value: Option<BridgeKindId>,
        path: &str,
    ) -> Result<usize, BridgeValidationError> {
        let rank = self.ordinals[key];
        let value = match self.index.get(root, rank) {
            Some(old) => merge(old, value, &self.classes, path)?,
            None => value,
        };
        let root = self.index.set(root, rank, value);
        self.sync();
        Ok(root)
    }
    pub(super) fn union(
        &mut self,
        left: usize,
        right: usize,
        path: &str,
    ) -> Result<usize, BridgeValidationError> {
        let root = self
            .index
            .union_fixed(left, right, &mut self.unions, &mut |old, new| {
                merge(old, new, &self.classes, path)
            })?;
        self.sync();
        Ok(root)
    }
    fn sync(&mut self) {
        for id in self.supports.len()..self.index.allocated() {
            let support = match self.index.node(id) {
                Node::Empty => Support::Empty,
                Node::Leaf(key, _) => match self.keys[key] {
                    Requirement::KindWitness(witness) => Support::Chain(witness),
                    _ => Support::Empty,
                },
                Node::Branch(left, right) => {
                    self.forest.join(self.supports[left], self.supports[right])
                }
            };
            self.supports.push(support);
        }
    }
    #[cfg(test)]
    pub(super) fn allocated(&self) -> usize {
        self.index.allocated() - 1 + self.unions.allocated()
    }
}
fn merge(
    previous: Option<BridgeKindId>,
    incoming: Option<BridgeKindId>,
    kinds: &[SemanticKind],
    path: &str,
) -> Result<Option<BridgeKindId>, BridgeValidationError> {
    if let Some(previous) = previous {
        if let Some(incoming) = incoming {
            BridgePlan::same_kind(previous, incoming, kinds, path)?;
        }
        Ok(Some(previous))
    } else {
        Ok(incoming)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge_plan::{BridgeNodeId, BridgeType, BridgeWitness, BridgeWitnessScope};

    #[test]
    fn merges_upgrade_missing_kinds_and_keep_compatible_left_representatives() {
        let plan = BridgePlan {
            kinds: vec![],
            witnesses: vec![],
            constructors: vec![],
            aliases: vec![],
            entries: vec![],
            nodes: vec![BridgeType {
                kind: BridgeKindId(0),
                node: BridgeNode::Var("x".into()),
            }],
        };
        let classes = [SemanticKind(0), SemanticKind(0), SemanticKind(1)];
        let key = Requirement::Variable("x".into());
        let mut summaries = Requirements::new(&plan, &classes);
        let none = summaries
            .insert(0, &key, None, "none")
            .expect("unknown kind");
        let left = summaries
            .insert(0, &key, Some(BridgeKindId(0)), "left")
            .expect("left");
        let right = summaries
            .insert(0, &key, Some(BridgeKindId(1)), "right")
            .expect("same semantic kind");
        let upgraded = summaries
            .union(none, right, "upgrade")
            .expect("None upgrades");
        assert_eq!(
            summaries.ordinary(upgraded).next().map(|(_, kind)| kind),
            Some(Some(BridgeKindId(1)))
        );
        assert_eq!(summaries.union(left, right, "compatible"), Ok(left));
        let conflict = summaries
            .insert(0, &key, Some(BridgeKindId(2)), "other")
            .expect("other summary");
        let error = summaries
            .union(left, conflict, "conflict")
            .expect_err("different kind");
        assert_eq!(error.path, "conflict");
        assert_eq!(error.message, "kind mismatch");
    }

    #[test]
    fn successful_union_caches_are_directional_policy_local_and_never_cache_errors() {
        let plan = BridgePlan {
            kinds: vec![],
            witnesses: vec![],
            constructors: vec![],
            aliases: vec![],
            entries: vec![],
            nodes: ["a", "z"]
                .map(|name| BridgeType {
                    kind: BridgeKindId(0),
                    node: BridgeNode::Var(name.into()),
                })
                .to_vec(),
        };
        let classes = [SemanticKind(0), SemanticKind(0), SemanticKind(1)];
        let mut summaries = Requirements::new(&plan, &classes);
        let a = Requirement::Variable("a".into());
        let z = Requirement::Variable("z".into());
        let left = summaries
            .insert(0, &a, Some(BridgeKindId(0)), "fixture")
            .expect("left");
        let right = summaries
            .insert(0, &a, Some(BridgeKindId(1)), "fixture")
            .expect("right");
        assert_eq!(summaries.union(left, right, "forward"), Ok(left));
        assert_eq!(summaries.union(right, left, "backward"), Ok(right));
        assert_eq!(summaries.union(left, right, "cached"), Ok(left));
        let mut isolated = Requirements::new(&plan, &[SemanticKind(0), SemanticKind(1)]);
        let a0 = isolated
            .insert(0, &a, Some(BridgeKindId(0)), "fixture")
            .expect("a0");
        let a1 = isolated
            .insert(0, &a, Some(BridgeKindId(1)), "fixture")
            .expect("a1");
        assert!(isolated.union(a0, a1, "different policy").is_err());
        let lhs = summaries
            .insert(left, &z, Some(BridgeKindId(0)), "fixture")
            .expect("lhs");
        let rhs = summaries
            .insert(right, &z, Some(BridgeKindId(2)), "fixture")
            .expect("rhs");
        for path in ["first failure", "second failure"] {
            let error = summaries
                .union(lhs, rhs, path)
                .expect_err("conflicting z kind");
            assert_eq!(error.path, path);
            assert_eq!(error.message, "kind mismatch");
        }
    }

    #[test]
    fn removing_a_support_recomputes_branch_aggregates_without_mutating_versions() {
        let plan = BridgePlan {
            kinds: vec![],
            constructors: vec![],
            nodes: vec![],
            aliases: vec![],
            entries: vec![],
            witnesses: (0..2)
                .map(|id| BridgeWitness {
                    binder: format!("K{id}"),
                    scope: BridgeWitnessScope::Root,
                    contract: BridgeNodeId(0),
                    members: vec![],
                })
                .collect(),
        };
        let mut summaries = Requirements::new(&plan, &[]);
        let a = Requirement::KindWitness(BridgeWitnessId(0));
        let b = Requirement::KindWitness(BridgeWitnessId(1));
        let left = summaries.insert(0, &a, None, "fixture").expect("support");
        let right = summaries.insert(0, &b, None, "fixture").expect("support");
        let both = summaries
            .union(left, right, "fixture")
            .expect("support union");
        assert!(!summaries.permits(both, Some(BridgeWitnessId(0))));
        let (without, removed) = summaries.remove(both, &b);
        assert_eq!(removed, Removed::NoKind);
        assert!(summaries.permits(without, Some(BridgeWitnessId(0))));
        assert!(!summaries.permits(both, Some(BridgeWitnessId(0))));
        assert!(summaries.permits(left, Some(BridgeWitnessId(0))));
        assert_eq!(
            summaries.kind_witnesses(both).collect::<Vec<_>>(),
            [BridgeWitnessId(0), BridgeWitnessId(1)]
        );
    }
}
