use std::collections::{BTreeMap, BTreeSet};

use super::{
    BridgeConstructorOrigin, BridgeDictionaryRole, BridgeMeaning, BridgeNode, BridgePlan,
    BridgeValidationError, BridgeWitnessScope, SemanticKind, WitnessMembers,
    atomic_owned_component, error, member_module, postorder, reference,
};

impl BridgePlan {
    pub(super) fn validate_witnesses(&self) -> Result<Vec<WitnessMembers>, BridgeValidationError> {
        let mut registries = Vec::with_capacity(self.witnesses.len());
        let mut owners = vec![0_u32; self.witnesses.len()];
        for (i, witness) in self.witnesses.iter().enumerate() {
            let path = format!("witnesses[{i}]");
            if let BridgeWitnessScope::Within(parent) = witness.scope {
                reference(parent.0, self.witnesses.len(), &format!("{path}.scope"))?;
            }
            reference(
                witness.contract.0,
                self.nodes.len(),
                &format!("{path}.contract"),
            )?;
            let mut names = BTreeSet::new();
            let mut registry = WitnessMembers {
                modules: BTreeSet::new(),
                members: BTreeMap::new(),
            };
            for (j, member) in witness.members.iter().enumerate() {
                self.kind_ref(member.kind, &format!("{path}.members[{j}].kind"))?;
                if !member
                    .owner_path
                    .iter()
                    .all(|part| atomic_owned_component(part))
                {
                    return Err(error(
                        format!("{path}.members[{j}].owner_path"),
                        "invalid atomic owned module component",
                    ));
                }
                if !names.insert((&member.owner_path, &member.name)) {
                    return Err(error(
                        format!("{path}.members[{j}].name"),
                        "duplicate witness member",
                    ));
                }
                let module = member_module(&witness.binder, &member.owner_path);
                registry.modules.insert(module.clone());
                if registry
                    .members
                    .insert((module, member.name.clone()), member.kind)
                    .is_some()
                {
                    return Err(error(
                        format!("{path}.members[{j}].owner_path"),
                        "ambiguous canonical member identity",
                    ));
                }
            }
            registries.push(registry);
        }
        let parent_edges: Vec<_> = self
            .witnesses
            .iter()
            .enumerate()
            .map(|(i, witness)| match witness.scope {
                BridgeWitnessScope::Root => vec![],
                BridgeWitnessScope::Within(parent) => {
                    vec![(parent.0 as usize, format!("witnesses[{i}].scope"))]
                }
            })
            .collect();
        postorder(&parent_edges, "witness parent")?;
        for (i, node) in self.nodes.iter().enumerate() {
            if let BridgeNode::Exists { witness, .. } = node.node {
                let index = reference(
                    witness.0,
                    self.witnesses.len(),
                    &format!("nodes[{i}].witness"),
                )?;
                if owners[index] != 0 {
                    return Err(error(
                        format!("nodes[{i}].witness"),
                        "witness has multiple owning Exists nodes",
                    ));
                }
                owners[index] = 1;
            }
        }
        for (i, count) in owners.iter().enumerate() {
            if *count != 1 {
                return Err(error(
                    format!("witnesses[{i}]"),
                    "witness requires exactly one owning Exists node",
                ));
            }
        }
        Ok(registries)
    }

    pub(super) fn validate_origin(
        &self,
        i: usize,
        kinds: &[SemanticKind],
        members: &[WitnessMembers],
    ) -> Result<(), BridgeValidationError> {
        let ctor = &self.constructors[i];
        let path = format!("constructors[{i}]");
        let BridgeConstructorOrigin::Witness(id) = ctor.origin else {
            if ctor.identity.module.starts_with("$exists:") {
                return Err(error(
                    format!("{path}.origin"),
                    "reserved existential module requires witness origin",
                ));
            }
            return Ok(());
        };
        let index = reference(id.0, self.witnesses.len(), &format!("{path}.origin"))?;
        let registry = &members[index];
        if !registry.modules.contains(&ctor.identity.module) {
            return Err(error(
                format!("{path}.identity.module"),
                "witness literal module disagrees with its owned paths",
            ));
        }
        let Some(kind) = registry
            .members
            .get(&(ctor.identity.module.clone(), ctor.identity.name.clone()))
        else {
            return Err(error(
                format!("{path}.identity.name"),
                "undeclared witness member",
            ));
        };
        Self::same_kind(ctor.kind, *kind, kinds, &format!("{path}.kind"))?;
        if !matches!(
            ctor.meaning,
            BridgeMeaning::Nominal | BridgeMeaning::Alias(_)
        ) {
            return Err(error(
                format!("{path}.meaning"),
                "witness constructor must be nominal or a declared alias",
            ));
        }
        if ctor.dictionary_role != BridgeDictionaryRole::Passed {
            return Err(error(
                format!("{path}.dictionary_role"),
                "witness constructor must pass dictionaries",
            ));
        }
        if ctor.service_impl.is_some() {
            return Err(error(
                format!("{path}.service_impl"),
                "witness constructor cannot authorize service implementations",
            ));
        }
        Ok(())
    }

    pub(super) fn validate_constructors(
        &self,
        kinds: &[SemanticKind],
        members: &[WitnessMembers],
    ) -> Result<(), BridgeValidationError> {
        let mut identities = BTreeSet::new();
        for (i, ctor) in self.constructors.iter().enumerate() {
            let path = format!("constructors[{i}]");
            self.kind_ref(ctor.kind, &format!("{path}.kind"))?;
            self.validate_origin(i, kinds, members)?;
            if !identities.insert((ctor.origin, &ctor.identity.module, &ctor.identity.name)) {
                return Err(error(
                    format!("{path}.identity"),
                    "duplicate constructor identity",
                ));
            }
            if (ctor.identity.module == "$held")
                != (ctor.dictionary_role == BridgeDictionaryRole::Held)
            {
                return Err(error(
                    format!("{path}.dictionary_role"),
                    "Held role must match the exact $held namespace",
                ));
            }
            if let BridgeMeaning::Alias(id) = ctor.meaning {
                let a = reference(id.0, self.aliases.len(), &format!("{path}.meaning"))?;
                reference(
                    self.aliases[a].constructor.0,
                    self.constructors.len(),
                    &format!("aliases[{a}].constructor"),
                )?;
                if self.aliases[a].constructor.0 as usize != i {
                    return Err(error(format!("{path}.meaning"), "alias backlink disagrees"));
                }
            }
            if let Some(id) = ctor.service_impl {
                let a = reference(id.0, self.aliases.len(), &format!("{path}.service_impl"))?;
                let target = reference(
                    self.aliases[a].constructor.0,
                    self.constructors.len(),
                    &format!("aliases[{a}].constructor"),
                )?;
                if self.constructors[target].origin != BridgeConstructorOrigin::Global {
                    return Err(error(
                        format!("{path}.service_impl"),
                        "service implementation alias must be globally declared",
                    ));
                }
                let body = reference(
                    self.aliases[a].body.0,
                    self.nodes.len(),
                    &format!("aliases[{a}].body"),
                )?;
                if !matches!(self.nodes[body].node, BridgeNode::Record(_)) {
                    return Err(error(
                        format!("{path}.service_impl"),
                        "service implementation alias must have a raw record body",
                    ));
                }
            }
        }
        // kinds is deliberately supplied only after kind-DAG validation.
        debug_assert_eq!(kinds.len(), self.kinds.len());
        Ok(())
    }
}
