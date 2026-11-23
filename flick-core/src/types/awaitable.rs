//! Elaborates the `await` prefix as an application of `Awaitable.await`.

use super::Checker;
use super::classes::{Dict, Outcome, args_of};
use super::infer::eff;
use super::kinds::Kind;
use super::lang::{self, Lang};
use super::ty::{Effects, Type};
use super::typing::Range;
use crate::diagnostics::codes;
use crate::numeric::default_num;
use crate::source::Span;
use crate::syntax::cst::Node;

impl Checker<'_> {
    /// Settle deferred projections before defaulting their row metavariables.
    pub(in crate::types) fn settle_awaitable_constraints(&mut self) {
        if self
            .classes
            .wanted
            .iter()
            .any(|w| w.await_operand.is_some())
        {
            self.solve_classes(false);
            for w in &self.classes.wanted {
                if w.await_operand.is_some() {
                    self.store.lower(&w.constraint);
                }
            }
        }
    }

    pub(super) fn await_expression(&mut self, node: &Node, operand: &Node, ty: &Type) -> Type {
        let input = self.whnf(ty);
        if matches!(input, Type::Error) {
            return Type::Error;
        }
        self.solve_classes(false);
        let input = self.whnf(ty);
        let givens = self.closure(&self.classes.givens);
        let class = Type::lang(Lang::Awaitable);
        let span = self.span(node);
        let operand_span = self.span(operand);
        if input
            .spine()
            .0
            .inference_meta_root()
            .is_some_and(|m| !self.store.is_numeric(m))
        {
            // A callback body can be checked before a later argument fixes
            // its input. Leave the class and its rows open until that point.
            let services = self.store.fresh_of(&Kind::ServiceRow);
            let errors = self.store.fresh_error_row();
            let result = self.fresh();
            let constraint = Type::apps(
                class,
                [input, services.clone(), errors.clone(), result.clone()],
            );
            self.want_awaitable(constraint, span, operand_span);
            self.await_effects(ty, services, errors, span);
            return result;
        }
        let constraint = self
            .project_constraint(&class, &input, &givens)
            .or_else(|| {
                // Some foreign opaque aliases need a productivity witness to expose
                // their Eff head. Dispatch still uses the resulting type class.
                let (r, e, a) = self.known_eff_parts(&input)?;
                self.project_constraint(&class, &eff(r, e, a), &givens)
            })
            .or_else(|| {
                let canonical = self.canon(&input);
                self.project_constraint(&class, &canonical, &givens)
            });
        let Some(constraint) = constraint else {
            return self.missing_awaitable(operand_span, ty);
        };
        let dict = match self.entail(&constraint, &givens, &mut Vec::new()) {
            Outcome::Solved(Dict::Error) => return Type::Error,
            Outcome::Solved(dict) => dict,
            Outcome::Budget(chain) => {
                self.over_budget(&constraint, &chain, span);
                return Type::Error;
            }
            Outcome::Stuck | Outcome::Missing(..) => {
                return self.missing_awaitable(operand_span, ty);
            }
        };
        let args = args_of(&constraint, Lang::Awaitable).expect("an Awaitable constraint");
        let [_, services, errors, result] = args[..] else {
            unreachable!("Awaitable determines its requirements, errors and result");
        };
        let key = node.range();
        self.record_await_worker(key, &dict, result);
        if self.lazy_local(operand) {
            self.typing.read_lazy.insert(key);
        }
        self.record_dict(span, 0, dict);
        self.await_effects(ty, services.clone(), errors.clone(), span);
        result.clone()
    }

    fn await_effects(&mut self, ty: &Type, services: Type, errors: Type, span: Span) {
        let services = self.live_await(ty, services);
        let effects = Effects {
            sync: false,
            ..Effects::new(services, errors)
        };
        self.call_effects(&effects, span);
    }

    pub(in crate::types) fn record_await_worker(&mut self, key: Range, dict: &Dict, result: &Type) {
        if let Some(worker) = self.await_worker(dict) {
            self.typing.builtin_awaits.insert(key, worker);
            self.record_type_args(key, vec![(Kind::Type, result.clone())]);
        }
    }

    /// The workers of the two standard dictionaries, available directly to IR
    /// optimizations. A supplied dictionary still uses the class method.
    fn await_worker(&self, dict: &Dict) -> Option<Lang> {
        let Dict::Instance { module, name, .. } = dict else {
            return None;
        };
        if module != &lang::item(Lang::Instances).module {
            return None;
        }
        let instance = self
            .env
            .instances()
            .find(|i| &i.module == module && &i.name == name)?;
        let args = args_of(&instance.head, Lang::Awaitable)?;
        let (head, _) = args.first()?.spine();
        if head.is_lang(Lang::Eff) {
            Some(Lang::AwaitEff)
        } else if head.is_lang(Lang::Lazy) {
            Some(Lang::LazyForce)
        } else {
            None
        }
    }

    pub(in crate::types) fn missing_awaitable(&mut self, operand: Span, input: &Type) -> Type {
        let input = self.store.resolve(input);
        let shown = input
            .inference_meta_root()
            .filter(|&m| self.store.is_numeric(m))
            .map_or_else(
                || self.show(&input),
                |m| self.show(&Type::lang(default_num(self.store.is_fractional(m)).lang())),
            );
        self.error(
            codes::E021,
            operand,
            format!("no Awaitable instance for `{shown}`; `await` accepts Eff, Lazy, or another type with an Awaitable instance"),
        );
        Type::Error
    }
}
