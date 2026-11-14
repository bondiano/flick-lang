use std::collections::BTreeMap;

use super::{
    BridgeAliasId, BridgeConstructorOrigin, BridgeKind, BridgeMeaning, BridgeNode, BridgePlan,
    BridgeValidationError, BridgeWitnessId, KindTables, Requirement, Requirements, SemanticKind,
    WitnessMembers, contexts, error, merge_requirement, merge_requirements, reference,
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
        let kind_needs = self.kind_witnesses(tables, members)?;
        let mut free: Vec<Requirements> = vec![BTreeMap::new(); self.nodes.len()];
        let mut rigids = BTreeMap::new();
        for &i in order {
            let path = format!("nodes[{i}]");
            let mut needed = BTreeMap::new();
            // Exists handles body/contract separately: only its body scopes the witness.
            if let BridgeNode::Exists { witness, body } = self.nodes[i].node {
                needed = free[body.0 as usize].clone();
                needed.remove(&Requirement::Witness(witness));
                needed.remove(&Requirement::KindWitness(witness));
                self.check_witness_shadow(witness, &needed, &path)?;
                let contract = self.witnesses[witness.0 as usize].contract;
                merge_requirements(&mut needed, &free[contract.0 as usize], kinds, &path)?;
            } else {
                for (child, _) in &edges[i] {
                    merge_requirements(&mut needed, &free[*child], kinds, &path)?;
                }
            }
            let own = &self.nodes[i];
            match &own.node {
                BridgeNode::Var(name) => merge_requirement(
                    &mut needed,
                    Requirement::Variable(name.clone()),
                    Some(own.kind),
                    kinds,
                    &path,
                )?,
                BridgeNode::Con(id) => {
                    if let BridgeConstructorOrigin::Witness(witness) =
                        self.constructors[id.0 as usize].origin
                    {
                        merge_requirement(
                            &mut needed,
                            Requirement::Witness(witness),
                            None,
                            kinds,
                            &path,
                        )?;
                    }
                }
                BridgeNode::Rigid(id) => {
                    if let Some(previous) = rigids.insert(*id, own.kind) {
                        Self::same_kind(previous, own.kind, kinds, &format!("{path}.kind"))?;
                    }
                }
                BridgeNode::Forall { vars, .. } => {
                    for binder in vars {
                        if let Some(Some(required)) =
                            needed.remove(&Requirement::Variable(binder.name.clone()))
                        {
                            Self::same_kind(required, binder.kind, kinds, &format!("{path}.vars"))?;
                        }
                    }
                }
                _ => {}
            }
            for witness in &kind_needs[self.nodes[i].kind.0 as usize] {
                merge_requirement(
                    &mut needed,
                    Requirement::KindWitness(*witness),
                    None,
                    kinds,
                    &path,
                )?;
            }
            if let BridgeNode::Forall { vars, .. } = &self.nodes[i].node {
                for binder in vars {
                    for witness in &kind_needs[binder.kind.0 as usize] {
                        merge_requirement(
                            &mut needed,
                            Requirement::KindWitness(*witness),
                            None,
                            kinds,
                            &path,
                        )?;
                    }
                }
            }
            free[i] = needed;
        }
        contexts::validate(self, edges, order, kinds, &kind_needs, &free)
    }

    pub(super) fn check_witness_shadow(
        &self,
        current: BridgeWitnessId,
        needed: &Requirements,
        path: &str,
    ) -> Result<(), BridgeValidationError> {
        let binder = &self.witnesses[current.0 as usize].binder;
        for requirement in needed.keys() {
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
