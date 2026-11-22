//! Layout-relevant types carried in the IR, shared behind [`TypeRef`].
//!
//! Effect, service, error, capture and region rows are omitted because no
//! value layout depends on them. Transparent type aliases are expanded to
//! their bodies during lowering.

mod visit;
pub(crate) mod wire;

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Stage;

/// A shared, interned handle to an [`IrType`].
///
/// Inside a [`Program`](crate::Program), wire serialization interns every
/// reachable `TypeRef` into the program's top-level `types` table and encodes
/// each reference as its integer index. Outside a `Program`, `TypeRef`
/// serializes inline as [`IrType`].
#[derive(Debug, Clone, Eq)]
pub struct TypeRef(Arc<IrType>);

impl TypeRef {
    pub fn new(ty: IrType) -> TypeRef {
        TypeRef(Arc::new(ty))
    }

    /// Whether two handles point to the same interned allocation.
    pub fn ptr_eq(&self, other: &TypeRef) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Deref for TypeRef {
    type Target = IrType;

    fn deref(&self) -> &IrType {
        &self.0
    }
}

impl PartialEq for TypeRef {
    fn eq(&self, other: &TypeRef) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || *self.0 == *other.0
    }
}

impl PartialOrd for TypeRef {
    fn partial_cmp(&self, other: &TypeRef) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TypeRef {
    fn cmp(&self, other: &TypeRef) -> Ordering {
        if Arc::ptr_eq(&self.0, &other.0) {
            Ordering::Equal
        } else {
            self.0.cmp(&other.0)
        }
    }
}

impl Hash for TypeRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&*self.0, f)
    }
}

impl Serialize for TypeRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        wire::serialize_ref(self, serializer)
    }
}

impl<'de> Deserialize<'de> for TypeRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        wire::deserialize_ref(deserializer)
    }
}

/// A class constraint whose dictionary is passed or stored at runtime.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Constraint {
    pub module: String,
    pub class: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<TypeRef>,
}

/// A layout-relevant type in the IR.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum IrType {
    /// A named type constructor applied to its layout-relevant arguments
    /// (e.g. `Prelude.Int` with no args, `Prelude.Option` with `[Int]`).
    Named {
        module: String,
        name: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<TypeRef>,
    },
    /// A type variable: a parameter of an enclosing type declaration,
    /// polymorphic definition, `Forall`, or GADT constructor.
    Var(String),
    /// Higher-kinded application of a non-named head (e.g. `f a`).
    App { head: TypeRef, args: Vec<TypeRef> },
    /// One stage of a function type: `param ->[stage] result`.
    Fun {
        param: TypeRef,
        stage: Stage,
        result: TypeRef,
    },
    /// A tuple `(T1, T2, ...)`.
    Tuple(Vec<TypeRef>),
    /// A closed (`tail: None`) or open (`tail: Some(row_var)`) record row,
    /// with fields sorted in canonical UTF-16 label order.
    Record {
        fields: Vec<(String, TypeRef)>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tail: Option<String>,
    },
    /// A higher-rank polymorphic type `forall vars. constraints => body`.
    Forall {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        vars: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        constraints: Vec<Constraint>,
        body: TypeRef,
    },
    /// The existential of a packed module (`exists binder : contract. body`).
    Exists {
        binder: String,
        contract: TypeRef,
        body: TypeRef,
    },
}

impl IrType {
    /// `(module, name)` when this is a bare named type constructor with no
    /// type arguments (`Int`, `Float`, `Bool`, `String`, …).
    pub fn as_bare_con(&self) -> Option<(&str, &str)> {
        match self {
            IrType::Named { module, name, args } if args.is_empty() => Some((module, name)),
            _ => None,
        }
    }

    /// Whether this is `Named { module, name, .. }`.
    pub fn is_named(&self, module: &str, name: &str) -> bool {
        matches!(self, IrType::Named { module: m, name: n, .. } if m == module && n == name)
    }

    /// Unrolls curried `Fun` arrows into `(params_with_stages, result)`.
    pub fn fun_stages(&self) -> (Vec<(&TypeRef, Stage)>, &IrType) {
        let mut params = Vec::new();
        let mut cur = self;
        while let IrType::Fun {
            param,
            stage,
            result,
        } = cur
        {
            params.push((param, *stage));
            cur = result;
        }
        (params, cur)
    }

    /// Calls `f` on every direct child [`TypeRef`].
    pub fn each_child<'a>(&'a self, f: &mut dyn FnMut(&'a TypeRef)) {
        match self {
            IrType::Named { args, .. } | IrType::Tuple(args) => {
                for a in args {
                    f(a);
                }
            }
            IrType::Var(_) => {}
            IrType::App { head, args } => {
                f(head);
                for a in args {
                    f(a);
                }
            }
            IrType::Fun { param, result, .. } => {
                f(param);
                f(result);
            }
            IrType::Record { fields, .. } => {
                for (_, ty) in fields {
                    f(ty);
                }
            }
            IrType::Forall {
                constraints, body, ..
            } => {
                for c in constraints {
                    for a in &c.args {
                        f(a);
                    }
                }
                f(body);
            }
            IrType::Exists { contract, body, .. } => {
                f(contract);
                f(body);
            }
        }
    }

    /// Substitutes free occurrences of variables mapped in `subst`.
    #[allow(clippy::too_many_lines)]
    pub fn substitute(&self, subst: &[(&str, &TypeRef)], interner: &mut TypeInterner) -> TypeRef {
        if subst.is_empty() {
            return interner.intern(self.clone());
        }
        match self {
            IrType::Var(name) => {
                for &(k, v) in subst {
                    if k == name {
                        return v.clone();
                    }
                }
                interner.var(name)
            }
            IrType::Named { module, name, args } => {
                let args = args.iter().map(|a| a.substitute(subst, interner)).collect();
                interner.intern(IrType::Named {
                    module: module.clone(),
                    name: name.clone(),
                    args,
                })
            }
            IrType::App { head, args } => {
                let head = head.substitute(subst, interner);
                let args: Vec<_> = args.iter().map(|a| a.substitute(subst, interner)).collect();
                match &*head {
                    IrType::Named {
                        module,
                        name,
                        args: head_args,
                    } => {
                        let mut all = head_args.clone();
                        all.extend(args);
                        interner.intern(IrType::Named {
                            module: module.clone(),
                            name: name.clone(),
                            args: all,
                        })
                    }
                    IrType::App {
                        head: inner,
                        args: head_args,
                    } => {
                        let mut all = head_args.clone();
                        all.extend(args);
                        interner.intern(IrType::App {
                            head: inner.clone(),
                            args: all,
                        })
                    }
                    _ => interner.intern(IrType::App { head, args }),
                }
            }
            IrType::Fun {
                param,
                stage,
                result,
            } => {
                let param = param.substitute(subst, interner);
                let result = result.substitute(subst, interner);
                interner.fun(param, *stage, result)
            }
            IrType::Tuple(items) => {
                let items = items
                    .iter()
                    .map(|a| a.substitute(subst, interner))
                    .collect();
                interner.tuple(items)
            }
            IrType::Record { fields, tail } => {
                let mut out_fields: Vec<(String, TypeRef)> = fields
                    .iter()
                    .map(|(l, t)| (l.clone(), t.substitute(subst, interner)))
                    .collect();
                let mut out_tail = tail.clone();
                if let Some(tail_var) = tail
                    && let Some(&(_, replacement)) = subst.iter().find(|&&(k, _)| k == tail_var)
                {
                    match &**replacement {
                        IrType::Record {
                            fields: extra,
                            tail: next_tail,
                        } => {
                            out_fields.extend(extra.iter().cloned());
                            crate::sort_by_utf16(&mut out_fields, |(l, _)| l.as_str());
                            out_tail.clone_from(next_tail);
                        }
                        IrType::Var(v) => out_tail = Some(v.clone()),
                        _ => {}
                    }
                }
                interner.intern(IrType::Record {
                    fields: out_fields,
                    tail: out_tail,
                })
            }
            IrType::Forall {
                vars,
                constraints,
                body,
            } => {
                let filtered: Vec<(&str, &TypeRef)> = subst
                    .iter()
                    .copied()
                    .filter(|&(k, _)| !vars.iter().any(|v| v == k))
                    .collect();
                if filtered.is_empty() {
                    return interner.intern(self.clone());
                }
                let constraints = constraints
                    .iter()
                    .map(|c| Constraint {
                        module: c.module.clone(),
                        class: c.class.clone(),
                        args: c
                            .args
                            .iter()
                            .map(|a| a.substitute(&filtered, interner))
                            .collect(),
                    })
                    .collect();
                let body = body.substitute(&filtered, interner);
                interner.intern(IrType::Forall {
                    vars: vars.clone(),
                    constraints,
                    body,
                })
            }
            IrType::Exists {
                binder,
                contract,
                body,
            } => {
                let contract = contract.substitute(subst, interner);
                let body = body.substitute(subst, interner);
                interner.intern(IrType::Exists {
                    binder: binder.clone(),
                    contract,
                    body,
                })
            }
        }
    }
}

/// Interns [`IrType`] values into deduplicated [`TypeRef`] handles.
#[derive(Debug, Default)]
pub struct TypeInterner {
    table: BTreeMap<IrType, TypeRef>,
}

impl TypeInterner {
    pub fn new() -> TypeInterner {
        TypeInterner::default()
    }

    pub fn intern(&mut self, ty: IrType) -> TypeRef {
        if let Some(existing) = self.table.get(&ty) {
            return existing.clone();
        }
        let handle = TypeRef::new(ty.clone());
        self.table.insert(ty, handle.clone());
        handle
    }

    pub fn named(
        &mut self,
        module: impl Into<String>,
        name: impl Into<String>,
        args: Vec<TypeRef>,
    ) -> TypeRef {
        self.intern(IrType::Named {
            module: module.into(),
            name: name.into(),
            args,
        })
    }

    pub fn var(&mut self, name: impl Into<String>) -> TypeRef {
        self.intern(IrType::Var(name.into()))
    }

    pub fn fun(&mut self, param: TypeRef, stage: Stage, result: TypeRef) -> TypeRef {
        self.intern(IrType::Fun {
            param,
            stage,
            result,
        })
    }

    pub fn tuple(&mut self, items: Vec<TypeRef>) -> TypeRef {
        self.intern(IrType::Tuple(items))
    }

    pub fn record(&mut self, mut fields: Vec<(String, TypeRef)>, tail: Option<String>) -> TypeRef {
        crate::sort_by_utf16(&mut fields, |(l, _)| l.as_str());
        self.intern(IrType::Record { fields, tail })
    }
}

impl fmt::Display for Constraint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.module, self.class)?;
        for arg in &self.args {
            write!(f, " ")?;
            write_atom(f, arg)?;
        }
        Ok(())
    }
}

impl fmt::Display for IrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IrType::Named { module, name, args } => {
                write!(f, "{module}.{name}")?;
                for arg in args {
                    write!(f, " ")?;
                    write_atom(f, arg)?;
                }
                Ok(())
            }
            IrType::Var(name) => write!(f, "{name}"),
            IrType::App { head, args } => {
                write_atom(f, head)?;
                for arg in args {
                    write!(f, " ")?;
                    write_atom(f, arg)?;
                }
                Ok(())
            }
            IrType::Fun {
                param,
                stage,
                result,
            } => {
                if matches!(
                    &**param,
                    IrType::Fun { .. } | IrType::Forall { .. } | IrType::Exists { .. }
                ) {
                    write!(f, "({param})")?;
                } else {
                    write!(f, "{param}")?;
                }
                let arrow = match stage {
                    Stage::Pure => "->",
                    Stage::Sync => "->[sync]",
                    Stage::Runtime => "->[rt]",
                };
                write!(f, " {arrow} {result}")
            }
            IrType::Tuple(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, ")")
            }
            IrType::Record { fields, tail } => {
                write!(f, "{{")?;
                for (i, (label, ty)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{label} : {ty}")?;
                }
                if let Some(tail) = tail {
                    if !fields.is_empty() {
                        write!(f, " | ")?;
                    }
                    write!(f, "{tail}")?;
                }
                write!(f, "}}")
            }
            IrType::Forall {
                vars,
                constraints,
                body,
            } => {
                if !vars.is_empty() {
                    write!(f, "forall {}. ", vars.join(" "))?;
                }
                if !constraints.is_empty() {
                    if constraints.len() == 1 {
                        write!(f, "{} => ", constraints[0])?;
                    } else {
                        write!(f, "(")?;
                        for (i, c) in constraints.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }
                            write!(f, "{c}")?;
                        }
                        write!(f, ") => ")?;
                    }
                }
                write!(f, "{body}")
            }
            IrType::Exists {
                binder,
                contract,
                body,
            } => write!(f, "exists {binder} : {contract}. {body}"),
        }
    }
}

fn write_atom(f: &mut fmt::Formatter<'_>, ty: &IrType) -> fmt::Result {
    let compound = matches!(
        ty,
        IrType::Named { args, .. } if !args.is_empty()
    ) || matches!(
        ty,
        IrType::App { .. } | IrType::Fun { .. } | IrType::Forall { .. } | IrType::Exists { .. }
    );
    if compound {
        write!(f, "({ty})")
    } else {
        write!(f, "{ty}")
    }
}
