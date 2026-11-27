//! Reachable heap graph serializer preserving DAG aliasing and `Ref`/`Lazy`
//! cycles while rejecting host resources and transient handles with exact paths.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use flick_core::ir::memory::{CodeId, Name};

use super::ImageError;
use super::heap_snap_extra::{
    snap_cron, snap_defect_code, snap_timezone, unserializable_data_kind,
};
use super::heap_types::{
    HeapSnapshot, SnapDateTimeData, SnapDictValue, SnapDurationData, SnapId, SnapNode, SnapObject,
    SnapSeqData, SnapValue, host_resource_tag_kind,
};
use crate::tier0::causes::CauseData;
use crate::tier0::{
    Data, DateTimeData, DictionaryValue, DurationData, GraphWalkerData, Heap, ObjectId, SeqData,
    Value,
};

/// Stateful graph walker that snapshots live heap roots into a [`HeapSnapshot`].
pub(crate) struct HeapSnapper<'h> {
    heap: &'h Heap,
    visited: BTreeMap<ObjectId, SnapId>,
    nodes: Vec<Option<SnapNode>>,
    pub(crate) referenced_codes: BTreeSet<CodeId>,
}

impl<'h> HeapSnapper<'h> {
    pub(crate) fn new(heap: &'h Heap) -> Self {
        Self {
            heap,
            visited: BTreeMap::new(),
            nodes: Vec::new(),
            referenced_codes: BTreeSet::new(),
        }
    }

    /// Snapshots a global root value, reporting exact `global` and field/index
    /// paths if any host resource or transient handle is reachable.
    pub(crate) fn snapshot_root(
        &mut self,
        global: &Name,
        value: &Value,
    ) -> Result<SnapValue, ImageError> {
        let mut path = format!("{}.{}", global.module, global.name);
        self.snap_value(global, &mut path, value)
    }

    pub(crate) fn finish(self) -> HeapSnapshot {
        let objects = self
            .nodes
            .into_iter()
            .map(|n| n.expect("every reserved snapshot slot filled"))
            .collect();
        HeapSnapshot { objects }
    }

    pub(super) fn reject(global: &Name, path: &str, kind: &str) -> ImageError {
        ImageError::UnserializableHostResource {
            global: global.clone(),
            path: path.to_owned(),
            kind: kind.to_owned(),
        }
    }

    pub(super) fn snap_value(
        &mut self,
        global: &Name,
        path: &mut String,
        value: &Value,
    ) -> Result<SnapValue, ImageError> {
        Ok(match value {
            Value::Unit => SnapValue::Unit,
            Value::Bool(b) => SnapValue::Bool(*b),
            Value::Int(v) => SnapValue::Int(*v),
            Value::Int8(v) => SnapValue::Int8(*v),
            Value::Int16(v) => SnapValue::Int16(*v),
            Value::Int64(v) => SnapValue::Int64(*v),
            Value::UInt8(v) => SnapValue::UInt8(*v),
            Value::UInt16(v) => SnapValue::UInt16(*v),
            Value::UInt32(v) => SnapValue::UInt32(*v),
            Value::UInt64(v) => SnapValue::UInt64(*v),
            Value::Float(v) => SnapValue::FloatBits(v.to_bits()),
            Value::Float32(v) => SnapValue::Float32Bits(v.to_bits()),
            Value::Object(tok) => SnapValue::Object(self.snap_object(global, path, tok.id)?),
            Value::Cell(cell) => SnapValue::Cell(self.snap_object(global, path, cell.id)?),
            Value::Lazy(_) => {
                let id = Heap::lazy_id(value).expect("lazy id");
                SnapValue::Lazy(self.snap_object(global, path, id)?)
            }
            Value::Aggregate {
                tag,
                ordinary_fields,
                fields,
            } => {
                if let Some(kind) = host_resource_tag_kind(tag) {
                    return Err(Self::reject(global, path, kind));
                }
                SnapValue::Aggregate {
                    tag: tag.clone(),
                    ordinary_fields: *ordinary_fields,
                    fields: self.snap_indexed_values(global, path, tag, fields)?,
                }
            }
            Value::Owner(_) => return Err(Self::reject(global, path, "scoped owner handle")),
            Value::Arena(_) | Value::ArenaView(_) => {
                return Err(Self::reject(global, path, "scoped arena handle"));
            }
            Value::Var(_) => return Err(Self::reject(global, path, "frame-local variable handle")),
            Value::Iterator(_) => {
                return Err(Self::reject(global, path, "sequence iterator handle"));
            }
            Value::ReuseNull => {
                return Err(Self::reject(global, path, "frame-limited reuse token"));
            }
        })
    }

    fn snap_indexed_values(
        &mut self,
        global: &Name,
        path: &mut String,
        tag: &str,
        fields: &[Value],
    ) -> Result<Vec<SnapValue>, ImageError> {
        let mut out = Vec::with_capacity(fields.len());
        let saved_len = path.len();
        for (idx, field) in fields.iter().enumerate() {
            if tag == "$list" {
                let _ = write!(path, "[{idx}]");
            } else {
                let _ = write!(path, ".{idx}");
            }
            let snapped = self.snap_value(global, path, field)?;
            path.truncate(saved_len);
            out.push(snapped);
        }
        Ok(out)
    }

    pub(super) fn snap_object(
        &mut self,
        global: &Name,
        path: &mut String,
        id: ObjectId,
    ) -> Result<SnapId, ImageError> {
        if let Some(&existing) = self.visited.get(&id) {
            return Ok(existing);
        }
        let obj = self
            .heap
            .object(id)
            .map_err(|err| ImageError::Vm(err.to_string()))?;
        let snap_id = u32::try_from(self.nodes.len())
            .map_err(|_| ImageError::Corrupt("heap snapshot object count overflow".into()))?;
        self.visited.insert(id, snap_id);
        self.nodes.push(None);

        let owner_module = self.heap.object_owner_module(id).map(str::to_owned);
        let data = self.snap_data(global, path, &obj.data)?;
        self.nodes[snap_id as usize] = Some(SnapNode { owner_module, data });
        Ok(snap_id)
    }

    #[allow(clippy::too_many_lines)]
    fn snap_data(
        &mut self,
        global: &Name,
        path: &mut String,
        data: &Data,
    ) -> Result<SnapObject, ImageError> {
        Ok(match data {
            Data::String(s) => SnapObject::String(s.clone()),
            Data::Utf16(u) => SnapObject::Utf16(u.clone()),
            Data::Bytes(b) => SnapObject::Bytes(b.clone()),
            Data::JsonNull => SnapObject::JsonNull,
            Data::BigDecimal { value, scale } => SnapObject::BigDecimal {
                value: value.to_str_radix(10),
                scale: *scale,
            },
            Data::RegExp {
                pattern,
                flags,
                global: is_global,
                ..
            } => SnapObject::RegExp {
                pattern: pattern.clone(),
                flags: flags.clone(),
                global: *is_global,
            },
            Data::BigInt(s) => SnapObject::BigInt(s.clone()),
            Data::Label(s) => SnapObject::Label(s.clone()),
            Data::ServiceTag { key } => SnapObject::ServiceTag { key: key.clone() },
            Data::Tagged {
                tag,
                ordinary_fields,
                fields,
                layout,
            } => {
                if let Some(kind) = host_resource_tag_kind(tag) {
                    return Err(Self::reject(global, path, kind));
                }
                SnapObject::Tagged {
                    tag: tag.clone(),
                    ordinary_fields: *ordinary_fields,
                    fields: self.snap_indexed_values(global, path, tag, fields)?,
                    layout: layout.clone(),
                }
            }
            Data::ListSlice {
                backing,
                offset,
                len,
            } => {
                let (backing_id, base, slice_len) = self
                    .heap
                    .list_view(backing)
                    .map_err(|err| ImageError::Vm(err.to_string()))?;
                let saved_len = path.len();
                if let Ok(backing_obj) = self.heap.object(backing_id)
                    && let Data::Tagged { fields, .. } = &backing_obj.data
                {
                    let end = (*offset + *len).min(base + slice_len).min(fields.len());
                    for (i, item) in fields[*offset..end].iter().enumerate() {
                        let _ = write!(path, "[{i}]");
                        let _ = self.snap_value(global, path, item)?;
                        path.truncate(saved_len);
                    }
                }
                SnapObject::ListSlice {
                    backing: self.snap_value(global, path, backing)?,
                    offset: *offset,
                    len: *len,
                }
            }
            Data::BoxedAggregate { layout, value } => SnapObject::BoxedAggregate {
                layout: layout.clone(),
                value: Box::new(self.snap_value(global, path, value)?),
            },
            Data::Record(fields) => {
                let mut out = Vec::with_capacity(fields.len());
                let saved_len = path.len();
                for (name, val) in fields {
                    let _ = write!(path, ".{name}");
                    let snapped = self.snap_value(global, path, val)?;
                    path.truncate(saved_len);
                    out.push((name.clone(), snapped));
                }
                SnapObject::Record(out)
            }
            Data::Dictionary(entries) => {
                let mut out = Vec::with_capacity(entries.len());
                let saved_len = path.len();
                for (name, entry) in entries {
                    let _ = write!(path, ".{name}");
                    let snapped = match entry {
                        DictionaryValue::Value(v) => {
                            SnapDictValue::Value(self.snap_value(global, path, v)?)
                        }
                        DictionaryValue::Getter(v) => {
                            SnapDictValue::Getter(self.snap_value(global, path, v)?)
                        }
                    };
                    path.truncate(saved_len);
                    out.push((name.clone(), snapped));
                }
                SnapObject::Dictionary(out)
            }
            Data::Closure { code, captures, .. } => {
                self.referenced_codes.insert(*code);
                SnapObject::Closure {
                    code: *code,
                    captures: self.snap_captures(global, path, captures)?,
                }
            }
            Data::Task { code, captures, .. } => {
                self.referenced_codes.insert(*code);
                SnapObject::Task {
                    code: *code,
                    captures: self.snap_captures(global, path, captures)?,
                }
            }
            Data::Cell { contents, .. } => {
                let saved_len = path.len();
                let snapped = match contents {
                    Some(v) => {
                        path.push_str(".contents");
                        let val = self.snap_value(global, path, v)?;
                        path.truncate(saved_len);
                        Some(val)
                    }
                    None => None,
                };
                SnapObject::Cell { contents: snapped }
            }
            Data::Lazy {
                local_owner, state, ..
            } => self.snap_lazy(global, path, *local_owner, state.as_ref())?,
            Data::Seq(seq) => SnapObject::Seq(match seq {
                SeqData::FromList(v) => SnapSeqData::FromList(self.snap_value(global, path, v)?),
                SeqData::Generator {
                    code,
                    captures,
                    limit,
                    ..
                } => {
                    self.referenced_codes.insert(*code);
                    SnapSeqData::Generator {
                        code: *code,
                        captures: self.snap_captures(global, path, captures)?,
                        limit: *limit,
                    }
                }
            }),
            Data::Layer(layer) => SnapObject::Layer(self.snap_layer(global, path, layer)?),
            Data::Context(entries) => self.snap_context(global, path, entries)?,
            Data::ContextRef { key, default_fn } => {
                let saved_len = path.len();
                path.push_str(".default");
                let val = self.snap_value(global, path, default_fn)?;
                path.truncate(saved_len);
                SnapObject::ContextRef {
                    key: key.clone(),
                    default_fn: val,
                }
            }
            Data::CapturedEnv { services, refs } => {
                self.snap_captured_env(global, path, services, refs)?
            }
            Data::Exit(exit) => SnapObject::Exit(self.snap_exit(global, path, exit)?),
            Data::ExitTask(exit) => SnapObject::ExitTask(self.snap_exit(global, path, exit)?),
            Data::MaskedTask { target_mask, task } => {
                let saved_len = path.len();
                path.push_str(".task");
                let val = self.snap_value(global, path, task)?;
                path.truncate(saved_len);
                SnapObject::MaskedTask {
                    target_mask: *target_mask,
                    task: val,
                }
            }
            Data::Cause(CauseData { reasons, trace }) => {
                self.snap_cause(global, path, reasons, trace.as_ref())?
            }
            Data::CauseReason(reason) => {
                SnapObject::CauseReason(self.snap_reason(global, path, reason)?)
            }
            Data::Defect(spec) => SnapObject::Defect(snap_defect_code(spec.code())),
            Data::Duration(d) => SnapObject::Duration(match d {
                DurationData::Millis(ms) => SnapDurationData::MillisBits(ms.to_bits()),
                DurationData::Nanos(ns) => SnapDurationData::Nanos(ns.to_str_radix(10)),
                DurationData::Infinite => SnapDurationData::Infinite,
                DurationData::NegativeInfinite => SnapDurationData::NegativeInfinite,
            }),
            Data::DateTime(DateTimeData { ms, zone }) => SnapObject::DateTime(SnapDateTimeData {
                ms_bits: ms.to_bits(),
                zone: zone.as_ref().map(snap_timezone),
            }),
            Data::TimeZone(tz) => SnapObject::TimeZone(snap_timezone(tz)),
            Data::Cron(cron) => SnapObject::Cron(Box::new(snap_cron(cron))),
            Data::Graph(graph) => {
                SnapObject::Graph(Box::new(self.snap_graph(global, path, graph)?))
            }
            Data::GraphWalker(GraphWalkerData { indices, values }) => {
                self.snap_walker(global, path, indices, values)?
            }
            other => return Err(Self::reject(global, path, unserializable_data_kind(other))),
        })
    }
}
