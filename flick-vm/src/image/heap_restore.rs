//! Two-pass heap restoration from a validated [`HeapSnapshot`].
//!
//! Pass 1 pre-allocates `Cell` and `Lazy` slots so cyclic `Ref` graphs resolve
//! cleanly. Pass 2 materializes DAG objects (preserving aliasing via `Heap::dup`)
//! and populates `Cell`/`Lazy` payloads. Any error rolls back every allocated
//! slot so a failed load leaves zero leaks.

use std::collections::BTreeMap;

use flick_core::ir::memory::CodeId;
use num_bigint::BigInt;

use super::ImageError;
use super::heap_restore_extra::{
    compile_regexp, restore_cli_multi, restore_cron, restore_defect, restore_timezone,
};
use super::heap_types::{
    HeapSnapshot, SnapDateTimeData, SnapDurationData, SnapId, SnapLazyState, SnapObject,
    SnapSeqData, SnapValue,
};
use crate::tier0::{
    CauseToken, Data, DateTimeData, DefectSpec, DurationData, GraphWalkerData, Heap, LazyState,
    ObjectId, ObjectToken, SeqData, Value,
};

/// Result of restoring a [`HeapSnapshot`] and global value roots into a target [`Heap`].
pub(crate) struct RestoredHeap {
    pub(crate) values: Vec<Option<Value>>,
    pub(crate) cells: Vec<Value>,
    pub(crate) lazies: Vec<Value>,
}

impl HeapSnapshot {
    /// Materializes `roots` and all reachable heap objects into `heap`,
    /// remapping `CodeId`s via `code_map` and stamping `execution`.
    pub(crate) fn restore(
        &self,
        heap: &mut Heap,
        execution: usize,
        code_map: &BTreeMap<CodeId, CodeId>,
        roots: &[Option<&SnapValue>],
        default_module: Option<&str>,
    ) -> Result<RestoredHeap, ImageError> {
        let mut restorer = Restorer {
            snap: self,
            heap,
            execution,
            code_map,
            default_module,
            materialized: vec![None; self.objects.len()],
            cells: Vec::new(),
            lazies: Vec::new(),
            root_values: Vec::with_capacity(roots.len()),
        };
        match restorer.run(roots) {
            Ok(()) => Ok(RestoredHeap {
                values: restorer.root_values,
                cells: restorer.cells,
                lazies: restorer.lazies,
            }),
            Err(err) => {
                restorer.rollback();
                Err(err)
            }
        }
    }
}

pub(super) struct Restorer<'a, 'h> {
    snap: &'a HeapSnapshot,
    pub(super) heap: &'h mut Heap,
    execution: usize,
    code_map: &'a BTreeMap<CodeId, CodeId>,
    default_module: Option<&'a str>,
    materialized: Vec<Option<ObjectId>>,
    cells: Vec<Value>,
    lazies: Vec<Value>,
    root_values: Vec<Option<Value>>,
}

impl Restorer<'_, '_> {
    fn run(&mut self, roots: &[Option<&SnapValue>]) -> Result<(), ImageError> {
        for (idx, node) in self.snap.objects.iter().enumerate() {
            match &node.data {
                SnapObject::Cell { .. } => {
                    let prev = self.heap.set_allocation_module(
                        node.owner_module.as_deref().or(self.default_module),
                    );
                    let tok = self.heap.allocate(Data::Cell {
                        execution: self.execution,
                        contents: None,
                    });
                    self.heap.restore_allocation_module(prev);
                    self.materialized[idx] = tok.object_id();
                    self.cells.push(tok);
                }
                SnapObject::Lazy { local_owner, .. } => {
                    let prev = self.heap.set_allocation_module(
                        node.owner_module.as_deref().or(self.default_module),
                    );
                    let tok = self.heap.allocate(Data::Lazy {
                        execution: self.execution,
                        local_owner: *local_owner,
                        state: None,
                    });
                    self.heap.restore_allocation_module(prev);
                    self.materialized[idx] = tok.object_id();
                    self.lazies.push(tok);
                }
                _ => {}
            }
        }

        for root in roots {
            let val = match root {
                Some(sv) => Some(self.materialize_value(sv)?),
                None => None,
            };
            self.root_values.push(val);
        }

        for (idx, node) in self.snap.objects.iter().enumerate() {
            match &node.data {
                SnapObject::Cell { contents: Some(sv) } => {
                    let val = self.materialize_value(sv)?;
                    let cell_id = self.materialized[idx].expect("pre-allocated cell");
                    if let Ok(obj) = self.heap.object_mut(cell_id)
                        && let Data::Cell { contents, .. } = &mut obj.data
                    {
                        *contents = Some(val);
                    }
                }
                SnapObject::Lazy {
                    state: Some(st), ..
                } => {
                    let restored_state = match st {
                        SnapLazyState::Pending(sv) => {
                            LazyState::Pending(self.materialize_value(sv)?)
                        }
                        SnapLazyState::DoneOk(sv) => LazyState::DoneOk(self.materialize_value(sv)?),
                        SnapLazyState::DoneCause(cid) => {
                            let val = self.materialize_object(*cid)?;
                            let cause = CauseToken::from_cause_value(self.heap, val)
                                .map_err(|err| ImageError::Corrupt(err.to_string()))?;
                            LazyState::DoneCause(cause)
                        }
                    };
                    let lazy_id = self.materialized[idx].expect("pre-allocated lazy");
                    if let Ok(obj) = self.heap.object_mut(lazy_id)
                        && let Data::Lazy { state, .. } = &mut obj.data
                    {
                        *state = Some(restored_state);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn rollback(&mut self) {
        let mut detached = Vec::new();
        for cell in &self.cells {
            if let Some(id) = cell.object_id()
                && let Ok(obj) = self.heap.object_mut(id)
                && let Data::Cell { contents, .. } = &mut obj.data
                && let Some(v) = contents.take()
            {
                detached.push(v);
            }
        }
        for lazy in &self.lazies {
            if let Some(id) = lazy.object_id()
                && let Ok(obj) = self.heap.object_mut(id)
                && let Data::Lazy { state, .. } = &mut obj.data
                && let Some(st) = state.take()
            {
                match st {
                    LazyState::Pending(v)
                    | LazyState::Running { task: v, .. }
                    | LazyState::RunningLocal { task: Some(v) }
                    | LazyState::DoneOk(v) => detached.push(v),
                    LazyState::DoneCause(c) => detached.push(c.into_value()),
                    LazyState::RunningLocal { task: None } => {}
                }
            }
        }
        let mut all = Vec::new();
        all.extend(self.root_values.drain(..).flatten());
        all.append(&mut self.cells);
        all.append(&mut self.lazies);
        all.extend(detached);
        let _ = self.heap.drop_values(all);
    }

    fn map_code(&self, old: CodeId) -> Result<CodeId, ImageError> {
        self.code_map
            .get(&old)
            .copied()
            .ok_or_else(|| ImageError::Corrupt(format!("unmapped CodeId {old} in heap snapshot")))
    }

    pub(super) fn materialize_value(&mut self, sv: &SnapValue) -> Result<Value, ImageError> {
        Ok(match sv {
            SnapValue::Unit => Value::Unit,
            SnapValue::Bool(b) => Value::Bool(*b),
            SnapValue::Int(v) => Value::Int(*v),
            SnapValue::Int8(v) => Value::Int8(*v),
            SnapValue::Int16(v) => Value::Int16(*v),
            SnapValue::Int64(v) => Value::Int64(*v),
            SnapValue::UInt8(v) => Value::UInt8(*v),
            SnapValue::UInt16(v) => Value::UInt16(*v),
            SnapValue::UInt32(v) => Value::UInt32(*v),
            SnapValue::UInt64(v) => Value::UInt64(*v),
            SnapValue::FloatBits(bits) => Value::Float(f64::from_bits(*bits)),
            SnapValue::Float32Bits(bits) => Value::Float32(f32::from_bits(*bits)),
            SnapValue::Object(id) => self.materialize_object(*id)?,
            SnapValue::Cell(id) => {
                let obj_id = self.materialized[*id as usize]
                    .ok_or_else(|| ImageError::Corrupt(format!("missing cell {id}")))?;
                Heap::cell_handle(obj_id, self.execution)
            }
            SnapValue::Lazy(id) => {
                let obj_id = self.materialized[*id as usize]
                    .ok_or_else(|| ImageError::Corrupt(format!("missing lazy {id}")))?;
                Heap::lazy_handle(obj_id, self.execution)
            }
            SnapValue::Aggregate {
                tag,
                ordinary_fields,
                fields,
            } => Value::Aggregate {
                tag: tag.clone(),
                ordinary_fields: *ordinary_fields,
                fields: self.materialize_vec(fields)?,
            },
        })
    }

    pub(super) fn materialize_vec(
        &mut self,
        items: &[SnapValue],
    ) -> Result<Vec<Value>, ImageError> {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            match self.materialize_value(item) {
                Ok(v) => out.push(v),
                Err(err) => {
                    let _ = self.heap.drop_values(out);
                    return Err(err);
                }
            }
        }
        Ok(out)
    }

    fn materialize_object(&mut self, snap_id: SnapId) -> Result<Value, ImageError> {
        let idx = snap_id as usize;
        if let Some(existing) = self.materialized[idx] {
            let repr = Value::Object(ObjectToken { id: existing });
            return self
                .heap
                .dup(&repr)
                .map_err(|err| ImageError::Vm(err.to_string()));
        }
        let node = &self.snap.objects[idx];
        let owner = node.owner_module.as_deref().or(self.default_module);

        if let SnapObject::ServiceTag { key } = &node.data {
            let prev = self.heap.set_allocation_module(owner);
            let res = self.heap.service_tag(key);
            self.heap.restore_allocation_module(prev);
            let val = res.map_err(|err| ImageError::Vm(err.to_string()))?;
            self.materialized[idx] = val.object_id();
            return Ok(val);
        }

        let data = self.build_data(&node.data)?;
        let prev = self.heap.set_allocation_module(owner);
        let val = self.heap.allocate(data);
        self.heap.restore_allocation_module(prev);
        self.materialized[idx] = val.object_id();
        Ok(val)
    }

    #[allow(clippy::too_many_lines)]
    fn build_data(&mut self, obj: &SnapObject) -> Result<Data, ImageError> {
        Ok(match obj {
            SnapObject::String(s) => Data::String(s.clone()),
            SnapObject::Utf16(u) => Data::Utf16(u.clone()),
            SnapObject::Bytes(b) => Data::Bytes(b.clone()),
            SnapObject::JsonNull => Data::JsonNull,
            SnapObject::BigDecimal { value, scale } => {
                let parsed = BigInt::parse_bytes(value.as_bytes(), 10).ok_or_else(|| {
                    ImageError::Corrupt(format!("invalid BigDecimal value `{value}`"))
                })?;
                Data::BigDecimal {
                    value: parsed,
                    scale: *scale,
                }
            }
            SnapObject::RegExp {
                pattern,
                flags,
                global,
            } => Data::RegExp {
                pattern: pattern.clone(),
                flags: flags.clone(),
                global: *global,
                compiled: compile_regexp(pattern, flags)?,
            },
            SnapObject::BigInt(s) => Data::BigInt(s.clone()),
            SnapObject::Label(s) => Data::Label(s.clone()),
            SnapObject::Tagged {
                tag,
                ordinary_fields,
                fields,
                layout,
            } => Data::Tagged {
                tag: tag.clone(),
                ordinary_fields: *ordinary_fields,
                fields: self.materialize_vec(fields)?,
                layout: layout.clone(),
            },
            SnapObject::ListSlice {
                backing,
                offset,
                len,
            } => self.build_list_slice(backing, *offset, *len)?,
            SnapObject::BoxedAggregate { layout, value } => Data::BoxedAggregate {
                layout: layout.clone(),
                value: Box::new(self.materialize_value(value)?),
            },
            SnapObject::Record(fields) => self.build_record(fields)?,
            SnapObject::Dictionary(entries) => self.build_dictionary(entries)?,
            SnapObject::Closure { code, captures } => Data::Closure {
                execution: self.execution,
                code: self.map_code(*code)?,
                captures: self.materialize_vec(captures)?,
            },
            SnapObject::Task { code, captures } => Data::Task {
                execution: self.execution,
                code: self.map_code(*code)?,
                captures: self.materialize_vec(captures)?,
            },
            SnapObject::Seq(seq) => Data::Seq(match seq {
                SnapSeqData::FromList(sv) => SeqData::FromList(self.materialize_value(sv)?),
                SnapSeqData::Generator {
                    code,
                    captures,
                    limit,
                } => SeqData::Generator {
                    execution: self.execution,
                    code: self.map_code(*code)?,
                    captures: self.materialize_vec(captures)?,
                    limit: *limit,
                },
            }),
            SnapObject::Layer(layer) => Data::Layer(self.build_layer(layer)?),
            SnapObject::Context(entries) => Data::Context(self.materialize_map(entries)?),
            SnapObject::ContextRef { key, default_fn } => Data::ContextRef {
                key: key.clone(),
                default_fn: self.materialize_value(default_fn)?,
            },
            SnapObject::CapturedEnv { services, refs } => {
                let svcs = self.materialize_vec(services)?;
                match self.materialize_map(refs) {
                    Ok(r_out) => Data::CapturedEnv {
                        services: svcs,
                        refs: r_out,
                    },
                    Err(err) => {
                        let _ = self.heap.drop_values(svcs);
                        return Err(err);
                    }
                }
            }
            SnapObject::Exit(e) => Data::Exit(self.build_exit(e)?),
            SnapObject::ExitTask(e) => Data::ExitTask(self.build_exit(e)?),
            SnapObject::MaskedTask { target_mask, task } => Data::MaskedTask {
                target_mask: *target_mask,
                task: self.materialize_value(task)?,
            },
            SnapObject::Cause { reasons, trace } => self.build_cause(reasons, trace.as_ref())?,
            SnapObject::CauseReason(r) => Data::CauseReason(self.build_reason(r)?),
            SnapObject::Defect(code) => Data::Defect(DefectSpec::from_code(restore_defect(code))),
            SnapObject::Duration(d) => Data::Duration(match d {
                SnapDurationData::MillisBits(b) => DurationData::Millis(f64::from_bits(*b)),
                SnapDurationData::Nanos(s) => {
                    let n = BigInt::parse_bytes(s.as_bytes(), 10).ok_or_else(|| {
                        ImageError::Corrupt(format!("invalid Duration nanos `{s}`"))
                    })?;
                    DurationData::Nanos(n)
                }
                SnapDurationData::Infinite => DurationData::Infinite,
                SnapDurationData::NegativeInfinite => DurationData::NegativeInfinite,
            }),
            SnapObject::DateTime(SnapDateTimeData { ms_bits, zone }) => {
                Data::DateTime(DateTimeData {
                    ms: f64::from_bits(*ms_bits),
                    zone: zone.as_ref().map(restore_timezone),
                })
            }
            SnapObject::TimeZone(tz) => Data::TimeZone(restore_timezone(tz)),
            SnapObject::Cron(c) => Data::Cron(Box::new(restore_cron(c))),
            SnapObject::Graph(g) => Data::Graph(Box::new(self.build_graph(g)?)),
            SnapObject::GraphWalker { indices, values } => {
                let idx_vals = self.materialize_vec(indices)?;
                match self.materialize_vec(values) {
                    Ok(vals) => Data::GraphWalker(GraphWalkerData {
                        indices: idx_vals,
                        values: vals,
                    }),
                    Err(err) => {
                        let _ = self.heap.drop_values(idx_vals);
                        return Err(err);
                    }
                }
            }
            SnapObject::CliSpec(spec) => Data::CliSpec(Box::new(self.build_cli_spec(spec)?)),
            SnapObject::CliMulti(multi) => Data::CliMulti(restore_cli_multi(*multi)),
            SnapObject::ServiceTag { .. } | SnapObject::Cell { .. } | SnapObject::Lazy { .. } => {
                unreachable!("ServiceTag, Cell, and Lazy handled earlier")
            }
        })
    }
}
