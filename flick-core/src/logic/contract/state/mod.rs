//! Shared deterministic state, symbolic values, linear arithmetic helpers,
//! pattern matching, and static contract admission for `PureTotalV2`.
//!
//! This module contains NO solver search (`omega::decide` is never imported
//! here); it holds only pure data representations and arithmetic shared by
//! [`super::eval`] and [`super::verify`].

mod admit;
mod pat;

pub use admit::{admit_contract, lookup_data};
pub use pat::{PatMatch, SplitTarget, collect_conjuncts, contains_unsat_contracted, match_pat};

use std::rc::Rc;

use crate::logic::omega::Linear;
use crate::logic::term::tuple;
use crate::logic::{Ctor, Data, Refused, TUPLES, Ty, omega};
use crate::source::Span;

use super::defs::AdmittedV2;
use super::term::{IntCmpOp, LangTypes};
use super::{ContractFailure, ContractSpec};

/// Admitted non-contracted helpers and merged data declarations for verifying
/// one contract.
pub struct AdmittedContract {
    pub defs: AdmittedV2,
    pub data: Data,
}

/// Smallest signed 32-bit `Int` value (`-2^31`).
pub const INT_MIN: i128 = i32::MIN as i128;
/// Largest signed 32-bit `Int` value (`2^31 - 1`).
pub const INT_MAX: i128 = i32::MAX as i128;

/// Deterministic step limit for symbolic execution and kernel verification.
pub const MAX_STEPS: u64 = 200_000;
/// Deterministic recursion depth limit (also bounds native and WASM stacks).
pub const MAX_DEPTH: u32 = 200;
/// Deterministic maximum number of explored symbolic paths per contract.
pub const MAX_PATHS: u32 = 1_024;
/// Deterministic shared Omega derivation budget across all queries of one
/// contract.
pub const MAX_OMEGA_BUDGET: u32 = omega::BUDGET;

/// Deterministic resource budgets for contract proving and verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budgets {
    pub max_steps: u64,
    pub max_depth: u32,
    pub max_paths: u32,
    pub omega_budget: u32,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            max_steps: MAX_STEPS,
            max_depth: MAX_DEPTH,
            max_paths: MAX_PATHS,
            omega_budget: MAX_OMEGA_BUDGET,
        }
    }
}

pub type IntVar = usize;
pub type SymId = u32;
pub type LinExpr = Linear;

/// A symbolic runtime value in `PureTotalV2`.
#[derive(Debug, Clone)]
pub enum SymVal {
    Int(LinExpr),
    Bool(bool),
    BoolExpr(BoolFormula),
    Unit,
    Con(Rc<Ctor>, Vec<SymVal>),
    Closure {
        env: SymEnv,
        param: Rc<str>,
        body: Rc<super::term::Term>,
        /// True when created inside an uncontracted runtime helper evaluated in
        /// `SpecHelper` mode, so beta-reducing it keeps `SpecHelper` semantics.
        from_helper: bool,
        /// Call-site span in the caller's module when created inside a foreign
        /// helper or cross-module summary, preserving local diagnostic spans
        /// across curried applications.
        foreign_caller: Option<Span>,
    },
    /// Partially applied call to a verified contracted function.
    Contracted {
        module: String,
        callee: String,
        args: Vec<SymVal>,
    },
    /// Opaque symbolic value (ADT, Rigid) refined by `sym_refinements`.
    Sym(SymId, Ty),
}

/// Persistent linked-list environment for local symbolic bindings.
#[derive(Debug, Clone, Default)]
pub struct SymEnv(Option<Rc<(Rc<str>, SymVal, SymEnv)>>);

impl SymEnv {
    pub fn with(&self, name: Rc<str>, value: SymVal) -> SymEnv {
        SymEnv(Some(Rc::new((name, value, self.clone()))))
    }

    pub fn get(&self, name: &str) -> Option<&SymVal> {
        let mut cursor = self;
        while let Some(cell) = &cursor.0 {
            if &*cell.0 == name {
                return Some(&cell.1);
            }
            cursor = &cell.2;
        }
        None
    }
}

/// Boolean formula over linear integer constraints and symbolic boolean atoms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoolFormula {
    True,
    False,
    Atom(SymId),
    /// `lin >= 0`
    LinGe(LinExpr),
    /// `lin == 0`
    LinEq(LinExpr),
    Not(Box<BoolFormula>),
    And(Box<BoolFormula>, Box<BoolFormula>),
    Or(Box<BoolFormula>, Box<BoolFormula>),
}

impl BoolFormula {
    pub fn not(f: BoolFormula) -> BoolFormula {
        match f {
            BoolFormula::True => BoolFormula::False,
            BoolFormula::False => BoolFormula::True,
            BoolFormula::Not(inner) => *inner,
            other => BoolFormula::Not(Box::new(other)),
        }
    }

    pub fn and(a: BoolFormula, b: BoolFormula) -> BoolFormula {
        match (a, b) {
            (BoolFormula::False, _) | (_, BoolFormula::False) => BoolFormula::False,
            (BoolFormula::True, x) | (x, BoolFormula::True) => x,
            (a, b) => BoolFormula::And(Box::new(a), Box::new(b)),
        }
    }

    pub fn or(a: BoolFormula, b: BoolFormula) -> BoolFormula {
        match (a, b) {
            (BoolFormula::True, _) | (_, BoolFormula::True) => BoolFormula::True,
            (BoolFormula::False, x) | (x, BoolFormula::False) => x,
            (a, b) => BoolFormula::Or(Box::new(a), Box::new(b)),
        }
    }
}

/// Whether evaluation enforces 32-bit `Int` range obligations (`Body`), uses
/// mathematical integer semantics in Z (`Spec`, for inline `require`/`ensure`),
/// or evaluates an uncontracted runtime helper inside a specification
/// (`SpecHelper`, where `+`/`-`/unary `-` are refused so wrapping runtime
/// helpers cannot silently diverge from Z).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalMode {
    Body,
    Spec,
    SpecHelper,
}

/// Snapshot of stack lengths in [`PathState`] for cheap backtracking across
/// branches.
#[derive(Debug, Clone, Copy)]
pub struct StateMark {
    vars: usize,
    facts_len: usize,
    next_sym: SymId,
    bool_facts_len: usize,
    sym_refinements_len: usize,
    call_results_len: usize,
}

/// Path-sensitive symbolic state carried along each execution path.
#[derive(Debug, Clone, Default)]
pub struct PathState {
    pub vars: usize,
    pub facts: Vec<Linear>,
    pub next_sym: SymId,
    pub bool_facts: Vec<(SymId, bool)>,
    pub sym_refinements: Vec<(SymId, SymVal)>,
    pub param_vals: Vec<(String, SymVal)>,
    pub call_results: Vec<(String, IntVar)>,
}

impl PathState {
    pub fn mark(&self) -> StateMark {
        StateMark {
            vars: self.vars,
            facts_len: self.facts.len(),
            next_sym: self.next_sym,
            bool_facts_len: self.bool_facts.len(),
            sym_refinements_len: self.sym_refinements.len(),
            call_results_len: self.call_results.len(),
        }
    }

    pub fn restore(&mut self, mark: StateMark) {
        self.vars = mark.vars;
        self.facts.truncate(mark.facts_len);
        self.next_sym = mark.next_sym;
        self.bool_facts.truncate(mark.bool_facts_len);
        self.sym_refinements.truncate(mark.sym_refinements_len);
        self.call_results.truncate(mark.call_results_len);
    }

    /// Allocates a fresh integer variable bounded to signed 32-bit `Int`
    /// (`[-2147483648, 2147483647]`).
    pub fn fresh_int_var(&mut self) -> IntVar {
        let v = self.vars;
        self.vars += 1;
        // v >= INT_MIN  <=>  v - INT_MIN >= 0
        self.facts.push(Linear {
            coeffs: vec![(v, 1)],
            constant: -INT_MIN,
        });
        // v <= INT_MAX  <=>  -v + INT_MAX >= 0
        self.facts.push(Linear {
            coeffs: vec![(v, -1)],
            constant: INT_MAX,
        });
        v
    }

    pub fn fresh_sym(&mut self) -> SymId {
        let s = self.next_sym;
        self.next_sym += 1;
        s
    }

    pub fn bool_fact(&self, id: SymId) -> Option<bool> {
        self.bool_facts
            .iter()
            .rev()
            .find(|(s, _)| *s == id)
            .map(|(_, b)| *b)
    }

    /// Returns true if `self` already contains a syntactic trivial
    /// contradiction (a constant linear fact `< 0` or contradictory boolean
    /// assignments for the same atom).
    pub fn has_trivial_contradiction(&self) -> bool {
        if self
            .facts
            .iter()
            .any(|f| f.coeffs.is_empty() && f.constant < 0)
        {
            return true;
        }
        for (i, (id_a, val_a)) in self.bool_facts.iter().enumerate() {
            if self.bool_facts[i + 1..]
                .iter()
                .any(|(id_b, val_b)| id_b == id_a && val_b != val_a)
            {
                return true;
            }
        }
        false
    }

    /// Chases constructor/tuple refinements at the head of `val`.
    pub fn whnf(&self, mut val: SymVal) -> SymVal {
        while let SymVal::Sym(id, _) = &val {
            match self.sym_refinements.iter().rev().find(|(s, _)| s == id) {
                Some((_, refined)) => val = refined.clone(),
                None => break,
            }
        }
        val
    }

    /// Allocates a fresh symbolic value of type `ty`, adding `[-2^31, 2^31 - 1]`
    /// bounds for every `Int` (including inside tuples).
    pub fn fresh_val_for_ty(
        &mut self,
        ty: &Ty,
        lang: &LangTypes,
        span: Span,
    ) -> Result<SymVal, ContractFailure> {
        self.fresh_val_for_ty_depth(ty, lang, span, 0)
    }

    fn fresh_val_for_ty_depth(
        &mut self,
        ty: &Ty,
        lang: &LangTypes,
        span: Span,
        depth: u32,
    ) -> Result<SymVal, ContractFailure> {
        if depth >= 64 {
            return Err(ContractFailure::Exhausted { span });
        }
        match ty {
            Ty::Int => {
                let v = self.fresh_int_var();
                Ok(SymVal::Int(lin_var(v)))
            }
            Ty::Bool => {
                let s = self.fresh_sym();
                Ok(SymVal::BoolExpr(BoolFormula::Atom(s)))
            }
            Ty::Data { module, name, args }
                if module == &lang.unit.0 && name == &lang.unit.1 && args.is_empty() =>
            {
                Ok(SymVal::Unit)
            }
            Ty::Data { module, args, .. } if module == TUPLES => {
                let mut fields = Vec::with_capacity(args.len());
                for arg in args {
                    fields.push(self.fresh_val_for_ty_depth(arg, lang, span, depth + 1)?);
                }
                Ok(SymVal::Con(tuple(args.len()), fields))
            }
            Ty::Data { .. } | Ty::Rigid(_) | Ty::Fun(..) => {
                let s = self.fresh_sym();
                Ok(SymVal::Sym(s, ty.clone()))
            }
            Ty::Refused(why) => Err(ContractFailure::Refused(Refused {
                span,
                why: why.clone(),
            })),
        }
    }

    /// Initializes all parameters of `spec` in declaration order, binding them
    /// in the returned [`SymEnv`] and recording them in `self.param_vals`.
    pub fn init_params(
        &mut self,
        spec: &ContractSpec,
        lang: &LangTypes,
    ) -> Result<SymEnv, ContractFailure> {
        let mut env = SymEnv::default();
        for (name, ty) in &spec.params {
            let val = self.fresh_val_for_ty(ty, lang, spec.decl_span)?;
            self.param_vals.push((name.to_string(), val.clone()));
            env = env.with(name.clone(), val);
        }
        Ok(env)
    }
}

pub fn lin_var(v: IntVar) -> Linear {
    Linear {
        coeffs: vec![(v, 1)],
        constant: 0,
    }
}

pub fn lin_const(c: i128) -> Linear {
    Linear::constant(c)
}

pub fn lin_add(a: &Linear, b: &Linear) -> Option<Linear> {
    Linear::combine([(a, 1), (b, 1)])
}

pub fn lin_sub(a: &Linear, b: &Linear) -> Option<Linear> {
    let nb = b.neg()?;
    Linear::combine([(a, 1), (&nb, 1)])
}

pub fn lin_add_const(a: &Linear, k: i128) -> Option<Linear> {
    Some(Linear {
        coeffs: a.coeffs.clone(),
        constant: a.constant.checked_add(k)?,
    })
}

/// Negation of `f >= 0` over integers: `-f - 1 >= 0`.
pub fn lin_not_ge(f: &Linear) -> Option<Linear> {
    let nf = f.neg()?;
    lin_add_const(&nf, -1)
}

/// Returns `(neg_lo, neg_hi)` for proving `INT_MIN <= res <= INT_MAX`:
/// - `neg_lo` represents `res < INT_MIN` (`-res + (INT_MIN - 1) >= 0`).
/// - `neg_hi` represents `res > INT_MAX` (`res - (INT_MAX + 1) >= 0`).
pub fn range_negations(res: &Linear) -> Option<(Linear, Linear)> {
    let neg_lo = lin_add_const(&res.neg()?, INT_MIN - 1)?;
    let neg_hi = lin_add_const(res, -(INT_MAX + 1))?;
    Some((neg_lo, neg_hi))
}

pub fn is_trivially_unsat_linear(f: &Linear) -> bool {
    f.coeffs.is_empty() && f.constant < 0
}

pub fn refuse<T>(span: Span, why: impl Into<String>) -> Result<T, ContractFailure> {
    Err(ContractFailure::Refused(Refused {
        span,
        why: why.into(),
    }))
}

pub fn local_span(span: Span, fallback: Span) -> Span {
    if span.file == fallback.file {
        span
    } else {
        fallback
    }
}

pub fn expect_int(val: &SymVal, state: &PathState, span: Span) -> Result<Linear, ContractFailure> {
    match state.whnf(val.clone()) {
        SymVal::Int(lin) => Ok(lin),
        _ => refuse(span, "an unknown `Int` value is outside the domain"),
    }
}

pub fn to_bool_formula(
    val: &SymVal,
    state: &PathState,
    span: Span,
) -> Result<BoolFormula, ContractFailure> {
    match state.whnf(val.clone()) {
        SymVal::Bool(true) => Ok(BoolFormula::True),
        SymVal::Bool(false) => Ok(BoolFormula::False),
        SymVal::BoolExpr(f) => Ok(f),
        SymVal::Sym(id, Ty::Bool) => Ok(BoolFormula::Atom(id)),
        _ => refuse(span, "an unknown `Bool` value is outside the domain"),
    }
}

pub fn negate_bool_val(
    val: &SymVal,
    state: &PathState,
    span: Span,
) -> Result<SymVal, ContractFailure> {
    let f = to_bool_formula(val, state, span)?;
    Ok(formula_to_sym_val(BoolFormula::not(f)))
}

pub fn formula_to_sym_val(f: BoolFormula) -> SymVal {
    match f {
        BoolFormula::True => SymVal::Bool(true),
        BoolFormula::False => SymVal::Bool(false),
        other => SymVal::BoolExpr(other),
    }
}

pub fn eval_int_cmp(
    op: IntCmpOp,
    lhs: &Linear,
    rhs: &Linear,
    span: Span,
) -> Result<SymVal, ContractFailure> {
    let ovf = || ContractFailure::Exhausted { span };
    let formula = match op {
        IntCmpOp::Eq => {
            let diff = lin_sub(lhs, rhs).ok_or_else(ovf)?;
            if diff.coeffs.is_empty() {
                return Ok(SymVal::Bool(diff.constant == 0));
            }
            BoolFormula::LinEq(diff)
        }
        IntCmpOp::Ne => {
            let diff = lin_sub(lhs, rhs).ok_or_else(ovf)?;
            if diff.coeffs.is_empty() {
                return Ok(SymVal::Bool(diff.constant != 0));
            }
            BoolFormula::Not(Box::new(BoolFormula::LinEq(diff)))
        }
        IntCmpOp::Le => {
            let diff = lin_sub(rhs, lhs).ok_or_else(ovf)?;
            if diff.coeffs.is_empty() {
                return Ok(SymVal::Bool(diff.constant >= 0));
            }
            BoolFormula::LinGe(diff)
        }
        IntCmpOp::Lt => {
            let diff = lin_sub(rhs, lhs)
                .and_then(|d| lin_add_const(&d, -1))
                .ok_or_else(ovf)?;
            if diff.coeffs.is_empty() {
                return Ok(SymVal::Bool(diff.constant >= 0));
            }
            BoolFormula::LinGe(diff)
        }
        IntCmpOp::Ge => {
            let diff = lin_sub(lhs, rhs).ok_or_else(ovf)?;
            if diff.coeffs.is_empty() {
                return Ok(SymVal::Bool(diff.constant >= 0));
            }
            BoolFormula::LinGe(diff)
        }
        IntCmpOp::Gt => {
            let diff = lin_sub(lhs, rhs)
                .and_then(|d| lin_add_const(&d, -1))
                .ok_or_else(ovf)?;
            if diff.coeffs.is_empty() {
                return Ok(SymVal::Bool(diff.constant >= 0));
            }
            BoolFormula::LinGe(diff)
        }
    };
    Ok(SymVal::BoolExpr(formula))
}

pub fn combine_bool_eq(
    eq: bool,
    lhs: &SymVal,
    rhs: &SymVal,
    state: &PathState,
    span: Span,
) -> Result<SymVal, ContractFailure> {
    let fl = to_bool_formula(lhs, state, span)?;
    let fr = to_bool_formula(rhs, state, span)?;
    let res = match (fl, fr) {
        (BoolFormula::True, r) | (r, BoolFormula::True) => {
            if eq {
                r
            } else {
                BoolFormula::not(r)
            }
        }
        (BoolFormula::False, r) | (r, BoolFormula::False) => {
            if eq {
                BoolFormula::not(r)
            } else {
                r
            }
        }
        (l, r) => {
            let nl = BoolFormula::not(l.clone());
            let nr = BoolFormula::not(r.clone());
            if eq {
                BoolFormula::or(BoolFormula::and(l, r), BoolFormula::and(nl, nr))
            } else {
                BoolFormula::or(BoolFormula::and(l, nr), BoolFormula::and(nl, r))
            }
        }
    };
    Ok(formula_to_sym_val(res))
}
