//! Auxiliary heap-snapshot serializers for layers, causes, contexts, graphs,
//! and temporal metadata.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use flick_core::ir::memory::Name;

use super::ImageError;
use super::heap_snap::HeapSnapper;
use super::heap_types::{
    SnapCliMultiplicity, SnapCliParam, SnapCliSpec, SnapCronData, SnapDefectCode, SnapExitData,
    SnapGraphData, SnapGraphEdge, SnapGraphNode, SnapLayerData, SnapLazyState, SnapObject,
    SnapStoredReason, SnapTimeZoneData, SnapValue,
};
use crate::stdlib::cli::{CliMultiplicity, CliSpec};
use crate::tier0::causes::StoredReason;
use crate::tier0::{
    CronData, Data, DefectCode, ExitData, GraphData, GraphEdge, GraphNode, LayerData, LazyState,
    TimeZoneData, Value,
};

impl HeapSnapper<'_> {
    pub(super) fn snap_lazy(
        &mut self,
        global: &Name,
        path: &mut String,
        local_owner: Option<usize>,
        state: Option<&LazyState>,
    ) -> Result<SnapObject, ImageError> {
        let saved_len = path.len();
        let snapped = match state {
            None => None,
            Some(LazyState::Pending(v)) => {
                path.push_str(".pending");
                let val = self.snap_value(global, path, v)?;
                path.truncate(saved_len);
                Some(SnapLazyState::Pending(val))
            }
            Some(LazyState::DoneOk(v)) => {
                path.push_str(".value");
                let val = self.snap_value(global, path, v)?;
                path.truncate(saved_len);
                Some(SnapLazyState::DoneOk(val))
            }
            Some(LazyState::DoneCause(c)) => {
                path.push_str(".cause");
                let cid = self.snap_object(global, path, c.object_id())?;
                path.truncate(saved_len);
                Some(SnapLazyState::DoneCause(cid))
            }
            Some(LazyState::Running { .. } | LazyState::RunningLocal { .. }) => {
                return Err(Self::reject(global, path, "in-flight lazy evaluation"));
            }
        };
        Ok(SnapObject::Lazy {
            local_owner,
            state: snapped,
        })
    }

    pub(super) fn snap_captures(
        &mut self,
        global: &Name,
        path: &mut String,
        captures: &[Value],
    ) -> Result<Vec<SnapValue>, ImageError> {
        let mut out = Vec::with_capacity(captures.len());
        let saved_len = path.len();
        for (i, cap) in captures.iter().enumerate() {
            let _ = write!(path, ".capture[{i}]");
            out.push(self.snap_value(global, path, cap)?);
            path.truncate(saved_len);
        }
        Ok(out)
    }

    pub(super) fn snap_context(
        &mut self,
        global: &Name,
        path: &mut String,
        entries: &BTreeMap<String, Value>,
    ) -> Result<SnapObject, ImageError> {
        let mut out = BTreeMap::new();
        let saved_len = path.len();
        for (k, v) in entries {
            let _ = write!(path, "[{k}]");
            out.insert(k.clone(), self.snap_value(global, path, v)?);
            path.truncate(saved_len);
        }
        Ok(SnapObject::Context(out))
    }

    pub(super) fn snap_captured_env(
        &mut self,
        global: &Name,
        path: &mut String,
        services: &[Value],
        refs: &BTreeMap<String, Value>,
    ) -> Result<SnapObject, ImageError> {
        let mut s_out = Vec::with_capacity(services.len());
        let saved_len = path.len();
        for (i, s) in services.iter().enumerate() {
            let _ = write!(path, ".services[{i}]");
            s_out.push(self.snap_value(global, path, s)?);
            path.truncate(saved_len);
        }
        let mut r_out = BTreeMap::new();
        for (k, v) in refs {
            let _ = write!(path, ".refs[{k}]");
            r_out.insert(k.clone(), self.snap_value(global, path, v)?);
            path.truncate(saved_len);
        }
        Ok(SnapObject::CapturedEnv {
            services: s_out,
            refs: r_out,
        })
    }

    pub(super) fn snap_cause(
        &mut self,
        global: &Name,
        path: &mut String,
        reasons: &[StoredReason],
        trace: Option<&Value>,
    ) -> Result<SnapObject, ImageError> {
        let mut out_reasons = Vec::with_capacity(reasons.len());
        let saved_len = path.len();
        for (i, r) in reasons.iter().enumerate() {
            let _ = write!(path, ".reasons[{i}]");
            out_reasons.push(self.snap_reason(global, path, r)?);
            path.truncate(saved_len);
        }
        let out_trace = match trace {
            Some(t) => {
                path.push_str(".trace");
                let v = self.snap_value(global, path, t)?;
                path.truncate(saved_len);
                Some(v)
            }
            None => None,
        };
        Ok(SnapObject::Cause {
            reasons: out_reasons,
            trace: out_trace,
        })
    }

    pub(super) fn snap_layer(
        &mut self,
        global: &Name,
        path: &mut String,
        layer: &LayerData,
    ) -> Result<SnapLayerData, ImageError> {
        let saved_len = path.len();
        Ok(match layer {
            LayerData::Succeed {
                tag,
                implementation,
            } => {
                path.push_str(".tag");
                let t = self.snap_value(global, path, tag)?;
                path.truncate(saved_len);
                path.push_str(".implementation");
                let imp = self.snap_value(global, path, implementation)?;
                path.truncate(saved_len);
                SnapLayerData::Succeed {
                    tag: t,
                    implementation: imp,
                }
            }
            LayerData::Merge { left, right } => {
                path.push_str(".left");
                let l = self.snap_value(global, path, left)?;
                path.truncate(saved_len);
                path.push_str(".right");
                let r = self.snap_value(global, path, right)?;
                path.truncate(saved_len);
                SnapLayerData::Merge { left: l, right: r }
            }
            LayerData::Empty => SnapLayerData::Empty,
            LayerData::Context(v) => SnapLayerData::Context(self.snap_value(global, path, v)?),
            LayerData::Dynamic { memo, build } => SnapLayerData::Dynamic {
                memo: *memo,
                build: self.snap_value(global, path, build)?,
            },
            LayerData::Fresh(v) => SnapLayerData::Fresh(self.snap_value(global, path, v)?),
            LayerData::OrDie(v) => SnapLayerData::OrDie(self.snap_value(global, path, v)?),
            LayerData::MergeAll(layers) => {
                let mut out = Vec::with_capacity(layers.len());
                for (i, l) in layers.iter().enumerate() {
                    let _ = write!(path, "[{i}]");
                    out.push(self.snap_value(global, path, l)?);
                    path.truncate(saved_len);
                }
                SnapLayerData::MergeAll(out)
            }
        })
    }

    pub(super) fn snap_exit(
        &mut self,
        global: &Name,
        path: &mut String,
        exit: &ExitData,
    ) -> Result<SnapExitData, ImageError> {
        Ok(match exit {
            ExitData::Success(v) => SnapExitData::Success(self.snap_value(global, path, v)?),
            ExitData::Failure(v) => SnapExitData::Failure(self.snap_value(global, path, v)?),
        })
    }

    pub(super) fn snap_reason(
        &mut self,
        global: &Name,
        path: &mut String,
        reason: &StoredReason,
    ) -> Result<SnapStoredReason, ImageError> {
        Ok(match reason {
            StoredReason::Fail(v) => SnapStoredReason::Fail(self.snap_value(global, path, v)?),
            StoredReason::Die(v) => SnapStoredReason::Die(self.snap_value(global, path, v)?),
            StoredReason::Interrupt { fiber_id } => SnapStoredReason::Interrupt {
                fiber_id: *fiber_id,
            },
        })
    }

    pub(super) fn snap_graph(
        &mut self,
        global: &Name,
        path: &mut String,
        graph: &GraphData,
    ) -> Result<SnapGraphData, ImageError> {
        let saved_len = path.len();
        let mut nodes = Vec::with_capacity(graph.nodes.len());
        for (i, GraphNode { index, data }) in graph.nodes.iter().enumerate() {
            let _ = write!(path, ".nodes[{i}]");
            nodes.push(SnapGraphNode {
                index: *index,
                data: self.snap_value(global, path, data)?,
            });
            path.truncate(saved_len);
        }
        let mut edges = Vec::with_capacity(graph.edges.len());
        for (
            i,
            GraphEdge {
                index,
                source,
                target,
                data,
            },
        ) in graph.edges.iter().enumerate()
        {
            let _ = write!(path, ".edges[{i}]");
            edges.push(SnapGraphEdge {
                index: *index,
                source: *source,
                target: *target,
                data: self.snap_value(global, path, data)?,
            });
            path.truncate(saved_len);
        }
        Ok(SnapGraphData {
            directed: graph.directed,
            nodes,
            edges,
            adj: graph.adj.clone(),
            radj: graph.radj.clone(),
            next_node: graph.next_node,
            next_edge: graph.next_edge,
        })
    }

    pub(super) fn snap_walker(
        &mut self,
        global: &Name,
        path: &mut String,
        indices: &[Value],
        values: &[Value],
    ) -> Result<SnapObject, ImageError> {
        let mut out_i = Vec::with_capacity(indices.len());
        let mut out_v = Vec::with_capacity(values.len());
        let saved_len = path.len();
        for (i, v) in indices.iter().enumerate() {
            let _ = write!(path, ".indices[{i}]");
            out_i.push(self.snap_value(global, path, v)?);
            path.truncate(saved_len);
        }
        for (i, v) in values.iter().enumerate() {
            let _ = write!(path, ".values[{i}]");
            out_v.push(self.snap_value(global, path, v)?);
            path.truncate(saved_len);
        }
        Ok(SnapObject::GraphWalker {
            indices: out_i,
            values: out_v,
        })
    }

    pub(super) fn snap_cli_spec(
        &mut self,
        global: &Name,
        path: &mut String,
        spec: &CliSpec,
    ) -> Result<SnapCliSpec, ImageError> {
        let saved_len = path.len();
        let mut params = Vec::with_capacity(spec.params.len());
        for (i, param) in spec.params.iter().enumerate() {
            let _ = write!(path, ".params[{i}].dictionary");
            let dictionary = self.snap_value(global, path, &param.dictionary)?;
            path.truncate(saved_len);
            params.push(SnapCliParam {
                flag: param.flag,
                label: param.label.clone(),
                aliases: param.aliases.clone(),
                help: param.help.clone(),
                multi: snap_cli_multi(param.multi),
                dictionary,
            });
        }
        let mut subcommands = Vec::with_capacity(spec.subcommands.len());
        for (i, (sub, help)) in spec.subcommands.iter().enumerate() {
            let _ = write!(path, ".subcommands[{i}]");
            let snapped = self.snap_cli_spec(global, path, sub)?;
            path.truncate(saved_len);
            subcommands.push((snapped, help.clone()));
        }
        let handler = match &spec.handler {
            Some(h) => {
                path.push_str(".handler");
                let snapped = self.snap_value(global, path, h)?;
                path.truncate(saved_len);
                Some(snapped)
            }
            None => None,
        };
        Ok(SnapCliSpec {
            name: spec.name.clone(),
            description: spec.description.clone(),
            params,
            subcommands,
            handler,
        })
    }
}

pub(super) const fn snap_cli_multi(multi: CliMultiplicity) -> SnapCliMultiplicity {
    match multi {
        CliMultiplicity::Required => SnapCliMultiplicity::Required,
        CliMultiplicity::Optional => SnapCliMultiplicity::Optional,
        CliMultiplicity::Many => SnapCliMultiplicity::Many,
    }
}

pub(super) fn unserializable_data_kind(data: &Data) -> &'static str {
    match data {
        Data::ForeignHandle { .. } | Data::ForeignCycle { .. } => "host foreign handle",
        Data::FiberHandle { .. } => "live fiber handle",
        Data::Deferred { .. } => "live deferred handle",
        Data::MemoMap { .. } => "live memo map handle",
        Data::FiberGroup { .. } => "live fiber group handle",
        Data::Continuation { .. } => "live continuation",
        Data::MaskRestore { .. } => "live mask restore token",
        Data::CachedRunTask { .. } | Data::CachedInvalidateTask { .. } => "live cached cell handle",
        Data::RecoveryResult { .. } => "transient recovery result",
        Data::ReuseToken { .. } => "frame-limited reuse token",
        Data::Var { .. } => "frame-local variable",
        Data::Arena { .. } => "scoped arena",
        Data::Iterator { .. } => "sequence iterator",
        _ => "unserializable runtime object",
    }
}

pub(super) fn snap_timezone(tz: &TimeZoneData) -> SnapTimeZoneData {
    match tz {
        TimeZoneData::Offset(v) => SnapTimeZoneData::OffsetBits(v.to_bits()),
        TimeZoneData::Named {
            id,
            fixed_seconds,
            is_local,
        } => SnapTimeZoneData::Named {
            id: id.clone(),
            fixed_seconds: *fixed_seconds,
            is_local: *is_local,
        },
    }
}

pub(super) fn snap_cron(cron: &CronData) -> SnapCronData {
    SnapCronData {
        tz: snap_timezone(&cron.tz),
        fields: cron.fields.clone(),
        and: cron.and,
        first: cron.first,
        last: cron.last,
        next: cron.next.clone(),
        prev: cron.prev.clone(),
    }
}

pub(super) fn snap_defect_code(code: &DefectCode) -> SnapDefectCode {
    match code {
        DefectCode::NoMatch => SnapDefectCode::NoMatch,
        DefectCode::MissingHandler { effect, op } => SnapDefectCode::MissingHandler {
            effect: effect.clone(),
            op: op.clone(),
        },
        DefectCode::IntDivisionByZero { remainder } => SnapDefectCode::IntDivisionByZero {
            remainder: *remainder,
        },
        DefectCode::BigIntNegativeExponent => SnapDefectCode::BigIntNegativeExponent,
        DefectCode::BigIntNonIntegralNumber => SnapDefectCode::BigIntNonIntegralNumber,
        DefectCode::BigIntInvalidString => SnapDefectCode::BigIntInvalidString,
        DefectCode::BigIntUndefined => SnapDefectCode::BigIntUndefined,
        DefectCode::ArenaOwnerClosed => SnapDefectCode::ArenaOwnerClosed,
        DefectCode::ArenaClosed => SnapDefectCode::ArenaClosed,
        DefectCode::ResumeTwice => SnapDefectCode::ResumeTwice,
        DefectCode::LazyCycle => SnapDefectCode::LazyCycle,
        DefectCode::Todo(m) => SnapDefectCode::Todo(m.clone()),
        DefectCode::Assert(m) => SnapDefectCode::Assert(m.clone()),
        DefectCode::RangeError(m) => SnapDefectCode::RangeError(m.clone()),
    }
}
