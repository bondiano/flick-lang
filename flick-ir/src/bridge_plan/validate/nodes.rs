use super::{
    BridgeKind, BridgeKindId, BridgeLabel, BridgeNode, BridgeNodeId, BridgePlan, BridgeRow,
    BridgeValidationError, BridgeWitnessId, CAPTURE_KINDS, SemanticKind, error, reference,
};

impl BridgePlan {
    pub(super) fn node_edges(&self) -> Result<Vec<Vec<(usize, String)>>, BridgeValidationError> {
        let mut edges = vec![vec![]; self.nodes.len()];
        for (i, ty) in self.nodes.iter().enumerate() {
            self.kind_ref(ty.kind, &format!("nodes[{i}].kind"))?;
            let mut edge = |id: BridgeNodeId, field: &str| -> Result<(), BridgeValidationError> {
                let path = format!("nodes[{i}].{field}");
                edges[i].push((reference(id.0, self.nodes.len(), &path)?, path));
                Ok(())
            };
            match &ty.node {
                BridgeNode::Con(c) => {
                    reference(
                        c.0,
                        self.constructors.len(),
                        &format!("nodes[{i}].constructor"),
                    )?;
                }
                BridgeNode::App { head, arg } => {
                    edge(*head, "head")?;
                    edge(*arg, "arg")?;
                }
                BridgeNode::Arrow {
                    param,
                    effects,
                    result,
                } => {
                    edge(*param, "param")?;
                    if let Some(e) = effects {
                        edge(e.services, "effects.services")?;
                        edge(e.errors, "effects.errors")?;
                    }
                    edge(*result, "result")?;
                }
                BridgeNode::Tuple(items) => {
                    for (j, item) in items.iter().enumerate() {
                        edge(*item, &format!("items[{j}]"))?;
                    }
                }
                BridgeNode::Record(row) | BridgeNode::Row(row) => {
                    for (j, (_, item)) in row.entries.iter().enumerate() {
                        edge(*item, &format!("entries[{j}].type"))?;
                    }
                    if let Some(tail) = row.tail {
                        edge(tail, "tail")?;
                    }
                }
                BridgeNode::Union(a, b) => {
                    edge(*a, "left")?;
                    edge(*b, "right")?;
                }
                BridgeNode::Without { row, .. } => edge(*row, "row")?,
                BridgeNode::Captured { ty, captures, .. }
                | BridgeNode::Bounded { ty, captures } => {
                    edge(*ty, "ty")?;
                    edge(*captures, "captures")?;
                }
                BridgeNode::Forall {
                    vars,
                    constraints,
                    body,
                } => {
                    self.binders(vars, &format!("nodes[{i}].vars"))?;
                    for (j, constraint) in constraints.iter().enumerate() {
                        edge(*constraint, &format!("constraints[{j}]"))?;
                    }
                    edge(*body, "body")?;
                }
                BridgeNode::Exists { witness, body } => {
                    edge(*body, "body")?;
                    let contract = self.witnesses[witness.0 as usize].contract;
                    edges[i].push((
                        contract.0 as usize,
                        format!("witnesses[{}].contract", witness.0),
                    ));
                }
                BridgeNode::Var(_) | BridgeNode::Symbol(_) | BridgeNode::Rigid(_) => {}
            }
        }
        Ok(edges)
    }

    pub(super) fn check_row(
        &self,
        row: &BridgeRow,
        kind: &BridgeKind,
        kinds: &[SemanticKind],
        classes: &[u16],
        path: &str,
    ) -> Result<(), BridgeValidationError> {
        for (j, (label, member)) in row.entries.iter().enumerate() {
            let member_path = format!("{path}.entries[{j}].type");
            match kind {
                BridgeKind::Row(k) => {
                    Self::same_kind(self.nodes[member.0 as usize].kind, *k, kinds, &member_path)?;
                }
                BridgeKind::ServiceRow => {
                    // These compiler-internal labels store captures/constraints,
                    // not service identities (regions::{live,provided}).
                    let expected = match label {
                        BridgeLabel::Live => BridgeKind::CaptureRow,
                        BridgeLabel::Provided(_) => BridgeKind::Constraint,
                        _ => BridgeKind::Service,
                    };
                    self.expect_kind(*member, &expected, &member_path)?;
                }
                BridgeKind::ErrorRow => {
                    self.expect_kind(*member, &BridgeKind::Type, &member_path)?;
                }
                // Captures may name a Region OR a Type whose captures are meant.
                // Do not invent a homogeneous Type member kind for capture rows.
                BridgeKind::CaptureRow => {
                    if classes[self.nodes[member.0 as usize].kind.0 as usize] & CAPTURE_KINDS == 0 {
                        return Err(error(&member_path, "inadmissible capture member kind"));
                    }
                }
                _ => return Err(error(path, "row node must have a row kind")),
            }
        }
        if let Some(tail) = row.tail {
            let actual = &self.kinds[self.nodes[tail.0 as usize].kind.0 as usize];
            let equal = match (kind, actual) {
                (BridgeKind::Row(a), BridgeKind::Row(b)) => {
                    kinds[a.0 as usize] == kinds[b.0 as usize]
                }
                _ => kind == actual,
            };
            if !equal {
                return Err(error(format!("{path}.tail"), "row tail kind mismatch"));
            }
        }
        Ok(())
    }

    pub(super) fn check_node(
        &self,
        i: usize,
        node: &BridgeNode,
        kinds: &[SemanticKind],
        classes: &[u16],
    ) -> Result<(), BridgeValidationError> {
        let path = format!("nodes[{i}]");
        let own = self.nodes[i].kind;
        let kind = &self.kinds[own.0 as usize];
        let same = |id: BridgeNodeId, field: &str| {
            Self::same_kind(
                own,
                self.nodes[id.0 as usize].kind,
                kinds,
                &format!("{path}.{field}"),
            )
        };
        let own_is = |expected: &BridgeKind| {
            if kind == expected {
                Ok(())
            } else {
                Err(error(format!("{path}.kind"), "unexpected kind"))
            }
        };
        match node {
            BridgeNode::Con(id) => Self::same_kind(
                own,
                self.constructors[id.0 as usize].kind,
                kinds,
                &format!("{path}.kind"),
            )?,
            BridgeNode::Symbol(_) => own_is(&BridgeKind::Symbol)?,
            BridgeNode::App { head, arg } => {
                self.check_application(*head, *arg, own, kinds, &path)?;
            }
            BridgeNode::Arrow {
                param,
                effects,
                result,
            } => {
                own_is(&BridgeKind::Type)?;
                self.check_arrow(*param, effects.as_ref(), *result, &path)?;
            }
            BridgeNode::Tuple(items) => {
                own_is(&BridgeKind::Type)?;
                for (j, item) in items.iter().enumerate() {
                    self.expect_kind(*item, &BridgeKind::Type, &format!("{path}.items[{j}]"))?;
                }
            }
            BridgeNode::Record(row) => {
                own_is(&BridgeKind::Type)?;
                self.check_record(row, &path)?;
            }
            BridgeNode::Row(row) => {
                if !Self::row_kind(kind) {
                    return Err(error(
                        format!("{path}.kind"),
                        "row node must have a row kind",
                    ));
                }
                self.check_row(row, kind, kinds, classes, &path)?;
            }
            BridgeNode::Union(a, b) => {
                if !Self::row_kind(kind) {
                    return Err(error(format!("{path}.kind"), "union must have a row kind"));
                }
                same(*a, "left")?;
                same(*b, "right")?;
            }
            BridgeNode::Without { row, .. } => {
                if !Self::row_kind(kind) {
                    return Err(error(
                        format!("{path}.kind"),
                        "subtraction must have a row kind",
                    ));
                }
                same(*row, "row")?;
            }
            BridgeNode::Captured { ty, captures, .. } | BridgeNode::Bounded { ty, captures } => {
                same(*ty, "ty")?;
                self.expect_kind(
                    *captures,
                    &BridgeKind::CaptureRow,
                    &format!("{path}.captures"),
                )?;
            }
            BridgeNode::Forall {
                constraints, body, ..
            } => {
                same(*body, "body")?;
                for (j, c) in constraints.iter().enumerate() {
                    self.expect_kind(
                        *c,
                        &BridgeKind::Constraint,
                        &format!("{path}.constraints[{j}]"),
                    )?;
                }
            }
            BridgeNode::Exists { witness, body } => {
                own_is(&BridgeKind::Type)?;
                self.check_exists(*witness, *body, own, kinds, &path)?;
            }
            BridgeNode::Var(_) | BridgeNode::Rigid(_) => {}
        }
        Ok(())
    }

    pub(super) fn check_exists(
        &self,
        witness: BridgeWitnessId,
        body: BridgeNodeId,
        own: BridgeKindId,
        kinds: &[SemanticKind],
        path: &str,
    ) -> Result<(), BridgeValidationError> {
        self.expect_kind(
            self.witnesses[witness.0 as usize].contract,
            &BridgeKind::Module,
            &format!("witnesses[{}].contract", witness.0),
        )?;
        Self::same_kind(
            own,
            self.nodes[body.0 as usize].kind,
            kinds,
            &format!("{path}.body"),
        )
    }

    pub(super) fn check_application(
        &self,
        head: BridgeNodeId,
        arg: BridgeNodeId,
        own: BridgeKindId,
        kinds: &[SemanticKind],
        path: &str,
    ) -> Result<(), BridgeValidationError> {
        let BridgeKind::Arrow(param, result) =
            self.kinds[self.nodes[head.0 as usize].kind.0 as usize]
        else {
            return Err(error(
                format!("{path}.head"),
                "application head kind is not an arrow",
            ));
        };
        Self::same_kind(
            param,
            self.nodes[arg.0 as usize].kind,
            kinds,
            &format!("{path}.arg"),
        )?;
        Self::same_kind(own, result, kinds, &format!("{path}.kind"))?;
        Ok(())
    }

    pub(super) fn check_arrow(
        &self,
        param: BridgeNodeId,
        effects: Option<&super::BridgeEffects>,
        result: BridgeNodeId,
        path: &str,
    ) -> Result<(), BridgeValidationError> {
        self.expect_kind(param, &BridgeKind::Type, &format!("{path}.param"))?;
        self.expect_kind(result, &BridgeKind::Type, &format!("{path}.result"))?;
        if let Some(e) = effects {
            self.expect_kind(
                e.services,
                &BridgeKind::ServiceRow,
                &format!("{path}.effects.services"),
            )?;
            self.expect_kind(
                e.errors,
                &BridgeKind::ErrorRow,
                &format!("{path}.effects.errors"),
            )?;
        }
        Ok(())
    }

    pub(super) fn check_record(
        &self,
        row: &BridgeRow,
        path: &str,
    ) -> Result<(), BridgeValidationError> {
        for (j, (label, member)) in row.entries.iter().enumerate() {
            if !matches!(label, BridgeLabel::Named(_)) {
                return Err(error(
                    format!("{path}.entries[{j}].label"),
                    "record field must have a named label",
                ));
            }
            self.expect_kind(
                *member,
                &BridgeKind::Type,
                &format!("{path}.entries[{j}].type"),
            )?;
        }
        if let Some(tail) = row.tail {
            let BridgeKind::Row(k) = self.kinds[self.nodes[tail.0 as usize].kind.0 as usize] else {
                return Err(error(
                    format!("{path}.tail"),
                    "record tail must have Row Type kind",
                ));
            };
            if self.kinds[k.0 as usize] != BridgeKind::Type {
                return Err(error(
                    format!("{path}.tail"),
                    "record tail must have Row Type kind",
                ));
            }
        }
        Ok(())
    }
}
