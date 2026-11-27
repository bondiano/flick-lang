//! Structural validation of decoded [`HeapSnapshot`] graphs before allocation.

use std::collections::{BTreeMap, BTreeSet};

use flick_core::ir::memory::{CodeId, Function};

use super::ImageError;
use super::heap_restore_extra::compile_regexp;
use super::heap_types::{
    HeapSnapshot, SnapDictValue, SnapDurationData, SnapExitData, SnapLayerData, SnapLazyState,
    SnapObject, SnapSeqData, SnapStoredReason, SnapValue, host_resource_tag_kind,
};

/// Maximum nesting depth of inline `SnapValue::Aggregate` trees.
const MAX_AGGREGATE_DEPTH: usize = 128;

impl HeapSnapshot {
    /// Validates that every `SnapId` inside `val` is in bounds and targets the expected kind.
    pub(crate) fn validate_value_indices(&self, val: &SnapValue) -> Result<(), ImageError> {
        let len = self.objects.len();
        let mut stack: Vec<(&SnapValue, usize)> = vec![(val, 0)];
        while let Some((current, depth)) = stack.pop() {
            if depth > MAX_AGGREGATE_DEPTH {
                return Err(ImageError::Corrupt(format!(
                    "aggregate nesting depth exceeds maximum ({MAX_AGGREGATE_DEPTH})"
                )));
            }
            match current {
                SnapValue::Object(id) if (*id as usize) >= len => {
                    return Err(ImageError::Corrupt(format!(
                        "snapshot object id {id} out of range (len {len})"
                    )));
                }
                SnapValue::Cell(id) => {
                    let node = self.objects.get(*id as usize).ok_or_else(|| {
                        ImageError::Corrupt(format!(
                            "snapshot cell id {id} out of range (len {len})"
                        ))
                    })?;
                    if !matches!(node.data, SnapObject::Cell { .. }) {
                        return Err(ImageError::Corrupt(format!(
                            "snapshot cell id {id} does not point to a Cell object"
                        )));
                    }
                }
                SnapValue::Lazy(id) => {
                    let node = self.objects.get(*id as usize).ok_or_else(|| {
                        ImageError::Corrupt(format!(
                            "snapshot lazy id {id} out of range (len {len})"
                        ))
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
                    if (*tag == "$list" || *tag == "$tuple") && *ordinary_fields != fields.len() {
                        return Err(ImageError::Corrupt(format!(
                            "reserved aggregate `{tag}` cannot carry dictionary evidence"
                        )));
                    }
                    for field in fields {
                        stack.push((field, depth + 1));
                    }
                }
                _ => {}
            }
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
            } => self.validate_tagged(idx, tag, *ordinary_fields, fields),
            SnapObject::ListSlice {
                backing,
                offset,
                len,
            } => self.validate_list_slice(idx, backing, *offset, *len),
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
                Ok(())
            }
            SnapObject::Closure { code, captures }
            | SnapObject::Task { code, captures }
            | SnapObject::Seq(SnapSeqData::Generator { code, captures, .. }) => {
                self.validate_callable(idx, *code, captures, functions)
            }
            SnapObject::BigDecimal { value, .. } => {
                if num_bigint::BigInt::parse_bytes(value.as_bytes(), 10).is_none() {
                    return Err(ImageError::Corrupt(format!(
                        "invalid BigDecimal value `{value}` at object {idx}"
                    )));
                }
                Ok(())
            }
            SnapObject::Duration(SnapDurationData::Nanos(s)) => {
                if num_bigint::BigInt::parse_bytes(s.as_bytes(), 10).is_none() {
                    return Err(ImageError::Corrupt(format!(
                        "invalid Duration nanos `{s}` at object {idx}"
                    )));
                }
                Ok(())
            }
            SnapObject::RegExp { pattern, flags, .. } => {
                let _ = compile_regexp(pattern, flags)?;
                Ok(())
            }
            SnapObject::GraphWalker { indices, values } => {
                if indices.len() != values.len() {
                    return Err(ImageError::Corrupt(format!(
                        "GraphWalker {idx} indices len ({}) != values len ({})",
                        indices.len(),
                        values.len()
                    )));
                }
                for v in indices.iter().chain(values.iter()) {
                    self.validate_value_indices(v)?;
                }
                Ok(())
            }
            SnapObject::Lazy {
                state: Some(SnapLazyState::DoneCause(cid)),
                ..
            } => self.validate_lazy_cause(idx, *cid),
            _ => {
                for child in object_child_values(obj) {
                    self.validate_value_indices(child)?;
                }
                Ok(())
            }
        }
    }

    fn validate_tagged(
        &self,
        idx: usize,
        tag: &str,
        ordinary_fields: usize,
        fields: &[SnapValue],
    ) -> Result<(), ImageError> {
        if let Some(kind) = host_resource_tag_kind(tag) {
            return Err(ImageError::Corrupt(format!(
                "forged host resource tag `{tag}` ({kind}) at object {idx}"
            )));
        }
        if ordinary_fields > fields.len() {
            return Err(ImageError::Corrupt(format!(
                "object {idx} ordinary_fields ({ordinary_fields}) > len ({})",
                fields.len()
            )));
        }
        if (tag == "$list" || tag == "$tuple") && ordinary_fields != fields.len() {
            return Err(ImageError::Corrupt(format!(
                "reserved constructor `{tag}` at object {idx} cannot carry dictionary evidence"
            )));
        }
        for f in fields {
            self.validate_value_indices(f)?;
        }
        Ok(())
    }

    fn validate_list_slice(
        &self,
        idx: usize,
        backing: &SnapValue,
        offset: usize,
        len: usize,
    ) -> Result<(), ImageError> {
        self.validate_value_indices(backing)?;
        let SnapValue::Object(backing_id) = backing else {
            return Err(ImageError::Corrupt(format!(
                "ListSlice at object {idx} backing is not an Object"
            )));
        };
        let target = self.objects.get(*backing_id as usize).ok_or_else(|| {
            ImageError::Corrupt(format!("ListSlice {idx} backing {backing_id} out of range"))
        })?;
        let SnapObject::Tagged { tag, fields, .. } = &target.data else {
            return Err(ImageError::Corrupt(format!(
                "ListSlice {idx} backing {backing_id} is not a Tagged $list"
            )));
        };
        if tag != "$list" || offset.checked_add(len).is_none_or(|end| end > fields.len()) {
            return Err(ImageError::Corrupt(format!(
                "ListSlice {idx} range {offset}..+{len} exceeds backing $list len {}",
                fields.len()
            )));
        }
        Ok(())
    }

    fn validate_callable(
        &self,
        idx: usize,
        code: CodeId,
        captures: &[SnapValue],
        functions: &BTreeMap<CodeId, Function>,
    ) -> Result<(), ImageError> {
        let func = functions.get(&code).ok_or_else(|| {
            ImageError::Corrupt(format!("object {idx} references unknown CodeId {code}"))
        })?;
        if func.params.len() != 2 {
            return Err(ImageError::Corrupt(format!(
                "object {idx} code {code} expects {} params, must be 2",
                func.params.len()
            )));
        }
        for c in captures {
            self.validate_value_indices(c)?;
        }
        Ok(())
    }

    fn validate_lazy_cause(&self, idx: usize, cid: u32) -> Result<(), ImageError> {
        let target = self.objects.get(cid as usize).ok_or_else(|| {
            ImageError::Corrupt(format!("lazy {idx} cause id {cid} out of range"))
        })?;
        if !matches!(target.data, SnapObject::Cause { .. }) {
            return Err(ImageError::Corrupt(format!(
                "lazy {idx} cause id {cid} is not a Cause object"
            )));
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
    let mut work = vec![val];
    while let Some(current) = work.pop() {
        match current {
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
                work.extend(fields.iter());
            }
            _ => {}
        }
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
