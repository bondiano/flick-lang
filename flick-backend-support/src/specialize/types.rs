//! Canonical identifiers, keys, fallback reasons, and specialized program items.

use std::collections::BTreeMap;
use std::fmt;

use flick_ir::{Constructor, Dict, Foreign, Span, TypeRef};

use crate::layout::TypeLayout;

use super::ir::{SpecDict, SpecExpr};

/// Canonical top-level definition identity: `(module, name)`.
pub type DefKey = (String, String);

/// Canonical constructor specialization key: `(module, type_name, ctor_name, type_args, boxed)`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CtorKey {
    pub module: String,
    pub ty: String,
    pub name: String,
    pub type_args: Vec<TypeRef>,
    pub boxed: bool,
}

/// Deterministic, canonical mangled symbol name for a specialized or fallback
/// definition, instance method, or foreign entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolId(pub String);

impl SymbolId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a definition or boundary compiles once over uniform boxed cells instead
/// of monomorphizing per concrete type argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FallbackReason {
    /// Recursive SCC whose recursive calls change or grow type arguments
    /// against a `forall` signature (polymorphic recursion).
    PolymorphicRecursion,
    /// Function taking or returning a higher-rank polymorphic callback
    /// (`IrType::Forall` with quantified type variables).
    HigherRankCallback,
    /// Packed module (`Prelude.Module`) or existential (`IrType::Exists`)
    /// abstraction boundary.
    PackedOrExistential,
    /// GADT constructor holding class dictionaries or existential type
    /// variables.
    GadtDictionary,
    /// Polymorphic `foreign` binding taking or returning type parameters.
    PolymorphicForeign,
    /// Open-row polymorphic function whose row tail remains polymorphic across
    /// a fallback boundary.
    OpenRowPolymorphic,
}

impl fmt::Display for FallbackReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FallbackReason::PolymorphicRecursion => write!(f, "polymorphic recursion"),
            FallbackReason::HigherRankCallback => write!(f, "higher-rank callback"),
            FallbackReason::PackedOrExistential => write!(f, "packed module or existential"),
            FallbackReason::GadtDictionary => write!(f, "GADT dictionary or existential"),
            FallbackReason::PolymorphicForeign => write!(f, "polymorphic foreign binding"),
            FallbackReason::OpenRowPolymorphic => write!(f, "open-row polymorphic record"),
        }
    }
}

/// Whether a lowered definition is monomorphized or uniform boxed-fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DefMode {
    /// Fully specialized for concrete types and layout-aware field offsets.
    Specialized,
    /// Compiled once over uniform boxed cells for the given static reason.
    Fallback(FallbackReason),
}

/// How one class dictionary parameter of a definition is handled in a given
/// specialization.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DictSpec {
    /// Exact dictionary evidence is statically known and baked into the
    /// specialized copy (no runtime dictionary parameter).
    Known(CanonicalDict),
    /// Dictionary is supplied dynamically at runtime (local `given`, opened
    /// packed-module dictionary, GADT field dictionary, or dynamic context).
    Runtime,
}

/// Canonical, `Ord`-implementing projection of a statically known [`Dict`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CanonicalDict {
    Instance {
        module: String,
        name: String,
        args: Vec<CanonicalDict>,
    },
    Super {
        dict: Box<CanonicalDict>,
        class: String,
    },
    Structural {
        class: String,
        fields: Vec<CanonicalDict>,
    },
    Primitive {
        constraint: String,
        labels: Vec<String>,
    },
    Site {
        place: String,
        definition: String,
    },
}

impl CanonicalDict {
    /// Converts `dict` into a [`CanonicalDict`] iff its entire tree is
    /// statically known (contains no `Given`, `Opened`, `Field`, `Value`, or
    /// `Error`).
    pub fn from_static(dict: &Dict) -> Option<CanonicalDict> {
        match dict {
            Dict::Instance { module, name, args } => {
                let args = args
                    .iter()
                    .map(CanonicalDict::from_static)
                    .collect::<Option<Vec<_>>>()?;
                Some(CanonicalDict::Instance {
                    module: module.clone(),
                    name: name.clone(),
                    args,
                })
            }
            Dict::Super { dict, class } => Some(CanonicalDict::Super {
                dict: Box::new(CanonicalDict::from_static(dict)?),
                class: class.clone(),
            }),
            Dict::Structural { class, fields } => {
                let fields = fields
                    .iter()
                    .map(CanonicalDict::from_static)
                    .collect::<Option<Vec<_>>>()?;
                Some(CanonicalDict::Structural {
                    class: class.clone(),
                    fields,
                })
            }
            Dict::Primitive { constraint, labels } => Some(CanonicalDict::Primitive {
                constraint: constraint.clone(),
                labels: labels.clone(),
            }),
            Dict::Site { place, definition } => Some(CanonicalDict::Site {
                place: place.clone(),
                definition: definition.clone(),
            }),
            Dict::Given(_)
            | Dict::Value { .. }
            | Dict::Opened { .. }
            | Dict::Field(_)
            | Dict::Error => None,
        }
    }

    /// Reconstructs the corresponding [`Dict`].
    pub fn to_dict(&self) -> Dict {
        match self {
            CanonicalDict::Instance { module, name, args } => Dict::Instance {
                module: module.clone(),
                name: name.clone(),
                args: args.iter().map(CanonicalDict::to_dict).collect(),
            },
            CanonicalDict::Super { dict, class } => Dict::Super {
                dict: Box::new(dict.to_dict()),
                class: class.clone(),
            },
            CanonicalDict::Structural { class, fields } => Dict::Structural {
                class: class.clone(),
                fields: fields.iter().map(CanonicalDict::to_dict).collect(),
            },
            CanonicalDict::Primitive { constraint, labels } => Dict::Primitive {
                constraint: constraint.clone(),
                labels: labels.clone(),
            },
            CanonicalDict::Site { place, definition } => Dict::Site {
                place: place.clone(),
                definition: definition.clone(),
            },
        }
    }

    /// Deterministic human-readable representation for canonical mangling.
    pub fn canonical_text(&self) -> String {
        match self {
            CanonicalDict::Instance { module, name, args } => {
                if args.is_empty() {
                    format!("I({module}.{name})")
                } else {
                    let parts: Vec<String> = args.iter().map(Self::canonical_text).collect();
                    format!("I({module}.{name}[{}])", parts.join(","))
                }
            }
            CanonicalDict::Super { dict, class } => {
                format!("Sup({},{class})", dict.canonical_text())
            }
            CanonicalDict::Structural { class, fields } => {
                let parts: Vec<String> = fields.iter().map(Self::canonical_text).collect();
                format!("Str({class}[{}])", parts.join(","))
            }
            CanonicalDict::Primitive { constraint, labels } => {
                format!("Prim({constraint}[{}])", labels.join(","))
            }
            CanonicalDict::Site { place, definition } => format!("Site({place},{definition})"),
        }
    }
}

/// Canonical identity of a specialized or fallback definition instance.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InstanceKey {
    /// Monomorphized top-level value definition.
    Value {
        def: DefKey,
        type_args: Vec<TypeRef>,
        dict_specs: Vec<DictSpec>,
    },
    /// Monomorphized class instance method called directly because exact
    /// dictionary evidence is statically known.
    InstanceMethod {
        instance: DefKey,
        method: String,
        context_dicts: Vec<CanonicalDict>,
        type_args: Vec<TypeRef>,
        method_dict_specs: Vec<DictSpec>,
    },
    /// Uniform boxed-cell fallback compiled once for `def`.
    Fallback { def: DefKey },
}

/// Value representation at a parameter, local, or expression boundary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueRep {
    /// Concrete monomorphic value laid out according to `TypeLayout`.
    Unboxed(TypeRef),
    /// Uniform boxed cell used by fallback code and polymorphic boundaries.
    Boxed,
}

/// One lowered definition (specialized or boxed-fallback) in a
/// [`super::SpecializedProgram`].
#[derive(Debug, Clone, PartialEq)]
pub struct SpecializedDef {
    pub symbol: SymbolId,
    pub source: DefKey,
    pub mode: DefMode,
    /// Concrete type arguments (`Vec::new()` for fallback definitions).
    pub type_args: Vec<TypeRef>,
    /// Runtime dictionary parameter names remaining after devirtualizing
    /// statically known dictionaries.
    pub runtime_dicts: Vec<String>,
    /// Instantiated type of this definition, when known.
    pub ty: Option<TypeRef>,
    /// Representation of the definition's value (`Unboxed` for specialized,
    /// `Boxed` for fallback).
    pub rep: ValueRep,
    /// Lowered expression body with explicit `Box`/`Unbox` and field offsets.
    pub body: SpecExpr,
    pub span: Span,
}

/// One reachable constructor specialization (with concrete layout) or boxed
/// fallback constructor.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecializedCtor {
    pub key: CtorKey,
    pub ctor: Constructor,
    /// Concrete instantiated result type (`None` for boxed fallback).
    pub result_ty: Option<TypeRef>,
    /// Concrete [`TypeLayout`] of the enclosing ADT when monomorphized.
    pub layout: Option<TypeLayout>,
    /// Indices of fields that are stored as boxed cells (e.g. GADT
    /// existentials or fallback constructors).
    pub boxed_fields: Vec<usize>,
}

/// One runtime class instance table (used when a dictionary is passed at
/// runtime rather than devirtualized to a direct call).
#[derive(Debug, Clone, PartialEq)]
pub struct SpecializedInstance {
    pub symbol: SymbolId,
    pub module: String,
    pub name: String,
    pub class: (String, String),
    /// Context dictionary parameter names taken to construct this dictionary.
    pub context: Vec<String>,
    /// Superclass dictionary projections.
    pub supers: Vec<(String, SpecDict)>,
    /// Uniform boxed-ABI method closures keyed by method name.
    pub methods: BTreeMap<String, SpecExpr>,
}

/// One reachable `foreign` binding in a [`super::SpecializedProgram`].
#[derive(Debug, Clone, PartialEq)]
pub struct SpecializedForeign {
    pub symbol: SymbolId,
    pub module: String,
    pub name: String,
    pub foreign: Foreign,
    /// `true` when the binding is polymorphic and takes/returns uniform boxed
    /// cells; `false` when monomorphic.
    pub polymorphic: bool,
    pub reason: Option<FallbackReason>,
}

/// Deterministic FNV-1a 64-bit hash (never relies on randomized `HashState`).
pub(crate) fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Canonical mangled [`SymbolId`] for an [`InstanceKey`].
pub(crate) fn mangle_key(key: &InstanceKey) -> SymbolId {
    match key {
        InstanceKey::Value {
            def: (module, name),
            type_args,
            dict_specs,
        } => {
            if type_args.is_empty() && dict_specs.iter().all(|d| matches!(d, DictSpec::Runtime)) {
                return SymbolId(format!("{module}.{name}"));
            }
            let sig = format_spec_sig(type_args, dict_specs);
            let hash = fnv1a_64(format!("V|{module}|{name}|{sig}").as_bytes());
            SymbolId(format!("{module}.{name}$s_{hash:016x}"))
        }
        InstanceKey::InstanceMethod {
            instance: (module, name),
            method,
            context_dicts,
            type_args,
            method_dict_specs,
        } => {
            let ctx: Vec<String> = context_dicts
                .iter()
                .map(CanonicalDict::canonical_text)
                .collect();
            let sig = format_spec_sig(type_args, method_dict_specs);
            let payload = format!("M|{module}|{name}|{method}|{}|{sig}", ctx.join(";"));
            let hash = fnv1a_64(payload.as_bytes());
            SymbolId(format!("{module}.{name}${method}$m_{hash:016x}"))
        }
        InstanceKey::Fallback {
            def: (module, name),
        } => SymbolId(format!("{module}.{name}$boxed")),
    }
}

/// Canonical [`SymbolId`] for a runtime class instance table.
pub(crate) fn mangle_runtime_instance(module: &str, name: &str) -> SymbolId {
    SymbolId(format!("{module}.{name}$dict"))
}

/// Canonical [`SymbolId`] for a `foreign` binding.
pub(crate) fn mangle_foreign(module: &str, name: &str, polymorphic: bool) -> SymbolId {
    if polymorphic {
        SymbolId(format!("{module}.{name}$foreign_boxed"))
    } else {
        SymbolId(format!("{module}.{name}$foreign"))
    }
}

fn format_spec_sig(type_args: &[TypeRef], dict_specs: &[DictSpec]) -> String {
    let tys: Vec<String> = type_args.iter().map(ToString::to_string).collect();
    let dicts: Vec<String> = dict_specs
        .iter()
        .map(|d| match d {
            DictSpec::Known(c) => format!("K:{}", c.canonical_text()),
            DictSpec::Runtime => "R".to_owned(),
        })
        .collect();
    format!("T[{}]|D[{}]", tys.join(","), dicts.join(","))
}
