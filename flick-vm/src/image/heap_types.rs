//! Wire types for heap graph snapshots stored in a [`super::VmImage`].

use std::collections::BTreeMap;

use flick_core::ir::memory::{CellLayout, CodeId};
use serde::{Deserialize, Serialize};

use super::ImageError;

/// Dense index into [`HeapSnapshot::objects`].
pub type SnapId = u32;

/// Serialized heap graph of objects reachable from a saved global table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HeapSnapshot {
    pub(crate) objects: Vec<SnapNode>,
}

/// One serialized heap object with optional module ownership tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapNode {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) owner_module: Option<String>,
    pub(crate) data: SnapObject,
}

/// Serialized representation of [`crate::tier0::Value`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapValue {
    Unit,
    Bool(bool),
    Int(i32),
    Int8(i8),
    Int16(i16),
    Int64(i64),
    UInt8(u8),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    FloatBits(u64),
    Float32Bits(u32),
    Object(SnapId),
    Cell(SnapId),
    Lazy(SnapId),
    Aggregate {
        tag: String,
        ordinary_fields: usize,
        fields: Vec<SnapValue>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SnapDictValue {
    Value(SnapValue),
    Getter(SnapValue),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SnapLayerData {
    Succeed {
        tag: SnapValue,
        implementation: SnapValue,
    },
    Merge {
        left: SnapValue,
        right: SnapValue,
    },
    Empty,
    Context(SnapValue),
    Dynamic {
        memo: bool,
        build: SnapValue,
    },
    Fresh(SnapValue),
    MergeAll(Vec<SnapValue>),
    OrDie(SnapValue),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SnapLazyState {
    Pending(SnapValue),
    DoneOk(SnapValue),
    DoneCause(SnapId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SnapSeqData {
    FromList(SnapValue),
    Generator {
        code: CodeId,
        captures: Vec<SnapValue>,
        limit: Option<usize>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SnapExitData {
    Success(SnapValue),
    Failure(SnapValue),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SnapDurationData {
    MillisBits(u64),
    Nanos(String),
    Infinite,
    NegativeInfinite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SnapTimeZoneData {
    OffsetBits(u64),
    Named {
        id: String,
        fixed_seconds: Option<i32>,
        is_local: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapDateTimeData {
    pub(crate) ms_bits: u64,
    pub(crate) zone: Option<SnapTimeZoneData>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapCronData {
    pub(crate) tz: SnapTimeZoneData,
    pub(crate) fields: [Vec<i32>; 6],
    pub(crate) and: bool,
    pub(crate) first: [i32; 6],
    pub(crate) last: [i32; 6],
    pub(crate) next: [Vec<i32>; 6],
    pub(crate) prev: [Vec<i32>; 6],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapGraphNode {
    pub(crate) index: i32,
    pub(crate) data: SnapValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapGraphEdge {
    pub(crate) index: i32,
    pub(crate) source: i32,
    pub(crate) target: i32,
    pub(crate) data: SnapValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapGraphData {
    pub(crate) directed: bool,
    pub(crate) nodes: Vec<SnapGraphNode>,
    pub(crate) edges: Vec<SnapGraphEdge>,
    pub(crate) adj: BTreeMap<i32, Vec<i32>>,
    pub(crate) radj: BTreeMap<i32, Vec<i32>>,
    pub(crate) next_node: i32,
    pub(crate) next_edge: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SnapStoredReason {
    Fail(SnapValue),
    Die(SnapValue),
    Interrupt { fiber_id: Option<usize> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SnapDefectCode {
    NoMatch,
    MissingHandler { effect: String, op: String },
    IntDivisionByZero { remainder: bool },
    BigIntNegativeExponent,
    BigIntNonIntegralNumber,
    BigIntInvalidString,
    BigIntUndefined,
    ArenaOwnerClosed,
    ArenaClosed,
    ResumeTwice,
    LazyCycle,
    Todo(String),
    Assert(String),
    RangeError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SnapCliMultiplicity {
    Required,
    Optional,
    Many,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapCliParam {
    pub(crate) flag: bool,
    pub(crate) label: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) help: String,
    pub(crate) multi: SnapCliMultiplicity,
    pub(crate) dictionary: SnapValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapCliSpec {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) params: Vec<SnapCliParam>,
    pub(crate) subcommands: Vec<(Self, String)>,
    pub(crate) handler: Option<SnapValue>,
}

/// Serialized representation of serializable [`crate::tier0::Data`] heap objects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SnapObject {
    String(String),
    Utf16(Vec<u16>),
    Bytes(Vec<u8>),
    JsonNull,
    BigDecimal {
        value: String,
        scale: i64,
    },
    RegExp {
        pattern: String,
        flags: String,
        global: bool,
    },
    BigInt(String),
    Label(String),
    ServiceTag {
        key: String,
    },
    Tagged {
        tag: String,
        ordinary_fields: usize,
        fields: Vec<SnapValue>,
        layout: Option<CellLayout>,
    },
    ListSlice {
        backing: SnapValue,
        offset: usize,
        len: usize,
    },
    BoxedAggregate {
        layout: Option<CellLayout>,
        value: Box<SnapValue>,
    },
    Record(Vec<(String, SnapValue)>),
    Dictionary(Vec<(String, SnapDictValue)>),
    Closure {
        code: CodeId,
        captures: Vec<SnapValue>,
    },
    Task {
        code: CodeId,
        captures: Vec<SnapValue>,
    },
    Cell {
        contents: Option<SnapValue>,
    },
    Lazy {
        local_owner: Option<usize>,
        state: Option<SnapLazyState>,
    },
    Seq(SnapSeqData),
    Layer(SnapLayerData),
    Context(BTreeMap<String, SnapValue>),
    ContextRef {
        key: String,
        default_fn: SnapValue,
    },
    CapturedEnv {
        services: Vec<SnapValue>,
        refs: BTreeMap<String, SnapValue>,
    },
    Exit(SnapExitData),
    ExitTask(SnapExitData),
    MaskedTask {
        target_mask: usize,
        task: SnapValue,
    },
    Cause {
        reasons: Vec<SnapStoredReason>,
        trace: Option<SnapValue>,
    },
    CauseReason(SnapStoredReason),
    Defect(SnapDefectCode),
    Duration(SnapDurationData),
    DateTime(SnapDateTimeData),
    TimeZone(SnapTimeZoneData),
    Cron(Box<SnapCronData>),
    Graph(Box<SnapGraphData>),
    GraphWalker {
        indices: Vec<SnapValue>,
        values: Vec<SnapValue>,
    },
    CliSpec(Box<SnapCliSpec>),
    CliMulti(SnapCliMultiplicity),
}

/// Identifies VM-internal constructor tags that back live OS/host resources
/// and must never be serialized into or forged by a VM image.
pub(crate) fn host_resource_tag_kind(tag: &str) -> Option<&'static str> {
    match tag {
        "$vm_file" => Some("open file handle"),
        "$vm_fs_file" => Some("open filesystem file handle"),
        "$vm_process" => Some("live child process handle"),
        "$vm_socket" | "$vm_tcp" | "$vm_udp" => Some("open socket handle"),
        "$vm_timer" => Some("live timer handle"),
        "$vm_host_handle" | "HostHandle" => Some("host handle"),
        "$vm_cmd_single" | "$vm_cmd_pipe" => None,
        other if other.starts_with("$vm_") => Some("VM host resource"),
        _ => None,
    }
}

impl HeapSnapshot {
    /// Remaps every `CodeId` inside closure/task/generator objects using `map`.
    pub(crate) fn remap_codes(&mut self, map: &BTreeMap<CodeId, CodeId>) -> Result<(), ImageError> {
        for node in &mut self.objects {
            match &mut node.data {
                SnapObject::Closure { code, .. }
                | SnapObject::Task { code, .. }
                | SnapObject::Seq(SnapSeqData::Generator { code, .. }) => {
                    *code = *map.get(code).ok_or_else(|| {
                        ImageError::Corrupt(format!("missing code mapping for CodeId {code}"))
                    })?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}
