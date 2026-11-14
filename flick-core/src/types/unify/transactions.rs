// Append-only solver transactions. Only mutated roots/marks are journalled.

#[derive(Debug)]
enum Undo {
    Meta(u32, MetaState),
    Context(u32, MetaContext),
    Mark(u32, Mark),
}

#[derive(Debug, Clone, Copy)]
enum Mark {
    Capture,
    Error,
    Numeric,
    Fractional,
}

/// An affine, LIFO checkpoint: dropping it without committing or rolling back
/// would leave an active transaction, so callers use the checker trial helper.
#[derive(Debug)]
pub(in crate::types) struct StoreCheckpoint {
    transaction: u64,
    undo: usize,
    allocated: usize,
    level: u32,
    equalities: Vec<Equality>,
    context: Context,
    scopes: Scopes,
}

impl Store {
    pub fn checkpoint(&mut self) -> StoreCheckpoint {
        let transaction = self.next_transaction;
        self.next_transaction = transaction.checked_add(1).expect("few solver transactions");
        self.transactions.push(transaction);
        StoreCheckpoint {
            transaction,
            undo: self.undo.len(),
            allocated: self.metas.len(),
            level: self.level,
            equalities: self.equalities.clone(),
            context: self.active_context.clone(),
            scopes: self.scopes.clone(),
        }
    }

    pub fn commit(&mut self, checkpoint: StoreCheckpoint) {
        assert_eq!(
            self.transactions.last(),
            Some(&checkpoint.transaction),
            "LIFO solver transaction"
        );
        self.transactions.pop();
        if self.transactions.is_empty() {
            self.undo.clear();
        }
        drop(checkpoint.equalities);
    }

    pub fn rollback(&mut self, checkpoint: StoreCheckpoint) {
        assert_eq!(
            self.transactions.last(),
            Some(&checkpoint.transaction),
            "LIFO solver transaction"
        );
        while self.undo.len() > checkpoint.undo {
            match self.undo.pop().expect("journal entry") {
                Undo::Meta(m, state) => self.metas[m as usize] = state,
                Undo::Context(m, context) => self.meta_contexts[m as usize] = context,
                Undo::Mark(m, mark) => {
                    self.marks(mark).remove(&m);
                }
            }
        }
        // Never truncate/reuse IDs retained in Typing or diagnostics. Aborted
        // new roots remain well-formed open variables, not partially solved ones.
        self.metas[checkpoint.allocated..].fill(MetaState::Open {
            level: checkpoint.level,
        });
        for context in &mut self.meta_contexts[checkpoint.allocated..] {
            context.permitted = Context::default();
            context.kind = None;
        }
        self.active_context = checkpoint.context;
        self.scopes = checkpoint.scopes;
        self.level = checkpoint.level;
        self.equalities = checkpoint.equalities;
        self.transactions.pop();
    }

    fn marks(&mut self, mark: Mark) -> &mut HashSet<u32> {
        match mark {
            Mark::Capture => &mut self.capture_rows,
            Mark::Error => &mut self.error_rows,
            Mark::Numeric => &mut self.numeric,
            Mark::Fractional => &mut self.fractional,
        }
    }

    fn mark(&mut self, m: u32, mark: Mark) {
        if self.marks(mark).insert(m) && !self.transactions.is_empty() {
            self.undo.push(Undo::Mark(m, mark));
        }
    }

    fn write(&mut self, m: u32, state: MetaState) {
        let slot = &mut self.metas[m as usize];
        if !self.transactions.is_empty() {
            self.undo.push(Undo::Meta(m, slot.clone()));
        }
        *slot = state;
    }
}

impl Checker<'_> {
    /// Commit successful checking; failed/speculative alternatives restore both
    /// solvers. Witness definitions and allocated IDs deliberately remain live.
    pub(in crate::types) fn inference_trial<T, E>(
        &mut self,
        check: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        let types = self.store.checkpoint();
        let kinds = self.kinds.checkpoint();
        let deferred = self.cx.regions.deferred.len();
        let result = check(self);
        if result.is_ok() {
            self.store.commit(types);
            self.kinds.commit(kinds);
        } else {
            self.store.rollback(types);
            self.kinds.rollback(kinds);
            self.cx.regions.deferred.truncate(deferred);
        }
        result
    }

    pub(in crate::types) fn unifies(&mut self, a: &Type, b: &Type) -> bool {
        self.inference_trial(|this| this.unify(a, b)).is_ok()
    }
}

#[cfg(test)]
mod transaction_tests {
    use super::*;

    fn meta(ty: &Type) -> u32 {
        ty.legacy_meta_root().expect("legacy meta")
    }

    #[test]
    fn failure_restores_binding_levels_marks_and_equalities_without_reusing_ids() {
        let mut store = Store {
            level: 2,
            ..Store::default()
        };
        let original = store.fresh();
        let checkpoint = store.checkpoint();
        store.level = 8;
        let allocated = store.fresh_numeric(true);
        let capture = store.fresh_capture_row();
        let skolem = store.skolem("kept-for-diagnostics");
        store.set(meta(&original), allocated.clone());
        store.mark(meta(&original), Mark::Capture);
        store.equalities.push(Equality {
            skolem: 0,
            ty: original.clone(),
            level: Some(8),
        });
        store.rollback(checkpoint);
        assert_eq!(store.resolve(&original), original);
        assert_eq!(store.resolve(&allocated), allocated);
        assert_eq!(store.resolve(&capture), capture);
        assert_eq!(store.level, 2);
        assert_eq!(store.meta_level(meta(&original)), Some(2));
        assert!(!store.is_capture_row(meta(&original)));
        assert!(!store.is_numeric(meta(&allocated)));
        assert!(!store.is_fractional(meta(&allocated)));
        assert!(store.equalities.is_empty());
        assert_eq!(store.skolem_name(0), "kept-for-diagnostics");
        assert_eq!(skolem, Type::Skolem(0));
        assert_eq!(store.fresh(), Type::meta(3));
    }

    #[test]
    fn an_outer_rollback_undoes_inner_commits_and_transitive_level_lowering() {
        let mut store = Store {
            level: 4,
            ..Store::default()
        };
        let original = store.fresh_error_row();
        let other = store.fresh();
        let outer = store.checkpoint();
        let inner = store.checkpoint();
        store.adjust(&other, 1).expect("no skolem");
        store.set(meta(&original), other.clone());
        store.mark_like(meta(&other), meta(&original));
        store.commit(inner);
        assert_eq!(store.meta_level(meta(&other)), Some(1));
        assert!(store.is_error_row(meta(&other)));
        store.rollback(outer);
        assert_eq!(store.resolve(&original), original);
        assert_eq!(store.meta_level(meta(&other)), Some(4));
        assert!(!store.is_error_row(meta(&other)));
        assert!(store.is_error_row(meta(&original)));
    }

    #[test]
    fn later_component_failure_restores_the_earlier_successful_binding() {
        super::test_checker::with_checker(|checker| {
            let variable = checker.fresh();
            let left = Type::Tuple(vec![variable.clone(), Type::con("Prelude", "Int")]);
            let right = Type::Tuple(vec![
                Type::con("Prelude", "String"),
                Type::con("Prelude", "Bool"),
            ]);
            assert!(checker.unify(&left, &right).is_err());
            assert_eq!(checker.zonk(&variable), variable);
        });
    }

    #[test]
    fn failure_diagnostics_preserve_agreed_constructor_arguments_without_solver_writes() {
        let text = "module Probe exposing (..)\ntype Pg\ntype Template d p r\n";
        super::test_checker::with_source(text, |checker, tree| {
            checker.module(tree);
            assert!(checker.diagnostics.is_empty());
            let dialect = checker.store.fresh_of(&Kind::Type);
            let result = checker.store.fresh_of(&Kind::Type);
            let fields = Type::Record(Row::closed(vec![(
                Elem::named("name"),
                Type::lang(Lang::String),
            )]));
            let expected = Type::apps(
                Type::con("Probe", "Template"),
                [
                    dialect.clone(),
                    Type::Record(Row::closed(Vec::new())),
                    result.clone(),
                ],
            );
            let actual = Type::apps(
                Type::con("Probe", "Template"),
                [Type::con("Probe", "Pg"), fields.clone(), fields.clone()],
            );
            let context = checker.store.witness_context();
            let level = checker.store.level;
            let failure = checker
                .unify(&actual, &expected)
                .expect_err("parameter mismatch");
            assert_eq!(checker.zonk(&dialect), dialect);
            assert_eq!(checker.zonk(&result), result);
            assert_eq!(checker.store.witness_context(), context);
            assert_eq!(checker.store.level, level);
            let span = checker.span(tree);
            checker.mismatch(&failure, &actual, &expected, span);
            assert_eq!(
                checker.diagnostics.last().expect("diagnostic").message,
                "expected `Template Pg {} _`, found `Template Pg { name : String } { name : String }`"
            );
            let accepted = Type::apps(
                Type::con("Probe", "Template"),
                [
                    Type::con("Probe", "Pg"),
                    Type::Record(Row::closed(Vec::new())),
                    fields,
                ],
            );
            checker
                .unify(&accepted, &expected)
                .expect("successful retry after failure");
            assert_eq!(checker.zonk(&dialect), Type::con("Probe", "Pg"));
        });
    }

    #[test]
    fn phantom_alias_fallback_success_does_not_commit_a_failed_argument_probe() {
        use crate::types::{Form, Kind, TypeDef};
        super::test_checker::with_checker(|checker| {
            checker.env.define_type(
                "Phantom",
                TypeDef {
                    kind: Kind::arrow(Kind::Type, Kind::arrow(Kind::Type, Kind::Type)),
                    form: Form::Alias {
                        params: vec!["a".into(), "b".into()],
                        body: Type::Tuple(Vec::new()),
                        opaque: false,
                    },
                },
            );
            let variable = checker.fresh();
            let left = Type::apps(
                Type::con("Probe", "Phantom"),
                [variable.clone(), Type::con("Prelude", "Bool")],
            );
            let right = Type::apps(
                Type::con("Probe", "Phantom"),
                [Type::con("Prelude", "Int"), Type::con("Prelude", "String")],
            );
            assert!(
                !checker.same_application(&left, &right),
                "the argument fast path fails"
            );
            assert!(
                checker.unify(&left, &right).is_ok(),
                "both phantom aliases reduce to Unit"
            );
            assert_eq!(
                checker.zonk(&variable),
                variable,
                "discarded fast-path binding leaked into successful fallback"
            );
        });
    }

    #[test]
    fn a_failed_trial_restores_type_and_kind_solvers_together() {
        use crate::types::Kind;
        super::test_checker::with_checker(|checker| {
            let ty = checker.fresh();
            let kind = checker.kinds.fresh();
            let result = checker.inference_trial(|this| {
                this.unify(&ty, &Type::con("Prelude", "Int"))
                    .expect("first component");
                this.kinds
                    .unify(&kind, &Kind::Symbol)
                    .expect("kind component");
                let _retained_id = this.fresh();
                Err::<(), _>(())
            });
            assert!(result.is_err());
            assert_eq!(checker.zonk(&ty), ty);
            assert_eq!(checker.kinds.resolve(&kind), kind);
            assert_eq!(checker.fresh(), Type::meta(2));
        });
    }

    #[test]
    fn a_later_primitive_project_field_failure_restores_earlier_fields() {
        use crate::types::classes::Outcome;
        use crate::types::lang::Lang;
        super::test_checker::with_checker(|checker| {
            let variable = checker.fresh();
            let record = Type::Record(Row::closed(vec![
                (Elem::named("x"), Type::lang(Lang::Int)),
                (Elem::named("y"), Type::lang(Lang::String)),
            ]));
            let output = Type::Record(Row::closed(vec![
                (Elem::named("x"), variable.clone()),
                (Elem::named("y"), Type::lang(Lang::Bool)),
            ]));
            let constraint = Type::apps(Type::lang(Lang::Project), [record, output]);
            assert!(matches!(
                checker.entails(&constraint, &[]),
                Outcome::Missing(..)
            ));
            assert_eq!(
                checker.zonk(&variable),
                variable,
                "failed primitive leaked its first field"
            );
        });
    }

    #[test]
    fn later_failure_discards_a_qualifier_deferred_by_an_earlier_component() {
        use crate::types::{Atom, Elided};
        super::test_checker::with_checker(|checker| {
            checker.cx.regions.deferring = Some(Span::new(checker.file, 0, 0));
            let unknown = checker.fresh();
            let function = Type::pure_fun(Type::Tuple(Vec::new()), Type::Tuple(Vec::new()));
            let qualified = |captures| Type::Captured {
                ty: Box::new(function.clone()),
                captures: Box::new(captures),
                value: false,
                elided: Elided::default(),
            };
            let left = Type::Tuple(vec![
                qualified(Type::Row(Row::closed(vec![(
                    Elem::Capture(Atom::meta(meta(&unknown))),
                    unknown,
                )]))),
                Type::con("Prelude", "Int"),
            ]);
            let right = Type::Tuple(vec![
                qualified(Type::empty_row()),
                Type::con("Prelude", "Bool"),
            ]);
            assert!(checker.unify(&left, &right).is_err());
            assert!(
                checker.cx.regions.deferred.is_empty(),
                "deferred qualifier from a failed comparison survived"
            );
        });
    }
}

#[cfg(test)]
#[path = "test_checker.rs"]
pub(in crate::types) mod test_checker;
