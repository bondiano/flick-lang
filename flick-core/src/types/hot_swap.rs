//! Hot-swap scheme compatibility and pure migration type checking.
//!
//! Enforces the exact scheme subsumption rule (`σ_new ≤ σ_old`) specified in
//! `docs/formal/hot-swap.md` and proved in `formal/Formal/HotSwap.lean`:
//! - generalizing a scheme is accepted; narrowing is rejected;
//! - contravariant parameter types (including capture/region qualifiers) may
//!   widen, never narrow;
//! - covariant results, effect/service/error rows, and callable capture/region
//!   qualifiers may narrow (or become pure), never widen;
//! - pure `Old -> New` migration functions must have no effect row (`effects: None`).

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::sync::LazyLock;

use crate::diagnostics::{Diagnostic, Severity};
use crate::iface::Interface;
use crate::source::{FileId, Span};
use crate::syntax::structure::Thresholds;
use crate::types::builtins::{self, builtin_env};
use crate::types::env::{Builtins, Env};
use crate::types::kinds::KindStore;
use crate::types::unify::Store;
use crate::types::{
    Checker, ForeignBackendSpec, ItemCx, Limits, Options, Type, Watched, classes, gadt,
    module_contract, proofs, show_type, toplevel, typing::Typing,
};

static EMPTY_WATCHED: LazyLock<Watched> = LazyLock::new(Watched::default);
static EMPTY_FOREIGN_BACKEND: LazyLock<ForeignBackendSpec> =
    LazyLock::new(ForeignBackendSpec::default);
fn all_local(_: &str) -> bool {
    true
}

/// Error returned when a replacement scheme fails hot-swap compatibility
/// (`σ_new ≤ σ_old`) or a migration function fails pure `Old -> New` typing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemeCompatibilityError {
    /// Human-readable reason explaining the violated typing, row, or region rule.
    pub reason: String,
    /// Rendered previous (or expected) scheme.
    pub old_scheme: String,
    /// Rendered proposed replacement (or migration) scheme.
    pub new_scheme: String,
}

impl fmt::Display for SchemeCompatibilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "incompatible hot-swap scheme (`{}` does not subsume `{}`): {}",
            self.new_scheme, self.old_scheme, self.reason
        )
    }
}

impl Error for SchemeCompatibilityError {}

/// Checks whether `new_scheme` subsumes `old_scheme` (`σ_new ≤ σ_old`) under
/// the loaded `interfaces` and `builtins`.
///
/// # Errors
/// Returns [`SchemeCompatibilityError`] if `new_scheme` narrows a polymorphic
/// binder, widens an effect/service/error row, narrows a contravariant
/// parameter capture/region qualifier, widens a covariant callable capture
/// qualifier, or introduces an unsatisfied class/outlives constraint.
pub fn check_scheme_subsumes(
    new_scheme: &Type,
    old_scheme: &Type,
    interfaces: &[Interface],
    builtins: &[Interface],
) -> Result<(), SchemeCompatibilityError> {
    let owned;
    let built = if let Some(by_class) = builtins::are_builtins(builtins) {
        builtin_env(by_class)
    } else {
        owned = Builtins::new(builtins);
        &owned
    };
    let mut checker = standalone_checker(interfaces, built);
    let span = Span::new(FileId::new(0), 0, 0);

    checker.store.level += 1;
    let (old_body, _skolems, old_givens) = checker.skolemize(old_scheme);
    checker.classes.givens.extend(old_givens);
    let new_body = checker.instantiate(new_scheme, span);

    if let Err(mismatch) = checker.subsume(&new_body, &old_body, span) {
        let reason = format!(
            "type mismatch between `{}` and `{}` ({mismatch:?})",
            checker.show(&new_body),
            checker.show(&old_body)
        );
        return Err(SchemeCompatibilityError {
            reason,
            old_scheme: show_type(old_scheme),
            new_scheme: show_type(new_scheme),
        });
    }

    checker.solve_classes(true);
    checker.solve_rows();
    checker.compare_deferred();
    checker.store.level -= 1;

    if let Some(diag) = first_error(&checker.diagnostics) {
        return Err(SchemeCompatibilityError {
            reason: diag.message.clone(),
            old_scheme: show_type(old_scheme),
            new_scheme: show_type(new_scheme),
        });
    }

    Ok(())
}

/// Checks that `migrate_scheme` is a pure function subsuming `old_ty -> new_ty`
/// (with `effects: None`, i.e. no runtime services or errors).
///
/// # Errors
/// Returns [`SchemeCompatibilityError`] if `migrate_scheme` is effectful or
/// does not accept `old_ty` and return `new_ty`.
pub fn check_migration_scheme(
    migrate_scheme: &Type,
    old_ty: &Type,
    new_ty: &Type,
    interfaces: &[Interface],
    builtins: &[Interface],
) -> Result<(), SchemeCompatibilityError> {
    let expected = Type::Fun {
        param: Box::new(old_ty.clone()),
        effects: None,
        result: Box::new(new_ty.clone()),
    };
    check_scheme_subsumes(migrate_scheme, &expected, interfaces, builtins)
}

/// Checks whether two closed types are mutually compatible (both `a ≤ b` and
/// `b ≤ a`) under `interfaces` and `builtins`.
#[must_use]
pub fn types_mutually_compatible(
    left: &Type,
    right: &Type,
    interfaces: &[Interface],
    builtins: &[Interface],
) -> bool {
    check_scheme_subsumes(left, right, interfaces, builtins).is_ok()
        && check_scheme_subsumes(right, left, interfaces, builtins).is_ok()
}

fn first_error(diagnostics: &[Diagnostic]) -> Option<&Diagnostic> {
    diagnostics.iter().find(|d| d.severity == Severity::Error)
}

fn standalone_checker<'a>(interfaces: &[Interface], builtins: &'a Builtins) -> Checker<'a> {
    let env = Env::from_interfaces(interfaces, builtins);
    let store = Store::default();
    let kinds = KindStore::for_witnesses(store.witness_authority());
    Checker {
        file: FileId::new(0),
        text: "",
        generated: 0,
        derived: Vec::new(),
        env,
        static_witnesses: &[],
        store,
        kinds,
        diagnostics: Vec::new(),
        cx: ItemCx::default(),
        signs: HashMap::new(),
        class_signs: HashMap::new(),
        opened: Vec::new(),
        contract_types: Vec::new(),
        opened_modules: Vec::new(),
        witness_counter: 0,
        annotation_contexts: Vec::new(),
        named_witnesses: module_contract::NamedWitnesses::default(),
        sites: Vec::new(),
        literals: Vec::new(),
        signatures: Vec::new(),
        equalities: Vec::new(),
        recs: Vec::new(),
        top: toplevel::TopLevel::default(),
        options: Options {
            local: &all_local,
            limits: Limits::default(),
            no_let_assert: false,
            thresholds: Thresholds::default(),
            watched: &EMPTY_WATCHED,
            stages_by_class: true,
            foreign_backend: &EMPTY_FOREIGN_BACKEND,
            validate_foreign_metadata: false,
            native_overrides: None,
            plugin_attributes: &[],
        },
        classes: classes::State::default(),
        typing: Typing::default(),
        predicates: HashMap::new(),
        proofs: proofs::Proofs::default(),
        cells: HashMap::new(),
        labels: Vec::new(),
        owners: Vec::new(),
        root: None,
        calls: Vec::new(),
        field_path: Vec::new(),
        func_refs: Vec::new(),
        given_param: None,
        piped_late: None,
        holes: Vec::new(),
        hole_ok: false,
        control_effects: HashSet::new(),
        builtin: false,
        sugar: Vec::new(),
        gadt: gadt::branch::State::default(),
        error_rows: None,
        unfolding: Vec::new(),
        skip_ir_types: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Atom, Binder, Effects, Elem, Elided, Kind, Row, builtins};

    fn int_ty() -> Type {
        Type::con("Prelude", "Int")
    }

    fn bool_ty() -> Type {
        Type::con("Prelude", "Bool")
    }

    fn unit_ty() -> Type {
        Type::con("Prelude", "Unit")
    }

    fn pure_fun(param: Type, result: Type) -> Type {
        Type::Fun {
            param: Box::new(param),
            effects: None,
            result: Box::new(result),
        }
    }

    fn rt_fun(param: Type, services: Type, errors: Type, result: Type) -> Type {
        Type::Fun {
            param: Box::new(param),
            effects: Some(Box::new(Effects::new(services, errors))),
            result: Box::new(result),
        }
    }

    fn owner_ty(region: &str) -> Type {
        Type::app(Type::con("Fiber", "Owner"), Type::Var(region.to_owned()))
    }

    fn named_row(module: &str, name: &str) -> Type {
        Type::Row(Row {
            entries: vec![(
                Elem::Named(format!("{module}.{name}")),
                Type::con(module, name),
            )],
            tail: None,
            ..Row::default()
        })
    }

    fn two_row(a: (&str, &str), b: (&str, &str)) -> Type {
        let mut entries = vec![
            (Elem::Named(format!("{}.{}", a.0, a.1)), Type::con(a.0, a.1)),
            (Elem::Named(format!("{}.{}", b.0, b.1)), Type::con(b.0, b.1)),
        ];
        Row::sort_entries(&mut entries);
        Type::Row(Row {
            entries,
            tail: None,
            ..Row::default()
        })
    }

    fn unit_thunk(captures: Option<Type>) -> Type {
        let arrow = rt_fun(unit_ty(), Type::empty_row(), Type::empty_row(), unit_ty());
        match captures {
            Some(caps) => Type::Captured {
                ty: Box::new(arrow),
                captures: Box::new(caps),
                value: false,
                elided: Elided::default(),
            },
            None => arrow,
        }
    }

    fn region_cap(region: &str) -> Type {
        Type::Row(Row {
            entries: vec![(
                Elem::Capture(Atom::Var(region.to_owned())),
                Type::Var(region.to_owned()),
            )],
            tail: None,
            ..Row::default()
        })
    }

    fn cap_var_row(var: &str) -> Type {
        Type::Row(Row {
            entries: Vec::new(),
            tail: Some(Box::new(Type::Var(var.to_owned()))),
            ..Row::default()
        })
    }

    #[test]
    fn generalizes_monomorphic_and_rejects_narrowing_polymorphic() {
        let built = builtins();
        let mono_int = pure_fun(int_ty(), int_ty());
        let poly_id = Type::Forall {
            vars: vec![Binder {
                name: "a".to_owned(),
                kind: Kind::Type,
            }],
            constraints: Vec::new(),
            body: Box::new(pure_fun(
                Type::Var("a".to_owned()),
                Type::Var("a".to_owned()),
            )),
        };

        assert!(
            check_scheme_subsumes(&poly_id, &mono_int, &[], built).is_ok(),
            "generalizing Int -> Int to forall a. a -> a must succeed"
        );
        assert!(
            check_scheme_subsumes(&mono_int, &poly_id, &[], built).is_err(),
            "narrowing forall a. a -> a to Int -> Int must fail"
        );
    }

    #[test]
    fn accepts_effect_narrowing_and_rejects_effect_widening() {
        let built = builtins();
        let log = named_row("Demo", "Log");
        let log_counter = two_row(("Demo", "Log"), ("Demo", "Counter"));
        let oops = named_row("Demo", "Oops");
        let oops_bad = two_row(("Demo", "Oops"), ("Demo", "Bad"));

        let pure_fn = pure_fun(int_ty(), int_ty());
        let rt_empty = rt_fun(int_ty(), Type::empty_row(), Type::empty_row(), int_ty());
        let rt_log = rt_fun(int_ty(), log, Type::empty_row(), int_ty());
        let rt_log_counter = rt_fun(int_ty(), log_counter, Type::empty_row(), int_ty());
        let rt_oops = rt_fun(int_ty(), Type::empty_row(), oops, int_ty());
        let rt_oops_bad = rt_fun(int_ty(), Type::empty_row(), oops_bad, int_ty());
        let poly_row = Type::Forall {
            vars: vec![Binder {
                name: "r".to_owned(),
                kind: Kind::ServiceRow,
            }],
            constraints: Vec::new(),
            body: Box::new(rt_fun(
                int_ty(),
                Type::Var("r".to_owned()),
                Type::empty_row(),
                int_ty(),
            )),
        };

        // Narrowing and generalization accepted:
        assert!(check_scheme_subsumes(&pure_fn, &rt_empty, &[], built).is_ok());
        assert!(check_scheme_subsumes(&pure_fn, &rt_log, &[], built).is_ok());
        assert!(check_scheme_subsumes(&rt_log, &rt_log_counter, &[], built).is_ok());
        assert!(check_scheme_subsumes(&rt_oops, &rt_oops_bad, &[], built).is_ok());
        assert!(check_scheme_subsumes(&poly_row, &rt_log, &[], built).is_ok());

        // Widening rejected:
        assert!(check_scheme_subsumes(&rt_empty, &pure_fn, &[], built).is_err());
        assert!(check_scheme_subsumes(&rt_log, &pure_fn, &[], built).is_err());
        assert!(check_scheme_subsumes(&rt_log_counter, &rt_log, &[], built).is_err());
        assert!(check_scheme_subsumes(&rt_oops_bad, &rt_oops, &[], built).is_err());
        assert!(check_scheme_subsumes(&rt_log, &poly_row, &[], built).is_err());
    }

    #[test]
    fn enforces_contravariant_and_covariant_capture_region_qualifiers() {
        let built = builtins();

        let param_s = Type::Forall {
            vars: vec![Binder {
                name: "s".to_owned(),
                kind: Kind::Region,
            }],
            constraints: Vec::new(),
            body: Box::new(rt_fun(
                unit_thunk(Some(region_cap("s"))),
                Type::empty_row(),
                Type::empty_row(),
                unit_ty(),
            )),
        };
        let param_empty = rt_fun(
            unit_thunk(None),
            Type::empty_row(),
            Type::empty_row(),
            unit_ty(),
        );
        let param_poly_c = Type::Forall {
            vars: vec![Binder {
                name: "c".to_owned(),
                kind: Kind::CaptureRow,
            }],
            constraints: Vec::new(),
            body: Box::new(rt_fun(
                unit_thunk(Some(cap_var_row("c"))),
                Type::empty_row(),
                Type::empty_row(),
                unit_ty(),
            )),
        };

        // Contravariant param capture narrowing `^{s} -> ^∅` rejected:
        assert!(check_scheme_subsumes(&param_empty, &param_s, &[], built).is_err());
        // Contravariant param capture widening `^∅ -> ^c` and `^{s} -> ^c` accepted:
        assert!(check_scheme_subsumes(&param_poly_c, &param_empty, &[], built).is_ok());
        assert!(check_scheme_subsumes(&param_poly_c, &param_s, &[], built).is_ok());

        let thunk_rec = |caps: Option<Type>| {
            Type::Record(Row {
                entries: vec![(Elem::Named("run".to_owned()), unit_thunk(caps))],
                tail: None,
                ..Row::default()
            })
        };
        let ret_empty = Type::Forall {
            vars: vec![Binder {
                name: "s".to_owned(),
                kind: Kind::Region,
            }],
            constraints: Vec::new(),
            body: Box::new(pure_fun(owner_ty("s"), thunk_rec(None))),
        };
        let ret_s = Type::Forall {
            vars: vec![Binder {
                name: "s".to_owned(),
                kind: Kind::Region,
            }],
            constraints: Vec::new(),
            body: Box::new(pure_fun(owner_ty("s"), thunk_rec(Some(region_cap("s"))))),
        };

        // Covariant callable capture widening `^∅ -> ^{s}` rejected:
        assert!(check_scheme_subsumes(&ret_s, &ret_empty, &[], built).is_err());
        // Covariant callable capture shrinking `^{s} -> ^∅` accepted:
        assert!(check_scheme_subsumes(&ret_empty, &ret_s, &[], built).is_ok());
    }

    #[test]
    fn checks_pure_migration_function_scheme() {
        let built = builtins();
        let pair_ty = Type::Tuple(vec![int_ty(), bool_ty()]);
        let valid = pure_fun(int_ty(), pair_ty.clone());
        let effectful = rt_fun(
            int_ty(),
            named_row("Demo", "Log"),
            Type::empty_row(),
            pair_ty.clone(),
        );
        let wrong_ret = pure_fun(int_ty(), bool_ty());

        assert!(check_migration_scheme(&valid, &int_ty(), &pair_ty, &[], built).is_ok());
        assert!(check_migration_scheme(&effectful, &int_ty(), &pair_ty, &[], built).is_err());
        assert!(check_migration_scheme(&wrong_ret, &int_ty(), &pair_ty, &[], built).is_err());
    }
}
