use std::collections::BTreeSet;

use super::{
    BridgeBinder, BridgeLabel, BridgeNode, BridgeNodeId, BridgePlan, BridgeRow,
    BridgeValidationError,
};

impl BridgePlan {
    /// Raw structural `TypeEq`, WITHOUT alias unfolding or alpha renaming.
    /// Unlike wire equality, ignores implicit kinds, provenance and declaration
    /// metadata; within effects, ignores only `native_direct`.
    /// Literal variable/rigid identities, captures/value and binders stay exact.
    pub fn raw_type_eq(
        &self,
        left: BridgeNodeId,
        right: BridgeNodeId,
    ) -> Result<bool, BridgeValidationError> {
        self.validate_structure()?;
        for (name, id) in [("left", left), ("right", right)] {
            if id.0 as usize >= self.nodes.len() {
                return Err(BridgeValidationError {
                    path: name.to_owned(),
                    message: "reference out of range".to_owned(),
                });
            }
        }
        let kinds = self.validate_kinds()?.raw;
        let mut work = vec![(left, right)];
        let mut seen = BTreeSet::new();
        while let Some((left, right)) = work.pop() {
            if left == right || !seen.insert((left, right)) {
                continue;
            }
            let (left, right) = (&self.nodes[left.0 as usize], &self.nodes[right.0 as usize]);
            if !self.equal_node(&left.node, &right.node, &kinds, &mut work) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn equal_node(
        &self,
        left: &BridgeNode,
        right: &BridgeNode,
        kinds: &[super::validate::RawKind],
        work: &mut Vec<(BridgeNodeId, BridgeNodeId)>,
    ) -> bool {
        match (left, right) {
            (BridgeNode::Con(left), BridgeNode::Con(right)) => {
                self.constructors[left.0 as usize].identity
                    == self.constructors[right.0 as usize].identity
            }
            (BridgeNode::Var(left), BridgeNode::Var(right))
            | (BridgeNode::Symbol(left), BridgeNode::Symbol(right)) => left == right,
            (BridgeNode::Rigid(left), BridgeNode::Rigid(right)) => left == right,
            (
                BridgeNode::App {
                    head: left,
                    arg: inner_left,
                },
                BridgeNode::App {
                    head: right,
                    arg: inner_right,
                },
            )
            | (BridgeNode::Union(left, inner_left), BridgeNode::Union(right, inner_right))
            | (
                BridgeNode::Bounded {
                    ty: left,
                    captures: inner_left,
                },
                BridgeNode::Bounded {
                    ty: right,
                    captures: inner_right,
                },
            ) => {
                work.extend([(*left, *right), (*inner_left, *inner_right)]);
                true
            }
            (
                BridgeNode::Arrow {
                    param: left,
                    effects: inner_left,
                    result: body_left,
                },
                BridgeNode::Arrow {
                    param: right,
                    effects: inner_right,
                    result: body_right,
                },
            ) => {
                work.extend([(*left, *right), (*body_left, *body_right)]);
                match (inner_left, inner_right) {
                    (None, None) => true,
                    (Some(inner_left), Some(inner_right))
                        if inner_left.sync == inner_right.sync =>
                    {
                        work.extend([
                            (inner_left.services, inner_right.services),
                            (inner_left.errors, inner_right.errors),
                        ]);
                        true
                    }
                    _ => false,
                }
            }
            (BridgeNode::Tuple(left), BridgeNode::Tuple(right)) => pairs(left, right, work),
            (BridgeNode::Record(left), BridgeNode::Record(right))
            | (BridgeNode::Row(left), BridgeNode::Row(right)) => rows(left, right, work),
            (
                BridgeNode::Without {
                    row: left,
                    gone: inner_left,
                },
                BridgeNode::Without {
                    row: right,
                    gone: inner_right,
                },
            ) => {
                work.push((*left, *right));
                inner_left.len() == inner_right.len()
                    && inner_left
                        .iter()
                        .zip(inner_right)
                        .all(|(left, right)| labels(left, right))
            }
            (
                BridgeNode::Exists {
                    witness: left,
                    body: left_body,
                },
                BridgeNode::Exists {
                    witness: right,
                    body: right_body,
                },
            ) => {
                let left = &self.witnesses[left.0 as usize];
                let right = &self.witnesses[right.0 as usize];
                work.extend([(left.contract, right.contract), (*left_body, *right_body)]);
                left.binder == right.binder
            }
            _ => equal_qualified(left, right, kinds, work),
        }
    }
}

fn pairs(
    left: &[BridgeNodeId],
    right: &[BridgeNodeId],
    work: &mut Vec<(BridgeNodeId, BridgeNodeId)>,
) -> bool {
    if left.len() != right.len() {
        return false;
    }
    work.extend(left.iter().copied().zip(right.iter().copied()));
    true
}
fn binders(
    left: &[BridgeBinder],
    right: &[BridgeBinder],
    kinds: &[super::validate::RawKind],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.name == right.name && kinds[left.kind.0 as usize] == kinds[right.kind.0 as usize]
        })
}
fn labels(left: &BridgeLabel, right: &BridgeLabel) -> bool {
    let (mut left, mut right) = (left, right);
    loop {
        match (left, right) {
            (BridgeLabel::Provided(inner_left), BridgeLabel::Provided(inner_right)) => {
                left = inner_left;
                right = inner_right;
            }
            _ => return left == right,
        }
    }
}
fn rows(left: &BridgeRow, right: &BridgeRow, work: &mut Vec<(BridgeNodeId, BridgeNodeId)>) -> bool {
    if left.entries.len() != right.entries.len() {
        return false;
    }
    match (left.tail, right.tail) {
        (None, None) => {}
        (Some(left), Some(right)) => work.push((left, right)),
        _ => return false,
    }
    for ((left, inner_left), (right, inner_right)) in left.entries.iter().zip(&right.entries) {
        if !labels(left, right) {
            return false;
        }
        work.push((*inner_left, *inner_right));
    }
    true
}

fn equal_qualified(
    left: &BridgeNode,
    right: &BridgeNode,
    kinds: &[super::validate::RawKind],
    work: &mut Vec<(BridgeNodeId, BridgeNodeId)>,
) -> bool {
    match (left, right) {
        (
            BridgeNode::Captured {
                ty: left,
                captures: inner_left,
                value: value_left,
            },
            BridgeNode::Captured {
                ty: right,
                captures: inner_right,
                value: value_right,
            },
        ) => {
            work.extend([(*left, *right), (*inner_left, *inner_right)]);
            value_left == value_right
        }
        (
            BridgeNode::Forall {
                vars: left,
                constraints: inner_left,
                body: body_left,
            },
            BridgeNode::Forall {
                vars: right,
                constraints: inner_right,
                body: body_right,
            },
        ) => {
            work.push((*body_left, *body_right));
            binders(left, right, kinds) && pairs(inner_left, inner_right, work)
        }
        _ => false,
    }
}
