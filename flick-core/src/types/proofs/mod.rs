//! Values of a type with an invariant made where the compiler proves the
//! invariant holds. A plugin declares the invariant of an `unboxed` type
//! (`@invariant$of`), the companions `check : Base -> Bool` and `value : T
//! -> Base`, and the member `prove`. Every application of the type's
//! constructor, the plugin's own code included, is an obligation: the facts
//! of its path must prove the check holds for its argument. `T.prove e` is
//! that application where the program writes it. Neither is a function: a
//! use other than applied to one argument is an error. The contract is
//! `docs/formal/refinements.md`.

mod contract;
mod discharge;
mod formula;
mod interval;
mod logic;
#[cfg(test)]
mod props;
mod shown;

use std::collections::{HashMap, HashSet};
use std::mem;
use std::rc::Rc;

pub(super) use formula::Fact;
use formula::{Raw, Value};
use logic::Formula;

use super::Checker;
use super::sugar_lints::unparen;
use super::typing::{Range, Ref, range};
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::numeric::Num;
use crate::source::Span;
use crate::syntax::cst::Node;

/// The invariant of a type, as its block declares it.
pub(super) struct Contract<'a> {
    module: String,
    ty: String,
    constructor: String,
    /// The companions, by their names in the module: `T$check`, `T$value`.
    check: String,
    value: String,
    /// The member `T.prove` spells.
    prove: String,
    /// Whether the compiler proves its check: a type without parameters
    /// over `Int`, `Float`, `String` or `Bool`.
    active: bool,
    /// The attribute, where a contract that does not hold is reported.
    at: Span,
    /// The body of `let T$check = …`.
    body: Option<&'a Node>,
}

/// An application of a constructor whose check must hold.
struct Pending {
    contract: usize,
    subject: Value,
    facts: Vec<Fact>,
    /// The application and its argument.
    span: Span,
    argument: Span,
}

#[derive(Default)]
pub(super) struct Proofs<'a> {
    contracts: Vec<Contract<'a>>,
    pending: Vec<Pending>,
    /// Each use of a constructor with an invariant in an expression, and
    /// whose it is.
    uses: Vec<(Range, Span, usize)>,
    /// The uses applied to an argument.
    applied: HashSet<Range>,
    /// The uses the program writes `T.prove`.
    spelled: HashSet<Range>,
    /// The value an immutable `let y = x` binds `y` to, by `y`'s binder.
    aliases: Vec<(u32, Value)>,
    /// The primitive comparisons, by their range, and the type they compare.
    compared: HashMap<Range, Num>,
}

impl Proofs<'_> {
    /// Whether facts are gathered: the module has an invariant to prove.
    pub(super) fn enabled(&self) -> bool {
        self.contracts.iter().any(|c| c.active)
    }

    fn aliased(&self, at: u32) -> Value {
        let alias = self.aliases.iter().rev().find(|(of, _)| *of == at);
        alias.map_or(Value::Local(at), |(_, v)| v.clone())
    }

    /// The invariant whose `value` companion is `module.name`.
    fn projected_by(&self, module: &str, name: &str) -> Option<usize> {
        (self.contracts.iter()).position(|c| c.active && c.module == module && c.value == name)
    }

    /// The invariant whose check is `module.name`.
    fn checked_by(&self, module: &str, name: &str) -> Option<usize> {
        (self.contracts.iter()).position(|c| c.active && c.module == module && c.check == name)
    }

    /// The invariant whose constructor is `module.name`.
    fn bound(&self, module: &str, name: &str) -> Option<usize> {
        (self.contracts.iter())
            .position(|c| c.active && c.module == module && c.constructor == name)
    }
}

impl Checker<'_> {
    /// Notes that the comparison at `at` settled to the primitive one of `num`.
    pub(super) fn note_compared(&mut self, at: Range, num: Num) {
        if self.proofs.enabled() && matches!(num, Num::Int | Num::Float) {
            self.proofs.compared.insert(at, num);
        }
    }

    /// Checks `body` with `fact` known to have run to `result`.
    pub(super) fn with_fact<R>(
        &mut self,
        fact: Option<&Rc<Formula<Raw>>>,
        result: bool,
        body: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let Some(fact) = fact else {
            return body(self);
        };
        let mark = self.cx.facts.len();
        self.cx.facts.push((Rc::clone(fact), result));
        let r = body(self);
        self.cx.facts.truncate(mark);
        r
    }

    /// A body that runs apart, a function's or a task's, starts with no
    /// facts: when it runs, the path that made it may tell nothing.
    pub(super) fn set_facts_aside(&mut self) {
        let facts = mem::take(&mut self.cx.facts);
        self.cx.held_facts.push(facts);
    }

    pub(super) fn take_facts_back(&mut self) {
        self.cx.facts = self.cx.held_facts.pop().unwrap_or_default();
    }

    /// The value `let` binds to the binder at `at` when it names another:
    /// facts of either are facts of both. Read before the binder is in
    /// scope.
    pub(super) fn let_alias(&self, parts: &super::bindings::LetParts) -> Option<Value> {
        if !self.proofs.enabled() || parts.is_function() || parts.rec {
            return None;
        }
        let body = unparen(parts.body?);
        let mut statements = body.nodes();
        let only = match (statements.next(), statements.next()) {
            (Some(only), None) => only,
            _ => body,
        };
        self.proof_value(only)
    }

    pub(super) fn proof_alias(&mut self, at: u32, value: Option<Value>) {
        if let Some(value) = value {
            self.proofs.aliases.push((at, value));
        }
    }

    /// Notes a use of a constructor with an invariant, in an expression.
    pub(super) fn note_constructor(&mut self, module: &str, name: &str, span: Span) {
        if let Some(index) = self.proofs.bound(module, name) {
            self.proofs.uses.push((range(span), span, index));
        }
    }

    /// `T.prove`: the constructor of `T` where `T` has an invariant whose
    /// member it is. Not a use of it applied yet.
    pub(super) fn prove_member(
        &mut self,
        ty: &Entity,
        member: &str,
        span: Span,
    ) -> Option<super::ty::Type> {
        let contract = (self.proofs.contracts.iter())
            .find(|c| c.module == ty.module && c.ty == ty.name && c.prove == member)?;
        if !contract.active {
            let message = format!(
                "cannot prove the check of `{}`: the compiler proves the check of a type without parameters over `Int`, `Float`, `String` or `Bool`; `{}.parse` checks a value when the program runs",
                ty.name, ty.name
            );
            self.error(codes::T077, span, message);
            return Some(super::ty::Type::Error);
        }
        let entity = Entity {
            module: contract.module.clone(),
            namespace: Namespace::Constructor,
            name: contract.constructor.clone(),
        };
        let scheme = self.env.value(&entity).cloned()?;
        let t = self.instantiate(&self.live_scheme(&scheme), span);
        self.proofs.spelled.insert(range(span));
        self.record_entity(&entity, &t, span);
        Some(t)
    }

    /// `head args` where `head` is a constructor with an invariant: the
    /// check must hold for the first argument.
    pub(super) fn note_construction(&mut self, head: &Node, args: &[&Node]) {
        let at = head.range();
        let Some(Ref::Constructor(c)) = self.typing.names.get(&at) else {
            return;
        };
        let (Some(contract), Some(argument)) =
            (self.proofs.bound(&c.module, &c.name), args.first())
        else {
            return;
        };
        self.proofs.applied.insert(at);
        let subject = (self.proof_value(argument))
            .or_else(|| self.proof_literal(argument).map(Value::Lit))
            .unwrap_or_else(|| Value::Once(unparen(argument).range()));
        let span = Span::new(self.file, at.0, argument.range().1);
        self.proofs.pending.push(Pending {
            contract,
            subject,
            facts: self.cx.facts.clone(),
            span,
            argument: self.span(argument),
        });
    }
}
