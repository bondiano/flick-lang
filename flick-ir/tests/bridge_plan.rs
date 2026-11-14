use flick_ir::bridge_plan::{
    BridgeEntry, BridgeKind, BridgeKindId, BridgeNode, BridgeNodeId, BridgePlan, BridgeSelect,
    BridgeType,
};

#[cfg(feature = "cbor")]
use ciborium::value::Value;

use flick_ir::TypeName;
use flick_ir::bridge_plan::{
    BridgeAlias, BridgeAliasId, BridgeAtom, BridgeBinder, BridgeBuiltin, BridgeConstructor,
    BridgeConstructorId, BridgeConstructorOrigin, BridgeDictionaryRole, BridgeEffects, BridgeLabel,
    BridgeMeaning, BridgeRigidId, BridgeRow, BridgeWitness, BridgeWitnessId, BridgeWitnessMember,
    BridgeWitnessScope,
};

include!("bridge_plan/fixtures.rs");
include!("bridge_plan/witness_fixtures.rs");
include!("bridge_plan/references.rs");
include!("bridge_plan/wire.rs");
include!("bridge_plan/stress.rs");
include!("bridge_plan/owned_paths.rs");
include!("bridge_plan/rows.rs");
include!("bridge_plan/named_kinds.rs");
include!("bridge_plan/alias_captures.rs");
include!("bridge_plan/witnesses.rs");
include!("bridge_plan/equality.rs");
