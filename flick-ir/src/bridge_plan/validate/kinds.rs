use std::collections::BTreeMap;

use super::{
    BridgeBinder, BridgeConstructorOrigin, BridgeKind, BridgeKindId, BridgeNodeId, BridgePlan,
    BridgeValidationError, KindTables, Requirement, Requirements, SemanticKind, WitnessMembers,
    error, reference,
};

impl BridgePlan {
    pub(super) fn kind_ref(
        &self,
        id: BridgeKindId,
        path: &str,
    ) -> Result<(), BridgeValidationError> {
        reference(id.0, self.kinds.len(), path).map(|_| ())
    }

    pub(super) fn same_kind(
        a: BridgeKindId,
        b: BridgeKindId,
        kinds: &[SemanticKind],
        path: &str,
    ) -> Result<(), BridgeValidationError> {
        if kinds[a.0 as usize] == kinds[b.0 as usize] {
            Ok(())
        } else {
            Err(error(path, "kind mismatch"))
        }
    }

    pub(super) fn expect_kind(
        &self,
        id: BridgeNodeId,
        expected: &BridgeKind,
        path: &str,
    ) -> Result<(), BridgeValidationError> {
        if &self.kinds[self.nodes[id.0 as usize].kind.0 as usize] == expected {
            Ok(())
        } else {
            Err(error(path, "unexpected kind"))
        }
    }

    pub(super) fn row_kind(kind: &BridgeKind) -> bool {
        matches!(
            kind,
            BridgeKind::Row(_)
                | BridgeKind::ServiceRow
                | BridgeKind::ErrorRow
                | BridgeKind::CaptureRow
        )
    }

    pub(super) fn binders(
        &self,
        binders: &[BridgeBinder],
        path: &str,
    ) -> Result<BTreeMap<String, BridgeKindId>, BridgeValidationError> {
        let mut names = BTreeMap::new();
        for (i, binder) in binders.iter().enumerate() {
            self.kind_ref(binder.kind, &format!("{path}[{i}].kind"))?;
            if names.insert(binder.name.clone(), binder.kind).is_some() {
                return Err(error(format!("{path}[{i}].name"), "duplicate binder"));
            }
        }
        Ok(names)
    }

    pub(super) fn kind_witnesses(
        &self,
        tables: &KindTables,
        members: &[WitnessMembers],
        needed: &mut Requirements,
    ) -> Result<(), BridgeValidationError> {
        for &i in &tables.order {
            let mut dependencies = 0;
            match &self.kinds[i] {
                BridgeKind::Named {
                    module,
                    name,
                    origin,
                } => match origin {
                    BridgeConstructorOrigin::Global if module.starts_with("$exists:") => {
                        return Err(error(
                            format!("kinds[{i}].origin"),
                            "reserved existential module requires witness origin",
                        ));
                    }
                    BridgeConstructorOrigin::Witness(id) => {
                        let wi =
                            reference(id.0, self.witnesses.len(), &format!("kinds[{i}].origin"))?;
                        if !members[wi]
                            .members
                            .contains_key(&(module.clone(), name.clone()))
                        {
                            return Err(error(
                                format!("kinds[{i}].origin"),
                                "undeclared witness named kind",
                            ));
                        }
                        dependencies = needed.insert(
                            0,
                            &Requirement::KindWitness(*id),
                            None,
                            &format!("kinds[{i}].origin"),
                        )?;
                    }
                    BridgeConstructorOrigin::Global => {}
                },
                BridgeKind::Arrow(a, b) => {
                    dependencies =
                        needed.union(needed.kind(*a), needed.kind(*b), &format!("kinds[{i}]"))?;
                }
                BridgeKind::Row(k) => dependencies = needed.kind(*k),
                _ => {}
            }
            needed.store_kind(i, dependencies);
        }
        Ok(())
    }
}
