use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use super::{
    BridgeAliasId, BridgeBinder, BridgeConstructorOrigin, BridgeDictionaryRole, BridgeEffects,
    BridgeKind, BridgeKindId, BridgeLabel, BridgeMeaning, BridgeNode, BridgeNodeId, BridgePlan,
    BridgeRow, BridgeWitnessId, BridgeWitnessScope,
};

/// A stable table/field path and a structural failure, not a semantic fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeValidationError {
    pub path: String,
    pub message: String,
}
impl fmt::Display for BridgeValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}
impl Error for BridgeValidationError {}
fn error(path: impl Into<String>, message: impl Into<String>) -> BridgeValidationError {
    BridgeValidationError {
        path: path.into(),
        message: message.into(),
    }
}
fn reference(id: u32, len: usize, path: &str) -> Result<usize, BridgeValidationError> {
    let index = id as usize;
    if index < len {
        Ok(index)
    } else {
        Err(error(path, "reference out of range"))
    }
}

// Match the compiler's atomic static-member grammar, not encoded module paths.
fn atomic_owned_component(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
        && !name.ends_with('$')
        && !name.contains("$$")
}

/// Postorder of a DAG, with an explicit stack even for adversarial deep graphs.
fn postorder(
    edges: &[Vec<(usize, String)>],
    table: &str,
) -> Result<Vec<usize>, BridgeValidationError> {
    let mut color = vec![0_u8; edges.len()];
    let mut order = Vec::with_capacity(edges.len());
    for root in 0..edges.len() {
        if color[root] != 0 {
            continue;
        }
        color[root] = 1;
        let mut stack = vec![(root, 0)];
        while let Some((node, next)) = stack.last_mut() {
            if let Some((child, path)) = edges[*node].get(*next) {
                *next += 1;
                match color[*child] {
                    1 => return Err(error(path, format!("{table} structural cycle"))),
                    0 => {
                        color[*child] = 1;
                        stack.push((*child, 0));
                    }
                    _ => {}
                }
            } else {
                color[*node] = 2;
                order.push(*node);
                stack.pop();
            }
        }
    }
    Ok(order)
}

// Interning resolved kinds makes equality independent of table duplication and
// avoids recursive equality or repeated walks through deep kind arrows.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum KindKey<K> {
    Base(u8),
    Row(K),
    Named(String, String, Option<BridgeConstructorOrigin>),
    Arrow(K, K),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct SemanticKind(pub(super) usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RawKind(pub(super) usize);
pub(super) struct KindTables {
    pub(super) semantic: Vec<SemanticKind>,
    pub(super) raw: Vec<RawKind>,
    categories: Vec<u16>,
    order: Vec<usize>,
}

#[path = "contexts.rs"]
mod contexts;
mod kinds;
mod nodes;
mod scopes;
mod witnesses;

struct WitnessMembers {
    modules: BTreeSet<String>,
    members: BTreeMap<(String, String), BridgeKindId>,
}

impl BridgePlan {
    /// Checks references, structural DAGs, kinds, declaration links and scopes.
    ///
    /// This does NOT prove builtin identities, generated service provenance,
    /// exact projection, selector reachability/physical FI, or N/H witnesses.
    /// Consumers must complete those checks before executing a decoded plan.
    pub fn validate_structure(&self) -> Result<(), BridgeValidationError> {
        for (name, len) in [
            ("kinds", self.kinds.len()),
            ("witnesses", self.witnesses.len()),
            ("constructors", self.constructors.len()),
            ("nodes", self.nodes.len()),
            ("aliases", self.aliases.len()),
            ("entries", self.entries.len()),
        ] {
            if u32::try_from(len).is_err() {
                return Err(error(name, "table exceeds u32 ID range"));
            }
        }
        let tables = self.validate_kinds()?;
        let kinds = &tables.semantic;
        let classes = &tables.categories;
        let members = self.validate_witnesses()?;
        self.validate_constructors(kinds, &members)?;
        let edges = self.node_edges()?;
        let order = postorder(&edges, "node")?;
        for (i, node) in self.nodes.iter().enumerate() {
            self.check_node(i, &node.node, kinds, classes)?;
        }
        self.validate_aliases(kinds)?;
        self.validate_scopes(&edges, &order, kinds, &tables, &members)
    }

    pub(super) fn validate_kinds(&self) -> Result<KindTables, BridgeValidationError> {
        let mut edges = vec![vec![]; self.kinds.len()];
        for (i, kind) in self.kinds.iter().enumerate() {
            let mut edge = |id: BridgeKindId, field: &str| -> Result<(), BridgeValidationError> {
                let path = format!("kinds[{i}].{field}");
                edges[i].push((reference(id.0, self.kinds.len(), &path)?, path));
                Ok(())
            };
            match kind {
                BridgeKind::Row(k) => edge(*k, "row")?,
                BridgeKind::Arrow(a, b) => {
                    edge(*a, "param")?;
                    edge(*b, "result")?;
                }
                _ => {}
            }
        }
        let order = postorder(&edges, "kind")?;
        let mut categories = vec![0; self.kinds.len()];
        let mut semantic = vec![SemanticKind(0); self.kinds.len()];
        let mut raw = vec![RawKind(0); self.kinds.len()];
        let mut semantic_keys = BTreeMap::new();
        let mut raw_keys = BTreeMap::new();
        for &i in &order {
            categories[i] = kind_class(&self.kinds[i], &categories);
            let sk = kind_key(&self.kinds[i], &semantic, true);
            let rk = kind_key(&self.kinds[i], &raw, false);
            let fresh = SemanticKind(semantic_keys.len());
            semantic[i] = *semantic_keys.entry(sk).or_insert(fresh);
            let fresh = RawKind(raw_keys.len());
            raw[i] = *raw_keys.entry(rk).or_insert(fresh);
        }
        Ok(KindTables {
            semantic,
            raw,
            categories,
            order,
        })
    }
}

// Ordinary variables and lexical witness identities never share a namespace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Requirement {
    Variable(String),
    Witness(BridgeWitnessId),
    KindWitness(BridgeWitnessId),
}
type Requirements = BTreeMap<Requirement, Option<BridgeKindId>>;

const TYPE: u16 = 1;
const SERVICE: u16 = 2;
const REGION: u16 = 4;
const CAPTURES: u16 = 8;
const ERRORS: u16 = 16;
const ARROW_TYPE: u16 = 32;
const ARROW_SERVICE: u16 = 64;
const ARROW_OTHER: u16 = 128;
const OTHER: u16 = 256;
const CAPTURE_KINDS: u16 =
    TYPE | REGION | CAPTURES | ERRORS | ARROW_TYPE | ARROW_SERVICE | ARROW_OTHER;

fn member_module(binder: &str, path: &[String]) -> String {
    let mut module = format!("$exists:{binder}");
    for part in path {
        module.push('.');
        module.push_str(part);
    }
    module
}

fn kind_key<K: Copy>(kind: &BridgeKind, classes: &[K], semantic: bool) -> KindKey<K> {
    match kind {
        BridgeKind::Row(k) => KindKey::Row(classes[k.0 as usize]),
        BridgeKind::Arrow(a, b) => KindKey::Arrow(classes[a.0 as usize], classes[b.0 as usize]),
        BridgeKind::Named {
            module,
            name,
            origin,
        } => KindKey::Named(module.clone(), name.clone(), semantic.then_some(*origin)),
        BridgeKind::Type => KindKey::Base(0),
        BridgeKind::Module => KindKey::Base(1),
        BridgeKind::ServiceRow => KindKey::Base(2),
        BridgeKind::ErrorRow => KindKey::Base(3),
        BridgeKind::CaptureRow => KindKey::Base(4),
        BridgeKind::Symbol => KindKey::Base(5),
        BridgeKind::Region => KindKey::Base(6),
        BridgeKind::Constraint => KindKey::Base(7),
        BridgeKind::Service => KindKey::Base(8),
    }
}

// Compute once in validated kind postorder. These classes constrain actual
// capture-row members only; frozen label atoms carry no kind requirements.
fn kind_class(kind: &BridgeKind, classes: &[u16]) -> u16 {
    match kind {
        BridgeKind::Arrow(_, result) => match classes[result.0 as usize] {
            TYPE | ARROW_TYPE => ARROW_TYPE,
            SERVICE | ARROW_SERVICE => ARROW_SERVICE,
            _ => ARROW_OTHER,
        },
        BridgeKind::Type => TYPE,
        BridgeKind::Service => SERVICE,
        BridgeKind::Region => REGION,
        BridgeKind::CaptureRow => CAPTURES,
        BridgeKind::ErrorRow => ERRORS,
        _ => OTHER,
    }
}

fn merge_requirement(
    needed: &mut Requirements,
    name: Requirement,
    kind: Option<BridgeKindId>,
    kinds: &[SemanticKind],
    path: &str,
) -> Result<(), BridgeValidationError> {
    if let Some(Some(previous)) = needed.get(&name) {
        if let Some(kind) = kind {
            BridgePlan::same_kind(*previous, kind, kinds, path)?;
        }
    } else {
        needed.insert(name, kind);
    }
    Ok(())
}
fn merge_requirements(
    needed: &mut Requirements,
    incoming: &Requirements,
    kinds: &[SemanticKind],
    path: &str,
) -> Result<(), BridgeValidationError> {
    for (name, kind) in incoming {
        merge_requirement(needed, name.clone(), *kind, kinds, path)?;
    }
    Ok(())
}
