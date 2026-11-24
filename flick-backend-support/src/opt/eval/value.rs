//! Runtime value domain and immutable lexical/dictionary environments for
//! the bounded portable-IR evaluator.

use std::cmp::Ordering;
use std::rc::Rc;
use std::str::Chars;

use flick_ir::{Constructor, Expr, ExprKind, Param, Pat, Stmt};

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
/// Internal values such as [`Value::Callable`], [`Value::Dict`],
/// [`Value::OpaqueGlobal`], and lone-surrogate [`Value::Utf16`] may be consumed
/// by enclosing computations without being materialized into IR.
#[derive(Debug, Clone)]
pub enum Value<'a> {
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
    Float(f64),
    Float32(f32),
    BigInt(i128),
    /// Valid UTF-8 string value.
    String(Rc<str>),
    /// Exact UTF-16 code units (supports target primitives like `String.slice`
    /// that may produce lone surrogates internally without materializing).
    Utf16(Rc<Vec<u16>>),
    /// Constructor value (partially or fully applied).
    Con {
        ctor: Rc<Constructor>,
        args: Rc<Vec<Value<'a>>>,
        dicts: Rc<Vec<DictValue<'a>>>,
    },
    Tuple(Rc<Vec<Value<'a>>>),
    List(Rc<Vec<Value<'a>>>),
    /// Record fields preserved in exact source order.
    Record(Rc<Vec<(Rc<str>, Value<'a>)>>),
    /// Lexical closure, built-in class method, or reviewed target primitive.
    Callable(Rc<Callable<'a>>),
    /// Resolved class dictionary evidence (boxed in `Rc` to keep `Value` compact).
    Dict(Rc<DictValue<'a>>),
    /// Inert reference to a global without an available pure Flick IR body.
    OpaqueGlobal,
}

/// Payload for [`Value::Callable`], unifying closures, built-in methods, and
/// reviewed target primitives under a single `Rc` monomorphization.
#[derive(Debug, Clone)]
pub enum Callable<'a> {
    /// Lexical closure capturing its defining environment (outside `rec_group`,
    /// avoiding reference cycles).
    Closure {
        env: Env<'a>,
        dict_params: &'a [String],
        params: &'a [Param],
        body: &'a Expr,
        /// Non-empty when bound as part of a mutually recursive `Stmt::Fun` group.
        rec_group: &'a [Stmt],
    },
    /// Partially or freshly bound built-in class method awaiting arguments.
    Method {
        dict: DictValue<'a>,
        name: Rc<str>,
        args: Rc<Vec<Value<'a>>>,
    },
    /// Partially applied reviewed target primitive awaiting remaining arguments.
    Primitive {
        module: Rc<str>,
        name: Rc<str>,
        arity: usize,
        dicts: Rc<Vec<DictValue<'a>>>,
        args: Rc<Vec<Value<'a>>>,
    },
}

/// Resolved class dictionary evidence inside the evaluator.
#[derive(Debug, Clone)]
pub enum DictValue<'a> {
    /// Known class instance applied to resolved context dictionaries.
    Instance {
        module: Rc<str>,
        name: Rc<str>,
        args: Rc<Vec<DictValue<'a>>>,
    },
    /// Superclass projection of a resolved dictionary.
    Super {
        dict: Rc<DictValue<'a>>,
        class: Rc<str>,
    },
    /// Explicitly constructed dictionary record (`ExprKind::Dictionary`).
    Record(Rc<Vec<(Rc<str>, Value<'a>)>>),
}

/// Immutable persistent environment mapping lexical names and `Dict::Given`
/// names in disjoint namespaces (distinguished by `is_dict`).
#[derive(Debug, Clone, Default)]
pub struct Env<'a>(Option<Rc<EnvNode<'a>>>);

#[derive(Debug)]
struct EnvNode<'a> {
    name: Rc<str>,
    is_dict: bool,
    /// `None` explicitly shadows an outer local binding with a dynamic/unknown local.
    value: Option<Value<'a>>,
    next: Env<'a>,
}

impl<'a> Env<'a> {
    #[inline(never)]
    fn push(&self, name: Rc<str>, is_dict: bool, value: Option<Value<'a>>) -> Self {
        Env(Some(Rc::new(EnvNode {
            name,
            is_dict,
            value,
            next: self.clone(),
        })))
    }

    #[must_use]
    pub fn with(&self, name: Rc<str>, value: Value<'a>) -> Self {
        self.push(name, false, Some(value))
    }

    #[must_use]
    pub(super) fn with_dict(&self, name: Rc<str>, dict: DictValue<'a>) -> Self {
        self.push(name, true, Some(Value::Dict(Rc::new(dict))))
    }

    /// Explicitly marks `name` as dynamically bound (shadowing any outer static value).
    #[must_use]
    pub fn shadow(&self, name: Rc<str>) -> Self {
        self.push(name, false, None)
    }

    /// Merges dictionary entries from `dicts` onto `self` (used only at `evaluate` entry).
    pub(super) fn merge_dicts(&self, dicts: DictEnv<'a>) -> Self {
        let mut items = Vec::new();
        let mut cursor = dicts.0;
        while let Some(node) = cursor.0 {
            if node.is_dict
                && let Some(val) = &node.value
            {
                items.push((Rc::clone(&node.name), val.clone()));
            }
            cursor = node.next.clone();
        }
        let mut out = self.clone();
        while let Some((name, val)) = items.pop() {
            out = out.push(name, true, Some(val));
        }
        out
    }

    fn lookup(
        &self,
        name: &str,
        is_dict: bool,
        budget: &mut EvalBudget,
    ) -> Result<Option<&Value<'a>>, StopReason> {
        let mut cursor = self;
        let mut hops: u32 = 0;
        while let Some(node) = &cursor.0 {
            hops += 1;
            if hops.is_multiple_of(8) {
                budget.charge_steps(1)?;
            }
            if node.is_dict == is_dict && &*node.name == name {
                return Ok(node.value.as_ref());
            }
            cursor = &node.next;
        }
        Ok(None)
    }

    /// Looks up lexical local `name`, charging steps before traversing deep lexical chains.
    pub(super) fn get(
        &self,
        name: &str,
        budget: &mut EvalBudget,
    ) -> Result<Option<&Value<'a>>, StopReason> {
        self.lookup(name, false, budget)
    }

    /// Looks up `Dict::Given(name)`, charging steps before traversing deep chains.
    pub(super) fn get_dict(
        &self,
        name: &str,
        budget: &mut EvalBudget,
    ) -> Result<Option<&DictValue<'a>>, StopReason> {
        match self.lookup(name, true, budget)? {
            Some(Value::Dict(d)) => Ok(Some(d)),
            _ => Ok(None),
        }
    }

    /// Non-budgeted lookup used during static walker analysis (bounded by lexical scope).
    pub(super) fn peek(&self, name: &str) -> Option<&Value<'a>> {
        let mut cursor = self;
        while let Some(node) = &cursor.0 {
            if !node.is_dict && &*node.name == name {
                return node.value.as_ref();
            }
            cursor = &node.next;
        }
        None
    }

    /// Extends `self` (the lexical environment *outside* the group) with all
    /// validated mutually recursive siblings in `group`.
    pub(super) fn with_rec_group(&self, group: &'a [Stmt]) -> Self {
        let mut env = self.clone();
        for stmt in group {
            if let Stmt::Fun {
                name,
                rec: true,
                dicts,
                value,
                ..
            } = stmt
                && let ExprKind::Lambda { params, body } = &value.kind
            {
                let closure = Value::Callable(Rc::new(Callable::Closure {
                    env: self.clone(),
                    dict_params: dicts.as_slice(),
                    params: params.as_slice(),
                    body: body.as_ref(),
                    rec_group: group,
                }));
                env = env.with(rc_str(name), closure);
            }
        }
        env
    }
}

/// Immutable persistent dictionary environment mapping `Dict::Given` names to resolved dictionaries.
#[derive(Debug, Clone, Default)]
pub struct DictEnv<'a>(Env<'a>);

impl<'a> DictEnv<'a> {
    #[must_use]
    pub fn with(&self, name: Rc<str>, dict: DictValue<'a>) -> Self {
        DictEnv(self.0.with_dict(name, dict))
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.0.is_none()
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

/// Outlined linear field lookup shared across record projection, equality,
/// pattern matching, and dictionary method dispatch.
#[inline(never)]
pub(super) fn find_field<'f, 'a>(
    fields: &'f [(Rc<str>, Value<'a>)],
    label: &str,
) -> Option<&'f Value<'a>> {
    for (k, v) in fields {
        if &**k == label {
            return Some(v);
        }
    }
    None
}

/// Outlined `Rc<str>` constructor avoiding repeated `allocate_for_slice` inlining
/// and avoiding `<Rc<str> as From<String>>` monomorphization in browser WASM.
#[inline(never)]
pub(super) fn rc_str(s: &str) -> Rc<str> {
    Rc::from(s)
}

#[inline(never)]
pub(super) fn empty_values<'a>() -> Rc<Vec<Value<'a>>> {
    Rc::new(Vec::new())
}

#[inline(never)]
pub(super) fn empty_dicts<'a>() -> Rc<Vec<DictValue<'a>>> {
    Rc::new(Vec::new())
}

#[inline(never)]
pub(super) fn empty_fields<'a>() -> Rc<Vec<(Rc<str>, Value<'a>)>> {
    Rc::new(Vec::new())
}

/// Unified UTF-16 code-unit stream over either a UTF-8 `&str` or a `&[u16]`.
pub(super) enum Utf16Source<'a> {
    Utf8 {
        chars: Chars<'a>,
        pending_low: Option<u16>,
    },
    Units(&'a [u16]),
}

impl<'a> Utf16Source<'a> {
    pub(super) fn from_str(s: &'a str) -> Self {
        Utf16Source::Utf8 {
            chars: s.chars(),
            pending_low: None,
        }
    }

    pub(super) fn from_units(units: &'a [u16]) -> Self {
        Utf16Source::Units(units)
    }

    pub(super) fn next_unit(&mut self) -> Option<u16> {
        match self {
            Utf16Source::Utf8 { chars, pending_low } => {
                if let Some(low) = pending_low.take() {
                    return Some(low);
                }
                let ch = chars.next()?;
                let mut buf = [0_u16; 2];
                let enc = ch.encode_utf16(&mut buf);
                if enc.len() == 2 {
                    *pending_low = Some(enc[1]);
                }
                Some(enc[0])
            }
            Utf16Source::Units(units) => {
                let (&first, rest) = units.split_first()?;
                *units = rest;
                Some(first)
            }
        }
    }
}

pub(super) fn cmp_utf16(mut a: Utf16Source<'_>, mut b: Utf16Source<'_>) -> Ordering {
    loop {
        match (a.next_unit(), b.next_unit()) {
            (Some(x), Some(y)) => {
                let ord = x.cmp(&y);
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
        }
    }
}

pub(super) fn eq_utf16(a: Utf16Source<'_>, b: Utf16Source<'_>) -> bool {
    cmp_utf16(a, b) == Ordering::Equal
}

/// Iteratively visits `root` and every nested sub-pattern without host recursion.
pub(super) fn walk_pat<'a>(root: &'a Pat, f: &mut dyn FnMut(&'a Pat)) {
    let mut stack = vec![root];
    while let Some(pat) = stack.pop() {
        f(pat);
        match pat {
            Pat::Wild | Pat::Bind(_) | Pat::Lit(_) => {}
            Pat::Tuple(items) | Pat::Con(_, items) | Pat::Or(items) => {
                for item in items {
                    stack.push(item);
                }
            }
            Pat::List(items, rest) => {
                for item in items {
                    stack.push(item);
                }
                if let Some(rest) = rest {
                    stack.push(rest);
                }
            }
            Pat::Record(fields) => {
                for (_, p) in fields {
                    stack.push(p);
                }
            }
        }
    }
}

/// Visits every pattern directly attached to `e` (optionally including `Lambda` params).
pub(super) fn each_expr_pat<'a>(e: &'a Expr, include_lambda: bool, f: &mut dyn FnMut(&'a Pat)) {
    match &e.kind {
        ExprKind::Match(_, arms) => {
            for arm in arms {
                f(&arm.pat);
            }
        }
        ExprKind::Handle(h) => {
            for arm in &h.errors {
                f(&arm.pat);
            }
        }
        ExprKind::Each { pat, .. } => f(pat),
        ExprKind::Block(stmts, _) => {
            for stmt in stmts {
                if let Stmt::Let { pat, .. } = stmt {
                    f(pat);
                }
            }
        }
        ExprKind::Lambda { params, .. } if include_lambda => {
            for p in params {
                f(&p.pat);
            }
        }
        _ => {}
    }
}

/// Computes exact decimal string byte length of an `i128` without allocating.
pub(super) fn bigint_byte_len(n: i128) -> u32 {
    if n == 0 {
        return 1;
    }
    let sign = u32::from(n < 0);
    let mut mag = n.unsigned_abs();
    let mut digits = 0_u32;
    while mag > 0 {
        mag /= 10;
        digits += 1;
    }
    sign + digits
}
