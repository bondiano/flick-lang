//! Structural validation of decoded [`HeapSnapshot`] graphs before allocation.

use std::collections::{BTreeMap, BTreeSet};

use flick_core::ir::memory::{CodeId, Function};

use super::ImageError;
use super::heap_types::{
    HeapSnapshot, SnapDictValue, SnapExitData, SnapLayerData, SnapLazyState, SnapObject,
    SnapSeqData, SnapStoredReason, SnapValue, host_resource_tag_kind,
};

impl HeapSnapshot {
    /// Validates that every `SnapId` inside `val` is in bounds and targets the expected kind.
    pub(crate) fn validate_value_indices(&self, val: &SnapValue) -> Result<(), ImageError> {
        let len = self.objects.len();
        match val {
            SnapValue::Object(id) if (*id as usize) >= len => {
                return Err(ImageError::Corrupt(format!(
                    "snapshot object id {id} out of range (len {len})"
                )));
            }
            SnapValue::Cell(id) => {
                let node = self.objects.get(*id as usize).ok_or_else(|| {
                    ImageError::Corrupt(format!("snapshot cell id {id} out of range (len {len})"))
                })?;
                if !matches!(node.data, SnapObject::Cell { .. }) {
                    return Err(ImageError::Corrupt(format!(
                        "snapshot cell id {id} does not point to a Cell object"
                    )));
                }
            }
            SnapValue::Lazy(id) => {
                let node = self.objects.get(*id as usize).ok_or_else(|| {
                    ImageError::Corrupt(format!("snapshot lazy id {id} out of range (len {len})"))
                })?;
                if !matches!(node.data, SnapObject::Lazy { .. }) {
                    return Err(ImageError::Corrupt(format!(
                        "snapshot lazy id {id} does not point to a Lazy object"
                    )));
                }
            }
            SnapValue::Aggregate {
                tag,
                ordinary_fields,
                fields,
            } => {
                if let Some(kind) = host_resource_tag_kind(tag) {
                    return Err(ImageError::Corrupt(format!(
                        "forged host resource tag `{tag}` ({kind}) in aggregate"
                    )));
                }
                if *ordinary_fields > fields.len() {
                    return Err(ImageError::Corrupt(format!(
                        "aggregate `{tag}` ordinary_fields ({ordinary_fields}) exceeds field count ({})",
                        fields.len()
                    )));
                }
                for field in fields {
                    self.validate_value_indices(field)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Validates every node's indices, tags, code references, and non-cell acyclicity.
    pub(crate) fn validate_structure(
        &self,
        functions: &BTreeMap<CodeId, Function>,
    ) -> Result<(), ImageError> {
        for (idx, node) in self.objects.iter().enumerate() {
            self.validate_object(idx, &node.data, functions)?;
        }
        let mut state = vec![0_u8; self.objects.len()];
        for idx in 0..self.objects.len() {
            if state[idx] == 0 {
                self.check_acyclic(idx, &mut state)?;
            }
        }
        Ok(())
    }

    fn validate_object(
        &self,
        idx: usize,
        obj: &SnapObject,
        functions: &BTreeMap<CodeId, Function>,
    ) -> Result<(), ImageError> {
        match obj {
            SnapObject::Tagged {
                tag,
                ordinary_fields,
                fields,
                ..
            } => {
                if let Some(kind) = host_resource_tag_kind(tag) {
                    return Err(ImageError::Corrupt(format!(
                        "forged host resource tag `{tag}` ({kind}) at object {idx}"
                    )));
                }
                if *ordinary_fields > fields.len() {
                    return Err(ImageError::Corrupt(format!(
                        "object {idx} ordinary_fields ({ordinary_fields}) > len ({})",
                        fields.len()
                    )));
                }
                for f in fields {
                    self.validate_value_indices(f)?;
                }
            }
            SnapObject::Record(fields) => {
                let mut seen = BTreeSet::new();
                for (name, val) in fields {
                    if !seen.insert(name) {
                        return Err(ImageError::Corrupt(format!(
                            "duplicate record field `{name}` at object {idx}"
                        )));
                    }
                    self.validate_value_indices(val)?;
                }
            }
            SnapObject::Closure { code, captures } | SnapObject::Task { code, captures } => {
                if !functions.contains_key(code) {
                    return Err(ImageError::Corrupt(format!(
                        "object {idx} references unknown CodeId {code}"
                    )));
                }
                for c in captures {
                    self.validate_value_indices(c)?;
                }
            }
            SnapObject::Seq(SnapSeqData::Generator { code, captures, .. }) => {
                if !functions.contains_key(code) {
                    return Err(ImageError::Corrupt(format!(
                        "generator {idx} references unknown CodeId {code}"
                    )));
                }
                for c in captures {
                    self.validate_value_indices(c)?;
                }
            }
            SnapObject::Lazy {
                state: Some(SnapLazyState::DoneCause(cid)),
                ..
            } => {
                let target = self.objects.get(*cid as usize).ok_or_else(|| {
                    ImageError::Corrupt(format!("lazy {idx} cause id {cid} out of range"))
                })?;
                if !matches!(target.data, SnapObject::Cause { .. }) {
                    return Err(ImageError::Corrupt(format!(
                        "lazy {idx} cause id {cid} is not a Cause object"
                    )));
                }
            }
            _ => {
                for child in object_child_values(obj) {
                    self.validate_value_indices(child)?;
                }
            }
        }
        Ok(())
    }

    fn check_acyclic(&self, start: usize, state: &mut [u8]) -> Result<(), ImageError> {
        let mut stack: Vec<(usize, bool)> = vec![(start, false)];
        while let Some((idx, exiting)) = stack.pop() {
            if exiting {
                state[idx] = 2;
                continue;
            }
            if state[idx] == 2 {
                continue;
            }
            if state[idx] == 1 {
                return Err(ImageError::Corrupt(format!(
                    "non-cell cyclic reference involving snapshot object {idx}"
                )));
            }
            if matches!(
                self.objects[idx].data,
                SnapObject::Cell { .. } | SnapObject::Lazy { .. }
            ) {
                state[idx] = 2;
                continue;
            }
            state[idx] = 1;
            stack.push((idx, true));
            for child in object_child_values(&self.objects[idx].data) {
                push_object_edges(child, &mut stack, state)?;
            }
        }
        Ok(())
    }
}

fn push_object_edges(
    val: &SnapValue,
    stack: &mut Vec<(usize, bool)>,
    state: &[u8],
) -> Result<(), ImageError> {
    match val {
        SnapValue::Object(id) => {
            let idx = *id as usize;
            if state[idx] == 1 {
                return Err(ImageError::Corrupt(format!(
                    "non-cell cyclic reference involving snapshot object {idx}"
                )));
            }
            if state[idx] == 0 {
                stack.push((idx, false));
            }
        }
        SnapValue::Aggregate { fields, .. } => {
            for f in fields {
                push_object_edges(f, stack, state)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn object_child_values(obj: &SnapObject) -> Vec<&SnapValue> {
    let mut out = Vec::new();
    match obj {
        SnapObject::Tagged { fields, .. }
        | SnapObject::Closure {
            captures: fields, ..
        }
        | SnapObject::Task {
            captures: fields, ..
        }
        | SnapObject::Seq(SnapSeqData::Generator {
            captures: fields, ..
        }) => out.extend(fields.iter()),
        SnapObject::ListSlice { backing: v, .. }
        | SnapObject::Seq(SnapSeqData::FromList(v))
        | SnapObject::Exit(SnapExitData::Success(v) | SnapExitData::Failure(v))
        | SnapObject::ExitTask(SnapExitData::Success(v) | SnapExitData::Failure(v))
        | SnapObject::ContextRef { default_fn: v, .. }
        | SnapObject::MaskedTask { task: v, .. } => out.push(v),
        SnapObject::BoxedAggregate { value, .. } => out.push(value),
        SnapObject::Record(fields) => out.extend(fields.iter().map(|(_, v)| v)),
        SnapObject::Dictionary(entries) => out.extend(entries.iter().map(|(_, dv)| match dv {
            SnapDictValue::Value(v) | SnapDictValue::Getter(v) => v,
        })),
        SnapObject::Cell { contents } => out.extend(contents.iter()),
        SnapObject::Lazy { state, .. } => match state {
            Some(SnapLazyState::Pending(v) | SnapLazyState::DoneOk(v)) => out.push(v),
            Some(SnapLazyState::DoneCause(_)) | None => {}
        },
        SnapObject::Layer(layer) => match layer {
            SnapLayerData::Succeed {
                tag,
                implementation,
            } => out.extend([tag, implementation]),
            SnapLayerData::Merge { left, right } => out.extend([left, right]),
            SnapLayerData::Context(v)
            | SnapLayerData::Dynamic { build: v, .. }
            | SnapLayerData::Fresh(v)
            | SnapLayerData::OrDie(v) => out.push(v),
            SnapLayerData::MergeAll(ls) => out.extend(ls.iter()),
            SnapLayerData::Empty => {}
        },
        SnapObject::Context(m) => out.extend(m.values()),
        SnapObject::CapturedEnv { services, refs } => {
            out.extend(services.iter());
            out.extend(refs.values());
        }
        SnapObject::Cause { reasons, trace } => {
            for r in reasons {
                if let SnapStoredReason::Fail(v) | SnapStoredReason::Die(v) = r {
                    out.push(v);
                }
            }
            out.extend(trace.iter());
        }
        SnapObject::CauseReason(SnapStoredReason::Fail(v) | SnapStoredReason::Die(v)) => {
            out.push(v);
        }
        SnapObject::Graph(g) => {
            out.extend(g.nodes.iter().map(|n| &n.data));
            out.extend(g.edges.iter().map(|e| &e.data));
        }
        SnapObject::GraphWalker { indices, values } => {
            out.extend(indices.iter());
            out.extend(values.iter());
        }
        _ => {}
    }
    out
}
