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
mod formula;
mod logic;

use std::collections::HashSet;
use std::mem;
use std::rc::Rc;

pub(super) use formula::Fact;
use formula::{Atom, Raw, Value};
use logic::{Formula, Knowledge};

use super::Checker;
use super::sugar_lints::unparen;
use super::typing::{Range, Ref, range};
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

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
            self.error(codes::T076, span, message);
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
        let subject =
            (self.proof_value(argument)).unwrap_or_else(|| Value::Once(unparen(argument).range()));
        let span = Span::new(self.file, at.0, argument.range().1);
        self.proofs.pending.push(Pending {
            contract,
            subject,
            facts: self.cx.facts.clone(),
            span,
            argument: self.span(argument),
        });
    }

    /// Decides each obligation once the module is solved: its operators and
    /// its dictionaries are known.
    pub(super) fn discharge_proofs(&mut self) {
        if self.proofs.contracts.is_empty() {
            return;
        }
        self.validate_contracts();
        for (at, span, index) in mem::take(&mut self.proofs.uses) {
            // a contract that does not hold is reported, and gives nothing
            if self.proofs.applied.contains(&at) || !self.proofs.contracts[index].active {
                continue;
            }
            let shown = self.operation_name(index, at);
            let message = format!(
                "`{shown}` makes a value only applied to one: it is no function to name alone, pass or apply in part"
            );
            self.error(codes::T078, span, message);
        }
        let summaries: Vec<Option<Formula<Atom>>> = (0..self.proofs.contracts.len())
            .map(|i| self.check_summary(i))
            .collect();
        for pending in mem::take(&mut self.proofs.pending) {
            if self.proofs.contracts[pending.contract].active {
                self.decide_proof(&pending, &summaries);
            }
        }
    }

    fn operation_name(&self, index: usize, at: Range) -> String {
        let contract = &self.proofs.contracts[index];
        if self.proofs.spelled.contains(&at) {
            format!("{}.{}", contract.ty, contract.prove)
        } else {
            contract.constructor.clone()
        }
    }

    fn decide_proof(&mut self, pending: &Pending, summaries: &[Option<Formula<Atom>>]) {
        let contract = &self.proofs.contracts[pending.contract];
        let check = Formula::Atom(Atom::Call {
            module: contract.module.clone(),
            name: contract.check.clone(),
            argument: pending.subject.clone(),
        });
        let mut facts: Vec<(Rc<Formula<Atom>>, bool)> = (pending.facts.iter())
            .map(|(f, b)| (Rc::new(self.settle_fact(f)), *b))
            .collect();
        // each `T.value v` the facts name holds the invariant of `T`
        let mut projections = Vec::new();
        projections_in(&pending.subject, &mut projections);
        for (fact, _) in &facts {
            fact.each_atom(&mut |a| match a {
                Atom::Call { argument, .. } | Atom::Truth(argument) => {
                    projections_in(argument, &mut projections);
                }
            });
        }
        for (index, of) in projections {
            let held = &self.proofs.contracts[index];
            let atom = Atom::Call {
                module: held.module.clone(),
                name: held.check.clone(),
                argument: Value::Projection(index, of),
            };
            facts.push((Rc::new(Formula::Atom(atom)), true));
        }
        let unfold = |atom: &Atom| {
            let Atom::Call {
                module,
                name,
                argument,
            } = atom
            else {
                return None;
            };
            let index = self.proofs.checked_by(module, name)?;
            let summary = summaries[index].as_ref()?;
            Some(summary.map(&mut |a| Formula::Atom(a.given(argument))))
        };
        let knowledge = Knowledge::of(&facts, &unfold);
        let summary = (summaries[pending.contract].as_ref())
            .map(|s| s.map(&mut |a: &Atom| Formula::Atom(a.given(&pending.subject))));
        let results = [Some(&check), summary.as_ref()].map(|f| f.and_then(|f| knowledge.holds(f)));
        if results.contains(&Some(true)) {
            return;
        }
        let ty = contract.ty.clone();
        let argument = &self.text[pending.argument.start as usize..pending.argument.end as usize];
        let runtime = format!("`{ty}.parse {argument}` checks it when the program runs");
        let wanted = summary.as_ref().unwrap_or(&check);
        if results.contains(&Some(false)) {
            let part = (knowledge.missing(wanted, true)).map_or_else(String::new, |f| {
                format!(": `{}` is false here", self.proof_shown(f))
            });
            let message = format!("the check of `{ty}` fails for this value{part}; {runtime}");
            self.error(codes::T077, pending.span, message);
            return;
        }
        let part = match knowledge.missing(wanted, true) {
            Some(f) => format!("nothing here establishes `{}`", self.proof_shown(f)),
            None => "its check goes beyond what the compiler reads".to_owned(),
        };
        let message =
            format!("cannot prove the check of `{ty}` holds for this value: {part}; {runtime}");
        self.error(codes::T076, pending.span, message);
    }

    /// The atoms of `fact` that stay atoms once the module is solved: a call
    /// that takes a dictionary depends on more than its argument.
    fn settle_fact(&self, fact: &Formula<Raw>) -> Formula<Atom> {
        fact.map(&mut |raw: &Raw| {
            let dictionaries = self.typing.dicts.get(&raw.at);
            if matches!(raw.atom, Atom::Call { .. }) && dictionaries.is_some_and(|d| !d.is_empty())
            {
                return Formula::Opaque;
            }
            Formula::Atom(raw.atom.clone())
        })
    }

    /// What the check of the invariant at `index` is, of its parameter: the
    /// body of a `fun`, or a call of the function it names.
    fn check_summary(&self, index: usize) -> Option<Formula<Atom>> {
        let body = self.proofs.contracts[index].body?;
        let mut statements = unparen(body).nodes();
        let node = match (statements.next(), statements.next()) {
            (Some(only), None) => unparen(only),
            _ => return None,
        };
        let mut size = 64;
        let raw = match node.kind {
            K::FunExpr => {
                let mut params = node.nodes().filter(|n| super::patterns::is_pattern(n.kind));
                let (Some(param), None) = (params.next(), params.next()) else {
                    return None;
                };
                let name = formula::word(param, self.text)?;
                let lambda = node.nodes().find(|n| n.kind == K::Block)?;
                let parameter = |n: &Node| {
                    let n = unparen(n);
                    let local = self.typing.names.get(&n.range()) == Some(&Ref::Local);
                    (local && formula::word(n, self.text) == Some(name)).then_some(Value::Param)
                };
                if param.kind != K::VarPat {
                    return None;
                }
                self.proof_formula(lambda, &mut size, &parameter)
            }
            K::NameExpr => match self.typing.names.get(&node.range()) {
                Some(Ref::Value { module, name }) => Formula::Atom(Raw {
                    atom: Atom::Call {
                        module: module.clone(),
                        name: name.clone(),
                        argument: Value::Param,
                    },
                    at: node.range(),
                }),
                _ => return None,
            },
            _ => return None,
        };
        Some(self.settle_fact(&raw))
    }

    fn proof_shown(&self, formula: &Formula<Atom>) -> String {
        match formula {
            Formula::Const(b) => b.to_string(),
            Formula::Atom(Atom::Truth(v)) => self.proof_value_shown(v),
            Formula::Atom(Atom::Call {
                module,
                name,
                argument,
            }) => {
                let name = name.replace('$', ".");
                let callee = match self.env.qualifier_of(module) {
                    Some(q) if module != &self.env.module => format!("{q}.{name}"),
                    _ => name,
                };
                format!("{callee} {}", self.proof_argument_shown(argument))
            }
            Formula::Not(x) => format!("not ({})", self.proof_shown(x)),
            Formula::And(x, y) => format!("{} && {}", self.proof_shown(x), self.proof_shown(y)),
            Formula::Or(x, y) => format!("{} || {}", self.proof_shown(x), self.proof_shown(y)),
            Formula::Opaque => "…".to_owned(),
        }
    }

    fn proof_argument_shown(&self, value: &Value) -> String {
        match value {
            Value::Projection(..) => format!("({})", self.proof_value_shown(value)),
            _ => self.proof_value_shown(value),
        }
    }

    fn proof_value_shown(&self, value: &Value) -> String {
        match value {
            Value::Local(at) => {
                let rest = &self.text[*at as usize..];
                let end = rest
                    .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '\''))
                    .unwrap_or(rest.len());
                rest[..end].to_owned()
            }
            Value::Projection(index, of) => {
                let ty = &self.proofs.contracts[*index].ty;
                format!("{ty}.value {}", self.proof_argument_shown(of))
            }
            Value::Once((start, end)) => self.text[*start as usize..*end as usize].to_owned(),
            Value::Param => "_".to_owned(),
        }
    }
}

/// The projections `value` is or holds, innermost first.
fn projections_in(value: &Value, out: &mut Vec<(usize, Rc<Value>)>) {
    if let Value::Projection(index, of) = value {
        projections_in(of, out);
        if !out.iter().any(|(i, v)| i == index && v == of) {
            out.push((*index, Rc::clone(of)));
        }
    }
}
