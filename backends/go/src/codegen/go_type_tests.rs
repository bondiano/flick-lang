//! Unit tests for `OptRepr::of` layout selection and substitution invariance.

use flick_backend_support::Known;
use flick_ir::{Constraint, IrType, Lang, LangItem, Namespace, TypeDecl, TypeRef};

use super::go_type::{OptRepr, TypeDecls};

fn lang_type(role: Lang, name: &str) -> LangItem {
    LangItem {
        lang: role.name().into(),
        module: "Prelude".into(),
        namespace: Namespace::Type,
        name: name.into(),
    }
}

fn opt_of(payload: IrType) -> IrType {
    IrType::Named {
        module: "Prelude".into(),
        name: "Option".into(),
        args: vec![TypeRef::new(payload)],
    }
}

#[test]
fn unconstrained_forall_errors_service_impl_and_opaque_alias_stay_erased() {
    let items = [
        lang_type(Lang::Option, "Option"),
        lang_type(Lang::Unknown, "Unknown"),
        lang_type(Lang::Int, "Int"),
        lang_type(Lang::Errors, "Errors"),
        lang_type(Lang::ServiceImpl, "ServiceImpl"),
    ];
    let lang = Known::of(&items);
    let alias_decl = TypeDecl {
        name: "UserId".into(),
        params: vec![],
        error: false,
        alias: Some(TypeRef::new(IrType::Named {
            module: "Prelude".into(),
            name: "Int".into(),
            args: vec![],
        })),
        constructors: vec![],
    };
    let mut decls = TypeDecls::new();
    decls.insert(("App".into(), "UserId".into()), &alias_decl);

    // Unconstrained `Forall` (e.g. `forall a. Unknown` or `forall a. Int`) has
    // no `*flickrt.Poly` wrapper and can agree with its instantiated body.
    for body in [
        IrType::Named {
            module: "Prelude".into(),
            name: "Unknown".into(),
            args: vec![],
        },
        IrType::Named {
            module: "Prelude".into(),
            name: "Int".into(),
            args: vec![],
        },
    ] {
        let unconstrained = IrType::Forall {
            vars: vec!["a".into()],
            constraints: vec![],
            body: TypeRef::new(body),
        };
        assert_eq!(
            OptRepr::of(&lang, &decls, Some(&opt_of(unconstrained))),
            None
        );
    }

    // Constrained `Forall` is wrapped in `*flickrt.Poly` at runtime (non-nil).
    let constrained = IrType::Forall {
        vars: vec!["a".into()],
        constraints: vec![Constraint {
            module: "Prelude".into(),
            class: "Show".into(),
            args: vec![TypeRef::new(IrType::Var("a".into()))],
        }],
        body: TypeRef::new(IrType::Var("a".into())),
    };
    assert_eq!(
        OptRepr::of(&lang, &decls, Some(&opt_of(constrained))),
        Some(OptRepr::RefSentinel)
    );

    // Opaque type aliases, `Errors`, and `ServiceImpl` unify with other types
    // and therefore stay erased (`None`).
    for erased in [
        IrType::Named {
            module: "App".into(),
            name: "UserId".into(),
            args: vec![],
        },
        IrType::Named {
            module: "Prelude".into(),
            name: "Errors".into(),
            args: vec![],
        },
        IrType::Named {
            module: "Prelude".into(),
            name: "ServiceImpl".into(),
            args: vec![],
        },
    ] {
        assert_eq!(OptRepr::of(&lang, &decls, Some(&opt_of(erased))), None);
    }
}
