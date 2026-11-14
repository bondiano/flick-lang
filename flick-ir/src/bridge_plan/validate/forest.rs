//! Ancestry of the already reference- and cycle-checked witness parent forest.
use super::{BridgePlan, BridgeWitnessId, BridgeWitnessScope};
#[cfg(test)]
use crate::bridge_plan::metrics;

pub(super) struct Forest {
    entered: Vec<usize>,
    exited: Vec<usize>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Support {
    Empty,
    Chain(BridgeWitnessId),
    Branched,
}
impl Forest {
    pub(super) fn new(plan: &BridgePlan) -> Self {
        let mut children = vec![vec![]; plan.witnesses.len()];
        let mut roots = vec![];
        for (id, witness) in plan.witnesses.iter().enumerate() {
            match witness.scope {
                BridgeWitnessScope::Root => roots.push(id),
                BridgeWitnessScope::Within(parent) => children[parent.0 as usize].push(id),
            }
        }
        let mut entered = vec![0; children.len()];
        let mut exited = vec![0; children.len()];
        let mut clock = 0;
        for root in roots {
            let mut pending = vec![(root, false)];
            while let Some((id, leave)) = pending.pop() {
                if leave {
                    exited[id] = clock;
                } else {
                    entered[id] = clock;
                    clock += 1;
                    pending.push((id, true));
                    pending.extend(children[id].iter().rev().map(|child| (*child, false)));
                }
            }
        }
        Self { entered, exited }
    }
    pub(super) fn contains(&self, ancestor: BridgeWitnessId, current: BridgeWitnessId) -> bool {
        #[cfg(test)]
        metrics::ancestor();
        let a = ancestor.0 as usize;
        let b = current.0 as usize;
        self.entered[a] <= self.entered[b] && self.entered[b] < self.exited[a]
    }
    pub(super) fn join(&self, left: Support, right: Support) -> Support {
        match (left, right) {
            (Support::Empty, other) | (other, Support::Empty) => other,
            (Support::Branched, _) | (_, Support::Branched) => Support::Branched,
            (Support::Chain(a), Support::Chain(b)) => {
                if self.contains(a, b) {
                    Support::Chain(b)
                } else if self.contains(b, a) {
                    Support::Chain(a)
                } else {
                    Support::Branched
                }
            }
        }
    }
    pub(super) fn satisfied(&self, support: Support, current: Option<BridgeWitnessId>) -> bool {
        match support {
            Support::Empty => true,
            Support::Chain(deepest) => {
                current.is_some_and(|current| self.contains(deepest, current))
            }
            Support::Branched => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge_plan::{BridgeNodeId, BridgeWitness};

    #[test]
    fn ancestry_and_deepest_support_follow_edges_not_id_order() {
        let parents = [Some(2), None, Some(1), None];
        let plan = BridgePlan {
            kinds: vec![],
            constructors: vec![],
            nodes: vec![],
            aliases: vec![],
            entries: vec![],
            witnesses: parents
                .into_iter()
                .enumerate()
                .map(|(i, parent)| BridgeWitness {
                    binder: format!("K{i}"),
                    scope: parent.map_or(BridgeWitnessScope::Root, |id| {
                        BridgeWitnessScope::Within(BridgeWitnessId(id))
                    }),
                    contract: BridgeNodeId(0),
                    members: vec![],
                })
                .collect(),
        };
        let forest = Forest::new(&plan);
        assert!(forest.contains(BridgeWitnessId(1), BridgeWitnessId(0)));
        assert!(!forest.contains(BridgeWitnessId(0), BridgeWitnessId(1)));
        assert_eq!(
            forest.join(
                Support::Chain(BridgeWitnessId(2)),
                Support::Chain(BridgeWitnessId(0))
            ),
            Support::Chain(BridgeWitnessId(0))
        );
        assert_eq!(
            forest.join(
                Support::Chain(BridgeWitnessId(0)),
                Support::Chain(BridgeWitnessId(3))
            ),
            Support::Branched
        );
        assert!(forest.satisfied(Support::Empty, None));
    }
}
