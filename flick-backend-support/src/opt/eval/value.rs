//! Runtime value domain and immutable lexical/dictionary environments for
//! the bounded portable-IR evaluator.

use std::rc::Rc;

use flick_ir::{Constructor, Dict, Expr, Param};

use super::{EvalBudget, StopReason};
use crate::numeric;

/// Accounting cost in bytes per slot in a composite value (tuple, list, constructor).
pub(super) const SLOT_BYTES: u32 = 8;
/// Accounting cost in bytes per field in a record or dictionary value.
pub(super) const FIELD_BYTES: u32 = 16;
/// Accounting cost in bytes for constructing a closure or method value.
pub(super) const CLOSURE_BYTES: u32 = 32;

/// Concrete value produced by bounded evaluation of portable Flick IR.
///
/// Internal values such as [`Value::Closure`], [`Value::Dict`], [`Value::Method`],
/// [`Value::Primitive`], [`Value::OpaqueGlobal`], and lone-surrogate [`Value::Utf16`]
/// may be consumed by enclosing computations without being materialized into IR.
#[derive(Debug, Clone)]
pub enum Value<'a> {
    Unit,
    Bool(bool),
    Int(i32),
    Float(f64),
    BigInt(i128),
    /// Valid UTF-8 string value.
    String(Rc<str>),
    /// Exact UTF-16 code units (supports target primitives like `String.slice`
    /// that may produce lone surrogates internally without materializing).
    Utf16(Rc<[u16]>),
    /// Constructor value (partially or fully applied).
    Con {
        ctor: Rc<Constructor>,
        args: Rc<[Value<'a>]>,
        dicts: Rc<[DictValue<'a>]>,
    },
    Tuple(Rc<[Value<'a>]>),
    List(Rc<[Value<'a>]>),
    /// Record fields preserved in exact source order.
    Record(Rc<[(Rc<str>, Value<'a>)]>),
    /// Lexical closure capturing its defining environment and dictionaries.
    Closure(Rc<Closure<'a>>),
    /// Resolved class dictionary evidence.
    Dict(DictValue<'a>),
    /// Bound method on a resolved dictionary (e.g. built-in `Numeric`/`Show`/`Display`).
    Method {
        dict: DictValue<'a>,
        name: Rc<str>,
        args: Rc<[Value<'a>]>,
    },
    /// Reviewed target primitive awaiting its remaining pure arguments.
    Primitive {
        module: Rc<str>,
        name: Rc<str>,
        arity: usize,
        dicts: Rc<[DictValue<'a>]>,
        args: Rc<[Value<'a>]>,
    },
    /// Inert reference to a global without an available pure Flick IR body.
    OpaqueGlobal {
        module: Rc<str>,
        name: Rc<str>,
    },
}

/// A closure carrying its defining body, remaining parameters, and captured values.
#[derive(Debug, Clone)]
pub struct Closure<'a> {
    pub env: Env<'a>,
    pub dict_env: DictEnv<'a>,
    pub dict_params: &'a [String],
    pub params: &'a [Param],
    pub body: &'a Expr,
    pub rec_group: Option<Rc<RecGroup<'a>>>,
}

/// A group of mutually recursive local functions bound together in a block.
///
/// The group captures the lexical environment *outside* the recursive bindings
/// so there are no reference cycles.
#[derive(Debug, Clone)]
pub struct RecGroup<'a> {
    pub env: Env<'a>,
    pub dict_env: DictEnv<'a>,
    pub defs: Rc<[RecFun<'a>]>,
}

/// One member of a [`RecGroup`].
#[derive(Debug, Clone)]
pub struct RecFun<'a> {
    pub name: Rc<str>,
    pub dict_params: &'a [String],
    pub params: &'a [Param],
    pub body: &'a Expr,
}

/// Resolved class dictionary evidence inside the evaluator.
#[derive(Debug, Clone)]
pub enum DictValue<'a> {
    /// Known class instance applied to resolved context dictionaries.
    Instance {
        module: Rc<str>,
        name: Rc<str>,
        args: Rc<[DictValue<'a>]>,
    },
    /// Superclass projection of a resolved dictionary.
    Super {
        dict: Box<DictValue<'a>>,
        class: Rc<str>,
    },
    /// Explicitly constructed dictionary record (`ExprKind::Dictionary`).
    Record(Rc<[(Rc<str>, Value<'a>)]>),
}

impl DictValue<'_> {
    /// Converts a resolved instance/super dictionary back to a portable IR [`Dict`],
    /// when representable as one (not an explicit runtime record).
    pub(super) fn to_ir_dict(&self) -> Option<Dict> {
        match self {
            DictValue::Instance { module, name, args } => {
                let mut out = Vec::with_capacity(args.len());
                for arg in args.iter() {
                    out.push(arg.to_ir_dict()?);
                }
                Some(Dict::Instance {
                    module: module.to_string(),
                    name: name.to_string(),
                    args: out,
                })
            }
            DictValue::Super { dict, class } => Some(Dict::Super {
                dict: Box::new(dict.to_ir_dict()?),
                class: class.to_string(),
            }),
            DictValue::Record(_) => None,
        }
    }
}

/// Immutable persistent lexical environment mapping local names to values
/// (or explicit dynamic shadows).
#[derive(Debug, Clone, Default)]
pub struct Env<'a>(Option<Rc<EnvNode<'a>>>);

#[derive(Debug)]
struct EnvNode<'a> {
    name: Rc<str>,
    /// `None` explicitly shadows an outer binding with a dynamic/unknown local.
    value: Option<Value<'a>>,
    next: Env<'a>,
}

impl<'a> Env<'a> {
    #[must_use]
    pub fn with(&self, name: Rc<str>, value: Value<'a>) -> Self {
        Env(Some(Rc::new(EnvNode {
            name,
            value: Some(value),
            next: self.clone(),
        })))
    }

    /// Explicitly marks `name` as dynamically bound (shadowing any outer static value).
    #[must_use]
    pub fn shadow(&self, name: Rc<str>) -> Self {
        Env(Some(Rc::new(EnvNode {
            name,
            value: None,
            next: self.clone(),
        })))
    }

    /// Looks up `name`, charging steps before traversing deep lexical chains.
    pub(super) fn get(
        &self,
        name: &str,
        budget: &mut EvalBudget,
    ) -> Result<Option<&Value<'a>>, StopReason> {
        let mut cursor = self;
        let mut hops: u32 = 0;
        while let Some(node) = &cursor.0 {
            hops += 1;
            if hops.is_multiple_of(8) {
                budget.charge_steps(1)?;
            }
            if &*node.name == name {
                return Ok(node.value.as_ref());
            }
            cursor = &node.next;
        }
        Ok(None)
    }

    /// Non-budgeted lookup used during static walker analysis (bounded by lexical scope).
    pub(super) fn peek(&self, name: &str) -> Option<&Value<'a>> {
        let mut cursor = self;
        while let Some(node) = &cursor.0 {
            if &*node.name == name {
                return node.value.as_ref();
            }
            cursor = &node.next;
        }
        None
    }

    /// Extends the environment with all recursive siblings of `group`.
    pub(super) fn with_rec_group(&self, group: &Rc<RecGroup<'a>>) -> Self {
        let mut env = self.clone();
        for def in group.defs.iter() {
            let closure = Value::Closure(Rc::new(Closure {
                env: group.env.clone(),
                dict_env: group.dict_env.clone(),
                dict_params: def.dict_params,
                params: def.params,
                body: def.body,
                rec_group: Some(Rc::clone(group)),
            }));
            env = env.with(Rc::clone(&def.name), closure);
        }
        env
    }
}

/// Immutable persistent dictionary environment mapping `Dict::Given` names to resolved dictionaries.
#[derive(Debug, Clone, Default)]
pub struct DictEnv<'a>(Option<Rc<DictEnvNode<'a>>>);

#[derive(Debug)]
struct DictEnvNode<'a> {
    name: Rc<str>,
    dict: DictValue<'a>,
    next: DictEnv<'a>,
}

impl<'a> DictEnv<'a> {
    #[must_use]
    pub fn with(&self, name: Rc<str>, dict: DictValue<'a>) -> Self {
        DictEnv(Some(Rc::new(DictEnvNode {
            name,
            dict,
            next: self.clone(),
        })))
    }

    pub(super) fn get(
        &self,
        name: &str,
        budget: &mut EvalBudget,
    ) -> Result<Option<&DictValue<'a>>, StopReason> {
        let mut cursor = self;
        let mut hops: u32 = 0;
        while let Some(node) = &cursor.0 {
            hops += 1;
            if hops.is_multiple_of(8) {
                budget.charge_steps(1)?;
            }
            if &*node.name == name {
                return Ok(Some(&node.dict));
            }
            cursor = &node.next;
        }
        Ok(None)
    }
}

/// Parses canonical IR `BigInt` digits (including an optional leading `-`) into `i128`.
pub(super) fn bigint_value(digits: &str) -> Option<i128> {
    if let Some(rest) = digits.strip_prefix('-') {
        if rest.starts_with('-') {
            return None;
        }
        numeric::bigint_digits_value(rest)?.checked_neg()
    } else {
        numeric::bigint_digits_value(digits)
    }
}
