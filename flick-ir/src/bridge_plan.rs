//! Standalone raw foreign-bridge metadata; not part of `Program` or `.iface`.
//!
//! Structural validation is deliberately not execution authorization. Extraction
//! must additionally verify language-item roles and generated service records;
//! an observer must validate physical selectors and compute complete N/H demand.
//! No serialized analysis bits or certificates are accepted here.
//!
//! Frozen row-label atoms retain literal spellings and rigid IDs after core
//! substitution; they are not lexical type-variable or witness references.
//! Witness member registries declare kind views, not execution certificates:
//! extraction must verify them against actual lexical module contracts.
//! Witness aliases retain declaration-site captures; this module checks static
//! contexts, not runtime descriptor frames or actual named-kind authority.

use serde::{Deserialize, Serialize};

use crate::TypeName;

mod equality;
mod validate;
pub use validate::BridgeValidationError;

macro_rules! ids {
    ($($name:ident),* $(,)?) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u32);
    )*};
}

// These six IDs index their corresponding BridgePlan tables.
ids!(
    BridgeKindId,
    BridgeConstructorId,
    BridgeNodeId,
    BridgeAliasId,
    BridgeEntryId,
    BridgeWitnessId
);
// A literal rigid/skolem identity, NEVER a table index or a fresh alias ID.
ids!(BridgeRigidId);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeKind {
    Type,
    Module,
    ServiceRow,
    ErrorRow,
    CaptureRow,
    Symbol,
    Region,
    Constraint,
    Service,
    Row(BridgeKindId),
    Named {
        module: String,
        name: String,
        origin: BridgeConstructorOrigin,
    },
    Arrow(BridgeKindId, BridgeKindId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeAtom {
    Var(String),
    Rigid(BridgeRigidId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeLabel {
    Named(String),
    Capture(BridgeAtom),
    Error(BridgeAtom),
    Service(BridgeAtom),
    Live,
    Provided(Box<BridgeLabel>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgePlan {
    pub kinds: Vec<BridgeKind>,
    pub witnesses: Vec<BridgeWitness>,
    pub constructors: Vec<BridgeConstructor>,
    pub nodes: Vec<BridgeType>,
    pub aliases: Vec<BridgeAlias>,
    pub entries: Vec<BridgeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeType {
    pub kind: BridgeKindId,
    pub node: BridgeNode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeConstructor {
    pub identity: TypeName,
    pub origin: BridgeConstructorOrigin,
    pub kind: BridgeKindId,
    pub meaning: BridgeMeaning,
    pub service_impl: Option<BridgeAliasId>,
    pub dictionary_role: BridgeDictionaryRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BridgeConstructorOrigin {
    Global,
    Witness(BridgeWitnessId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeWitness {
    pub scope: BridgeWitnessScope,
    pub binder: String,
    pub contract: BridgeNodeId,
    pub members: Vec<BridgeWitnessMember>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeWitnessScope {
    Root,
    Within(BridgeWitnessId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeWitnessMember {
    /// Exact atomic Interface.modules ownership edges, not a name prefix.
    pub owner_path: Vec<String>,
    pub name: String,
    pub kind: BridgeKindId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeMeaning {
    Nominal,
    Alias(BridgeAliasId),
    Builtin(BridgeBuiltin),
}

/// Exact compiler-known conversion/projection roles, not class layouts.
/// Extraction checks these against its language-item and hardcoded identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeBuiltin {
    Eff,
    List,
    Option,
    Result,
    Layer,
    Schema,
    PureCodec,
    Codec,
    Fiber,
    Duration,
    Schedule,
    Chunk,
    NonEmptyChunk,
    HashMap,
    HashSet,
    Handlers,
    Exit,
    Failure,
    Errors,
    Field,
    ServiceImpl,
}

/// Orthogonal raw-identity role, never inferred by unfolding `meaning`.
/// A distinct identity aliasing an erased head remains `Passed` unless that
/// identity itself has an authoritative erased language-item role. In
/// particular, `Alias` does not imply `Passed`; extraction must verify roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeDictionaryRole {
    Passed,
    Erased,
    /// Only erased at exactly two raw arguments, the first a bare Symbol.
    Held,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeAlias {
    pub constructor: BridgeConstructorId,
    pub params: Vec<BridgeBinder>,
    pub free: Vec<BridgeBinder>,
    pub body: BridgeNodeId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeBinder {
    pub name: String,
    pub kind: BridgeKindId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeEntry {
    /// Original root after only the already-physical outer dictionary scheme.
    pub root: BridgeNodeId,
    pub free: Vec<BridgeBinder>,
    pub select: BridgeSelect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeSelect {
    Value,
    Argument(u32),
    ResultAfter(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeNode {
    Con(BridgeConstructorId),
    Var(String),
    Symbol(String),
    App {
        head: BridgeNodeId,
        arg: BridgeNodeId,
    },
    Arrow {
        param: BridgeNodeId,
        effects: Option<BridgeEffects>,
        result: BridgeNodeId,
    },
    Tuple(Vec<BridgeNodeId>),
    Record(BridgeRow),
    Row(BridgeRow),
    Union(BridgeNodeId, BridgeNodeId),
    Without {
        row: BridgeNodeId,
        gone: Vec<BridgeLabel>,
    },
    Captured {
        ty: BridgeNodeId,
        captures: BridgeNodeId,
        value: bool,
    },
    Bounded {
        ty: BridgeNodeId,
        captures: BridgeNodeId,
    },
    Forall {
        vars: Vec<BridgeBinder>,
        constraints: Vec<BridgeNodeId>,
        body: BridgeNodeId,
    },
    Exists {
        witness: BridgeWitnessId,
        body: BridgeNodeId,
    },
    Rigid(BridgeRigidId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeRow {
    pub entries: Vec<(BridgeLabel, BridgeNodeId)>,
    pub tail: Option<BridgeNodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeEffects {
    pub services: BridgeNodeId,
    pub errors: BridgeNodeId,
    pub sync: bool,
    pub native_direct: bool,
}
