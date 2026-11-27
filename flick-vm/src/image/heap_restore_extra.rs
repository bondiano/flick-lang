//! Auxiliary heap-restoration builders for structured objects, layers, causes,
//! graphs, regular expressions, and temporal metadata.

use std::collections::BTreeMap;

use regex_automata::meta::Regex;

use super::ImageError;
use super::heap_restore::Restorer;
use super::heap_types::{
    SnapCronData, SnapDefectCode, SnapDictValue, SnapExitData, SnapGraphData, SnapLayerData,
    SnapStoredReason, SnapTimeZoneData, SnapValue,
};
use crate::tier0::causes::{CauseData, StoredReason};
use crate::tier0::{
    CronData, Data, DefectCode, DictionaryValue, ExitData, GraphData, GraphEdge, GraphNode,
    LayerData, TimeZoneData, Value,
};

impl Restorer<'_, '_> {
    pub(super) fn build_list_slice(
        &mut self,
        backing: &SnapValue,
        offset: usize,
        len: usize,
    ) -> Result<Data, ImageError> {
        let backing_val = self.materialize_value(backing)?;
        let valid = backing_val
            .object_id()
            .and_then(|id| self.heap.object(id).ok())
            .is_some_and(|o| {
                matches!(
                    &o.data,
                    Data::Tagged { tag, fields, .. }
                        if tag == "$list"
                            && offset.checked_add(len).is_some_and(|end| end <= fields.len())
                )
            });
        if !valid {
            let _ = self.heap.drop_value(backing_val);
            return Err(ImageError::Corrupt(
                "invalid ListSlice backing/range".into(),
            ));
        }
        Ok(Data::ListSlice {
            backing: backing_val,
            offset,
            len,
        })
    }

    pub(super) fn build_record(
        &mut self,
        fields: &[(String, SnapValue)],
    ) -> Result<Data, ImageError> {
        let mut out = Vec::with_capacity(fields.len());
        for (name, sv) in fields {
            match self.materialize_value(sv) {
                Ok(v) => out.push((name.clone(), v)),
                Err(err) => {
                    let _ = self
                        .heap
                        .drop_values(out.into_iter().map(|(_, v)| v).collect());
                    return Err(err);
                }
            }
        }
        Ok(Data::Record(out))
    }

    pub(super) fn build_dictionary(
        &mut self,
        entries: &[(String, SnapDictValue)],
    ) -> Result<Data, ImageError> {
        let mut out = Vec::with_capacity(entries.len());
        for (name, entry) in entries {
            let built = match entry {
                SnapDictValue::Value(sv) => self.materialize_value(sv).map(DictionaryValue::Value),
                SnapDictValue::Getter(sv) => {
                    self.materialize_value(sv).map(DictionaryValue::Getter)
                }
            };
            match built {
                Ok(v) => out.push((name.clone(), v)),
                Err(err) => {
                    let to_drop: Vec<Value> = out
                        .into_iter()
                        .map(|(_, dv)| match dv {
                            DictionaryValue::Value(v) | DictionaryValue::Getter(v) => v,
                        })
                        .collect();
                    let _ = self.heap.drop_values(to_drop);
                    return Err(err);
                }
            }
        }
        Ok(Data::Dictionary(out))
    }

    pub(super) fn materialize_map(
        &mut self,
        entries: &BTreeMap<String, SnapValue>,
    ) -> Result<BTreeMap<String, Value>, ImageError> {
        let mut out = BTreeMap::new();
        for (k, sv) in entries {
            match self.materialize_value(sv) {
                Ok(v) => {
                    out.insert(k.clone(), v);
                }
                Err(err) => {
                    let _ = self.heap.drop_values(out.into_values().collect());
                    return Err(err);
                }
            }
        }
        Ok(out)
    }

    pub(super) fn build_cause(
        &mut self,
        reasons: &[SnapStoredReason],
        trace: Option<&SnapValue>,
    ) -> Result<Data, ImageError> {
        let mut out = Vec::with_capacity(reasons.len());
        for r in reasons {
            match self.build_reason(r) {
                Ok(sr) => out.push(sr),
                Err(err) => {
                    let vals: Vec<Value> = out
                        .into_iter()
                        .filter_map(StoredReason::into_value)
                        .collect();
                    let _ = self.heap.drop_values(vals);
                    return Err(err);
                }
            }
        }
        let tr = match trace {
            Some(sv) => match self.materialize_value(sv) {
                Ok(v) => Some(v),
                Err(err) => {
                    let vals: Vec<Value> = out
                        .into_iter()
                        .filter_map(StoredReason::into_value)
                        .collect();
                    let _ = self.heap.drop_values(vals);
                    return Err(err);
                }
            },
            None => None,
        };
        Ok(Data::Cause(CauseData {
            reasons: out,
            trace: tr,
        }))
    }

    pub(super) fn build_layer(&mut self, layer: &SnapLayerData) -> Result<LayerData, ImageError> {
        Ok(match layer {
            SnapLayerData::Succeed {
                tag,
                implementation,
            } => {
                let t = self.materialize_value(tag)?;
                match self.materialize_value(implementation) {
                    Ok(imp) => LayerData::Succeed {
                        tag: t,
                        implementation: imp,
                    },
                    Err(err) => {
                        let _ = self.heap.drop_value(t);
                        return Err(err);
                    }
                }
            }
            SnapLayerData::Merge { left, right } => {
                let l = self.materialize_value(left)?;
                match self.materialize_value(right) {
                    Ok(r) => LayerData::Merge { left: l, right: r },
                    Err(err) => {
                        let _ = self.heap.drop_value(l);
                        return Err(err);
                    }
                }
            }
            SnapLayerData::Empty => LayerData::Empty,
            SnapLayerData::Context(v) => LayerData::Context(self.materialize_value(v)?),
            SnapLayerData::Dynamic { memo, build } => LayerData::Dynamic {
                memo: *memo,
                build: self.materialize_value(build)?,
            },
            SnapLayerData::Fresh(v) => LayerData::Fresh(self.materialize_value(v)?),
            SnapLayerData::OrDie(v) => LayerData::OrDie(self.materialize_value(v)?),
            SnapLayerData::MergeAll(ls) => LayerData::MergeAll(self.materialize_vec(ls)?),
        })
    }

    pub(super) fn build_exit(&mut self, exit: &SnapExitData) -> Result<ExitData, ImageError> {
        Ok(match exit {
            SnapExitData::Success(v) => ExitData::Success(self.materialize_value(v)?),
            SnapExitData::Failure(v) => ExitData::Failure(self.materialize_value(v)?),
        })
    }

    pub(super) fn build_reason(
        &mut self,
        reason: &SnapStoredReason,
    ) -> Result<StoredReason, ImageError> {
        Ok(match reason {
            SnapStoredReason::Fail(v) => StoredReason::Fail(self.materialize_value(v)?),
            SnapStoredReason::Die(v) => StoredReason::Die(self.materialize_value(v)?),
            SnapStoredReason::Interrupt { fiber_id } => StoredReason::Interrupt {
                fiber_id: *fiber_id,
            },
        })
    }

    pub(super) fn build_graph(&mut self, graph: &SnapGraphData) -> Result<GraphData, ImageError> {
        let mut nodes = Vec::with_capacity(graph.nodes.len());
        for n in &graph.nodes {
            match self.materialize_value(&n.data) {
                Ok(data) => nodes.push(GraphNode {
                    index: n.index,
                    data,
                }),
                Err(err) => {
                    let _ = self
                        .heap
                        .drop_values(nodes.into_iter().map(|gn| gn.data).collect());
                    return Err(err);
                }
            }
        }
        let mut edges = Vec::with_capacity(graph.edges.len());
        for e in &graph.edges {
            match self.materialize_value(&e.data) {
                Ok(data) => edges.push(GraphEdge {
                    index: e.index,
                    source: e.source,
                    target: e.target,
                    data,
                }),
                Err(err) => {
                    let _ = self
                        .heap
                        .drop_values(nodes.into_iter().map(|gn| gn.data).collect());
                    let _ = self
                        .heap
                        .drop_values(edges.into_iter().map(|ge| ge.data).collect());
                    return Err(err);
                }
            }
        }
        Ok(GraphData {
            directed: graph.directed,
            nodes,
            edges,
            adj: graph.adj.clone(),
            radj: graph.radj.clone(),
            next_node: graph.next_node,
            next_edge: graph.next_edge,
        })
    }
}

pub(super) fn compile_regexp(pattern: &str, flags: &str) -> Result<Regex, ImageError> {
    if flags.contains('y') {
        return Err(ImageError::Corrupt(
            "unsupported sticky flag `y` in RegExp snapshot".into(),
        ));
    }
    let mut inline = String::new();
    for flag in ['i', 'm', 's'] {
        if flags.contains(flag) {
            inline.push(flag);
        }
    }
    let source = if inline.is_empty() {
        pattern.to_owned()
    } else {
        format!("(?{inline}){pattern}")
    };
    Regex::new(&source)
        .map_err(|err| ImageError::Corrupt(format!("invalid RegExp /{pattern}/{flags}: {err}")))
}

pub(super) fn restore_timezone(tz: &SnapTimeZoneData) -> TimeZoneData {
    match tz {
        SnapTimeZoneData::OffsetBits(b) => TimeZoneData::Offset(f64::from_bits(*b)),
        SnapTimeZoneData::Named {
            id,
            fixed_seconds,
            is_local,
        } => TimeZoneData::Named {
            id: id.clone(),
            fixed_seconds: *fixed_seconds,
            is_local: *is_local,
        },
    }
}

pub(super) fn restore_cron(c: &SnapCronData) -> CronData {
    CronData {
        tz: restore_timezone(&c.tz),
        fields: c.fields.clone(),
        and: c.and,
        first: c.first,
        last: c.last,
        next: c.next.clone(),
        prev: c.prev.clone(),
    }
}

pub(super) fn restore_defect(code: &SnapDefectCode) -> DefectCode {
    match code {
        SnapDefectCode::NoMatch => DefectCode::NoMatch,
        SnapDefectCode::MissingHandler { effect, op } => DefectCode::MissingHandler {
            effect: effect.clone(),
            op: op.clone(),
        },
        SnapDefectCode::IntDivisionByZero { remainder } => DefectCode::IntDivisionByZero {
            remainder: *remainder,
        },
        SnapDefectCode::BigIntNegativeExponent => DefectCode::BigIntNegativeExponent,
        SnapDefectCode::BigIntNonIntegralNumber => DefectCode::BigIntNonIntegralNumber,
        SnapDefectCode::BigIntInvalidString => DefectCode::BigIntInvalidString,
        SnapDefectCode::BigIntUndefined => DefectCode::BigIntUndefined,
        SnapDefectCode::ArenaOwnerClosed => DefectCode::ArenaOwnerClosed,
        SnapDefectCode::ArenaClosed => DefectCode::ArenaClosed,
        SnapDefectCode::ResumeTwice => DefectCode::ResumeTwice,
        SnapDefectCode::LazyCycle => DefectCode::LazyCycle,
        SnapDefectCode::Todo(m) => DefectCode::Todo(m.clone()),
        SnapDefectCode::Assert(m) => DefectCode::Assert(m.clone()),
        SnapDefectCode::RangeError(m) => DefectCode::RangeError(m.clone()),
    }
}
