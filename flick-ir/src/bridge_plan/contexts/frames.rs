//! Interned lexical identities and persistent nearest-name indexes.
use std::collections::{BTreeMap, BTreeSet};

use super::{Binding, BindingId, Context, Root};
use crate::bridge_plan::persistent::Index;
use crate::bridge_plan::{BridgeNode, BridgePlan, BridgeWitnessId};

pub(super) struct Frames {
    ordinary: Vec<Option<(usize, Binding)>>,
    ordinary_ids: BTreeMap<(usize, BindingId), usize>,
    names: BTreeMap<String, usize>,
    ordinary_roots: Vec<usize>,
    ordinary_index: Index<usize>,
    witnesses: Vec<Option<(usize, BridgeWitnessId)>>,
    witness_ids: BTreeMap<(usize, BridgeWitnessId), usize>,
    witness_roots: Vec<usize>,
    witness_binders: Vec<usize>,
    witness_index: Index<BridgeWitnessId>,
    pub(super) contexts: Vec<Context>,
    context_ids: BTreeMap<Context, usize>,
}
impl Frames {
    pub(super) fn new(plan: &BridgePlan) -> Self {
        let mut names = BTreeSet::new();
        for binder in plan
            .entries
            .iter()
            .flat_map(|entry| &entry.free)
            .chain(
                plan.aliases
                    .iter()
                    .flat_map(|alias| alias.free.iter().chain(&alias.params)),
            )
            .chain(plan.nodes.iter().flat_map(|node| match &node.node {
                BridgeNode::Forall { vars, .. } => vars.as_slice(),
                _ => &[],
            }))
        {
            names.insert(binder.name.clone());
        }
        let names: BTreeMap<_, _> = names
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name, i))
            .collect();
        let ordinary_index = Index::new(names.len());
        let witness_names: BTreeMap<_, _> = plan
            .witnesses
            .iter()
            .map(|w| w.binder.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .enumerate()
            .map(|(i, name)| (name, i))
            .collect();
        let witness_index = Index::new(witness_names.len());
        let witness_binders = plan
            .witnesses
            .iter()
            .map(|w| witness_names[&w.binder])
            .collect();
        Self {
            ordinary: vec![None],
            ordinary_ids: BTreeMap::new(),
            names,
            ordinary_roots: vec![0],
            ordinary_index,
            witnesses: vec![None],
            witness_ids: BTreeMap::new(),
            witness_roots: vec![0],
            witness_binders,
            witness_index,
            contexts: vec![],
            context_ids: BTreeMap::new(),
        }
    }
    pub(super) fn context(&mut self, context: Context) -> usize {
        let next = self.contexts.len();
        *self.context_ids.entry(context).or_insert_with(|| {
            self.contexts.push(context);
            next
        })
    }
    pub(super) fn empty(&mut self, root: Root) -> usize {
        self.context(Context {
            root,
            ordinary: 0,
            witnesses: 0,
        })
    }
    pub(super) fn bind(&mut self, context: usize, binding: Binding) -> usize {
        let mut cx = self.contexts[context];
        let key = (cx.ordinary, binding.id);
        cx.ordinary = if let Some(frame) = self.ordinary_ids.get(&key) {
            *frame
        } else {
            let frame = self.ordinary.len();
            let root = self.ordinary_index.set(
                self.ordinary_roots[cx.ordinary],
                self.names[&binding.name],
                frame,
            );
            self.ordinary_roots.push(root);
            self.ordinary.push(Some((cx.ordinary, binding)));
            self.ordinary_ids.insert(key, frame);
            frame
        };
        self.context(cx)
    }
    pub(super) fn witness(&mut self, context: usize, witness: BridgeWitnessId) -> usize {
        let mut cx = self.contexts[context];
        let key = (cx.witnesses, witness);
        cx.witnesses = if let Some(frame) = self.witness_ids.get(&key) {
            *frame
        } else {
            let frame = self.witnesses.len();
            let root = self.witness_index.set(
                self.witness_roots[cx.witnesses],
                self.witness_binders[witness.0 as usize],
                witness,
            );
            self.witness_roots.push(root);
            self.witnesses.push(Some((cx.witnesses, witness)));
            self.witness_ids.insert(key, frame);
            frame
        };
        self.context(cx)
    }
    pub(super) fn lookup(&self, context: usize, name: &str) -> Option<Binding> {
        let key = *self.names.get(name)?;
        let root = self.ordinary_roots[self.contexts[context].ordinary];
        let frame = self.ordinary_index.get(root, key)?;
        self.ordinary[frame]
            .as_ref()
            .map(|(_, binding)| binding.clone())
    }
    pub(super) fn nearest(&self, context: usize) -> Option<BridgeWitnessId> {
        self.witnesses[self.contexts[context].witnesses]
            .as_ref()
            .map(|(_, id)| *id)
    }
    pub(super) fn accessible(&self, context: usize, witness: BridgeWitnessId) -> bool {
        let root = self.witness_roots[self.contexts[context].witnesses];
        self.witness_index
            .get(root, self.witness_binders[witness.0 as usize])
            == Some(witness)
    }
}

#[cfg(test)]
#[path = "frames/tests.rs"]
mod tests;
