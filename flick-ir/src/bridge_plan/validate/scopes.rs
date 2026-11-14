use super::requirements::Removed;
use std::collections::BTreeMap;

use super::{
    BridgeAliasId, BridgeConstructorOrigin, BridgeKind, BridgeMeaning, BridgeNode, BridgePlan,
    BridgeValidationError, BridgeWitnessId, KindTables, Requirement, Requirements, SemanticKind,
    WitnessMembers, contexts, error, reference,
};

impl BridgePlan {
    pub(super) fn validate_aliases(
        &self,
        kinds: &[SemanticKind],
    ) -> Result<(), BridgeValidationError> {
        for (i, alias) in self.aliases.iter().enumerate() {
            let path = format!("aliases[{i}]");
            let ctor = reference(
                alias.constructor.0,
                self.constructors.len(),
                &format!("{path}.constructor"),
            )?;
            let body = reference(alias.body.0, self.nodes.len(), &format!("{path}.body"))?;
            if self.constructors[ctor].meaning
                != BridgeMeaning::Alias(BridgeAliasId(
                    u32::try_from(i).map_err(|_| error(&path, "alias ID overflow"))?,
                ))
            {
                return Err(error(
                    format!("{path}.constructor"),
                    "constructor must point back to this alias",
                ));
            }
            let params = self.binders(&alias.params, &format!("{path}.params"))?;
            let free = self.binders(&alias.free, &format!("{path}.free"))?;
            if params.keys().any(|name| free.contains_key(name)) {
                return Err(error(
                    format!("{path}.free"),
                    "free binder overlaps alias parameter",
                ));
            }
            let mut kind = self.constructors[ctor].kind;
            for (j, param) in alias.params.iter().enumerate() {
                let BridgeKind::Arrow(a, b) = self.kinds[kind.0 as usize] else {
                    return Err(error(
                        format!("{path}.params[{j}]"),
                        "alias has more parameters than constructor kind",
                    ));
                };
                Self::same_kind(a, param.kind, kinds, &format!("{path}.params[{j}].kind"))?;
                kind = b;
            }
            Self::same_kind(kind, self.nodes[body].kind, kinds, &format!("{path}.body"))?;
        }
        Ok(())
    }

    pub(super) fn validate_scopes(
        &self,
        edges: &[Vec<(usize, String)>],
        order: &[usize],
        kinds: &[SemanticKind],
        tables: &KindTables,
        members: &[WitnessMembers],
    ) -> Result<(), BridgeValidationError> {
        let mut free = Requirements::new(self, kinds);
        self.kind_witnesses(tables, members, &mut free)?;
        let mut rigids = BTreeMap::new();
        for &i in order {
            let path = format!("nodes[{i}]");
            let mut needed = 0;
            // Exists handles body/contract separately: only its body scopes the witness.
            if let BridgeNode::Exists { witness, body } = self.nodes[i].node {
                needed = free.root(body.0 as usize);
                (needed, _) = free.remove(needed, &Requirement::Witness(witness));
                (needed, _) = free.remove(needed, &Requirement::KindWitness(witness));
                self.check_witness_shadow(witness, needed, &free, &path)?;
                let contract = self.witnesses[witness.0 as usize].contract;
                needed = free.union(needed, free.root(contract.0 as usize), &path)?;
            } else {
                for (child, _) in &edges[i] {
                    needed = free.union(needed, free.root(*child), &path)?;
                }
            }
            let own = &self.nodes[i];
            match &own.node {
                BridgeNode::Var(name) => {
                    needed = free.insert(
                        needed,
                        &Requirement::Variable(name.clone()),
                        Some(own.kind),
                        &path,
                    )?;
                }
                BridgeNode::Con(id) => {
                    if let BridgeConstructorOrigin::Witness(witness) =
                        self.constructors[id.0 as usize].origin
                    {
                        needed =
                            free.insert(needed, &Requirement::Witness(witness), None, &path)?;
                    }
                }
                BridgeNode::Rigid(id) => {
                    if let Some(previous) = rigids.insert(*id, own.kind) {
                        Self::same_kind(previous, own.kind, kinds, &format!("{path}.kind"))?;
                    }
                }
                BridgeNode::Forall { vars, .. } => {
                    for binder in vars {
                        let removed;
                        (needed, removed) =
                            free.remove(needed, &Requirement::Variable(binder.name.clone()));
                        if let Removed::Kind(required) = removed {
                            Self::same_kind(required, binder.kind, kinds, &format!("{path}.vars"))?;
                        }
                    }
                }
                _ => {}
            }
            needed = free.union(needed, free.kind(self.nodes[i].kind), &path)?;
            if let BridgeNode::Forall { vars, .. } = &self.nodes[i].node {
                for binder in vars {
                    needed = free.union(needed, free.kind(binder.kind), &path)?;
                }
            }
            free.store(i, needed);
        }
        contexts::validate(self, edges, order, kinds, &free)
    }

    pub(super) fn check_witness_shadow(
        &self,
        current: BridgeWitnessId,
        needed: usize,
        free: &Requirements,
        path: &str,
    ) -> Result<(), BridgeValidationError> {
        let binder = &self.witnesses[current.0 as usize].binder;
        for requirement in free.direct(needed) {
            if let Requirement::Witness(other) = requirement
                && self.witnesses[other.0 as usize].binder == *binder
            {
                return Err(error(
                    format!("{path}.body"),
                    "different witness crosses a same-name shadow barrier",
                ));
            }
        }
        Ok(())
    }
}
