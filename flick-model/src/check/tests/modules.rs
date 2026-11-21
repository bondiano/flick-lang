//! Packed module (`Module Sig`) and existential (`exists K : Sig. T`) rules:
//! accepted packs/unpacks, nominal signature separation, abstract witness
//! isolation and escape rejection.

use std::collections::HashMap;
use std::rc::Rc;

use super::{T, a, decls, int, let_, mono, poly, record, rejected_by, ty, var};
use crate::check::Checker;
use crate::decls::{Decls, ModuleContract};
use crate::syntax::{Name, Path, Prim, Term};
use crate::types::{CONTRACT_SCOPE, EXISTS_PREFIX, Fields, Kind, Scheme, Stage, Type};

fn abstract_member(name: &str) -> Type {
    Type::Con(Path::new(CONTRACT_SCOPE, name))
}

fn exists_member(binder: &str, name: &str) -> Type {
    Type::Con(Path::new(&format!("{EXISTS_PREFIX}{binder}"), name))
}

fn contract(
    abstract_types: Vec<(&str, Kind)>,
    manifest_types: Vec<(&str, Vec<&str>, Type)>,
    fields: Vec<(&str, Scheme)>,
    packable: bool,
) -> ModuleContract {
    ModuleContract {
        abstract_types: abstract_types
            .into_iter()
            .map(|(n, k)| (Name::from(n), k))
            .collect(),
        manifest_types: manifest_types
            .into_iter()
            .map(|(n, ps, body)| {
                (
                    Name::from(n),
                    (ps.into_iter().map(Name::from).collect(), body),
                )
            })
            .collect(),
        nominals: HashMap::new(),
        fields: fields
            .into_iter()
            .map(|(n, s)| (Name::from(n), s))
            .collect(),
        packable,
    }
}

fn module_decls() -> Decls {
    let mut d = decls();
    // `type module SigA = answer : Int` and `type module SigB = answer : Int`
    for sig in ["SigA", "SigB"] {
        d.module_contracts.insert(
            Path::new(T, sig),
            contract(vec![], vec![], vec![("answer", mono(ty("Int")))], true),
        );
    }
    // `type module IdSig = identity : forall a. a -> a`
    d.module_contracts.insert(
        Path::new(T, "IdSig"),
        contract(
            vec![],
            vec![],
            vec![("identity", poly(Type::fun(a(), Stage::Pure, a())))],
            true,
        ),
    );
    // `type module Policy` and `OtherPolicy`: identical shape, distinct signatures
    for sig in ["Policy", "OtherPolicy"] {
        let key = abstract_member("Key");
        d.module_contracts.insert(
            Path::new(T, sig),
            contract(
                vec![("Key", Kind::Type)],
                vec![],
                vec![
                    ("key", mono(key.clone())),
                    (
                        "render",
                        mono(Type::fun(key.clone(), Stage::Pure, ty("String"))),
                    ),
                    (
                        "eq_key",
                        mono(Type::fun(
                            key.clone(),
                            Stage::Pure,
                            Type::fun(key, Stage::Pure, ty("Bool")),
                        )),
                    ),
                ],
                true,
            ),
        );
    }
    // `type module ManifestBox = type Box a = List a; type Token`
    d.module_contracts.insert(
        Path::new(T, "ManifestBox"),
        contract(
            vec![("Token", Kind::Type)],
            vec![("Box", vec!["a"], Type::app(ty("List"), a()))],
            vec![],
            true,
        ),
    );
    // Non-packable signature (abstract nominal or nested module)
    d.module_contracts.insert(
        Path::new(T, "StaticOnly"),
        contract(vec![], vec![], vec![("answer", mono(ty("Int")))], false),
    );
    d.values.insert(
        Path::new(T, "packed_a"),
        mono(Type::Module(Path::new(T, "SigA"))),
    );
    d.values.insert(
        Path::new(T, "packed_policy"),
        mono(Type::Module(Path::new(T, "Policy"))),
    );
    d.values.insert(
        Path::new(T, "exists_policy"),
        mono(Type::Exists(
            "K".into(),
            Path::new(T, "Policy"),
            Rc::new(exists_member("K", "Key")),
        )),
    );
    d.values.insert(
        Path::new(T, "poly_id"),
        poly(Type::fun(a(), Stage::Pure, a())),
    );
    d.values.insert(
        Path::new(T, "int_id"),
        mono(Type::fun(ty("Int"), Stage::Pure, ty("Int"))),
    );
    d
}

#[test]
fn packed_module_checks_fields_and_preserves_signature_identity() {
    let d = module_decls();
    let sig_a = Type::Module(Path::new(T, "SigA"));
    let sig_b = Type::Module(Path::new(T, "SigB"));

    // Packing matching record succeeds
    Checker::new(&d)
        .check_top(&record(&[("answer", int(42))]), &mono(sig_a.clone()))
        .expect("matching record packs as SigA");

    // Unpacking and projecting field succeeds
    let unpack_a = let_(
        "K",
        Term::Global(Path::new(T, "packed_a")),
        Term::Field(Rc::new(var("K")), "answer".into()),
    );
    assert_eq!(
        Checker::new(&d).infer_top(&unpack_a).expect("unpack").0,
        ty("Int")
    );

    // Same-shaped signature `SigB` rejects a `Module SigA` value
    rejected_by(
        Checker::new(&d).check_top(&Term::Global(Path::new(T, "packed_a")), &mono(sig_b)),
        "≡",
    );

    // A `Module SigA` value is not a plain `{ answer : Int }` record
    let plain_record = Type::Record(Fields {
        fields: [("answer".into(), ty("Int"))].into(),
        tail: None,
    });
    rejected_by(
        Checker::new(&d).check_top(&Term::Global(Path::new(T, "packed_a")), &mono(plain_record)),
        "≡",
    );

    // Missing field, extra field, mismatched field type and non-packable contract are rejected
    rejected_by(
        Checker::new(&d).check_top(&record(&[]), &mono(sig_a.clone())),
        "Rec",
    );
    rejected_by(
        Checker::new(&d).check_top(
            &record(&[("answer", int(1)), ("extra", int(2))]),
            &mono(sig_a.clone()),
        ),
        "Rec",
    );
    rejected_by(
        Checker::new(&d).check_top(&record(&[("answer", Term::string("no"))]), &mono(sig_a)),
        "≡",
    );
    rejected_by(
        Checker::new(&d).check_top(
            &record(&[("answer", int(1))]),
            &mono(Type::Module(Path::new(T, "StaticOnly"))),
        ),
        "Module",
    );
}

#[test]
fn packed_module_checks_polymorphic_fields() {
    let d = module_decls();
    let id_sig = Type::Module(Path::new(T, "IdSig"));
    Checker::new(&d)
        .check_top(
            &record(&[("identity", Term::Global(Path::new(T, "poly_id")))]),
            &mono(id_sig.clone()),
        )
        .expect("polymorphic identity satisfies IdSig");
    rejected_by(
        Checker::new(&d).check_top(
            &record(&[("identity", Term::Global(Path::new(T, "int_id")))]),
            &mono(id_sig),
        ),
        "≡",
    );
}

#[test]
fn packed_module_abstract_witness_checks_consistency_separation_and_no_escape() {
    let d = module_decls();
    let policy = Type::Module(Path::new(T, "Policy"));

    // Consistent witness (String) packs cleanly
    let good_pack = record(&[
        ("key", Term::string("x")),
        ("render", Term::lam("s", var("s"))),
        ("eq_key", Term::lam("x", Term::lam("y", Term::bool(true)))),
    ]);
    Checker::new(&d)
        .check_top(&good_pack, &mono(policy.clone()))
        .expect("consistent abstract witness packs");

    // Inconsistent witness (Int key vs String->String render) is rejected
    let bad_pack = record(&[
        ("key", int(1)),
        (
            "render",
            Term::lam(
                "s",
                Term::apps(Term::prim(Prim::Concat), [var("s"), Term::string("!")]),
            ),
        ),
        ("eq_key", Term::lam("x", Term::lam("y", Term::bool(true)))),
    ]);
    rejected_by(Checker::new(&d).check_top(&bad_pack, &mono(policy)), "≡");

    // Single opening shares its witness across fields
    let same_opening = let_(
        "K",
        Term::Global(Path::new(T, "packed_policy")),
        Term::app(
            Term::Field(Rc::new(var("K")), "render".into()),
            Term::Field(Rc::new(var("K")), "key".into()),
        ),
    );
    assert_eq!(
        Checker::new(&d)
            .infer_top(&same_opening)
            .expect("same opening")
            .0,
        ty("String")
    );

    // Two openings receive distinct witnesses and cannot be mixed
    let mixed_openings = let_(
        "A",
        Term::Global(Path::new(T, "packed_policy")),
        let_(
            "B",
            Term::Global(Path::new(T, "packed_policy")),
            Term::apps(
                Term::Field(Rc::new(var("A")), "eq_key".into()),
                [
                    Term::Field(Rc::new(var("A")), "key".into()),
                    Term::Field(Rc::new(var("B")), "key".into()),
                ],
            ),
        ),
    );
    rejected_by(Checker::new(&d).infer_top(&mixed_openings), "≡");

    // Witness cannot escape unpack directly
    let escaping = let_(
        "K",
        Term::Global(Path::new(T, "packed_policy")),
        Term::Field(Rc::new(var("K")), "key".into()),
    );
    rejected_by(Checker::new(&d).infer_top(&escaping), "Module");

    // Unprojected bindings preserve Module type in lists and conditionals
    let pass_through = let_(
        "copy",
        Term::Global(Path::new(T, "packed_policy")),
        Term::List(Rc::from([
            var("copy"),
            Term::Global(Path::new(T, "packed_policy")),
        ])),
    );
    assert_eq!(
        Checker::new(&d)
            .infer_top(&pass_through)
            .expect("list of packed modules")
            .0,
        Type::app(ty("List"), Type::Module(Path::new(T, "Policy")))
    );
}

#[test]
fn existential_packages_check_pack_open_alpha_and_signature_separation() {
    let d = module_decls();
    let exists_policy = Type::Exists(
        "K".into(),
        Path::new(T, "Policy"),
        Rc::new(exists_member("K", "Key")),
    );
    let exists_alpha = Type::Exists(
        "J".into(),
        Path::new(T, "Policy"),
        Rc::new(exists_member("J", "Key")),
    );
    let exists_other = Type::Exists(
        "K".into(),
        Path::new(T, "OtherPolicy"),
        Rc::new(exists_member("K", "Key")),
    );

    // Re-packing an opened module into an existential succeeds
    let pack_from_opened = let_(
        "K",
        Term::Global(Path::new(T, "packed_policy")),
        record(&[
            ("$module", var("K")),
            ("$value", Term::Field(Rc::new(var("K")), "key".into())),
        ]),
    );
    Checker::new(&d)
        .check_top(&pack_from_opened, &mono(exists_policy.clone()))
        .expect("repack opened module into existential");

    // Packing a mismatched payload value (Int instead of Key=String) is rejected
    let bad_payload = let_(
        "K",
        record(&[
            ("key", Term::string("x")),
            ("render", Term::lam("s", var("s"))),
            ("eq_key", Term::lam("x", Term::lam("y", Term::bool(true)))),
        ]),
        record(&[("$module", var("K")), ("$value", int(42))]),
    );
    rejected_by(
        Checker::new(&d).check_top(&bad_payload, &mono(exists_policy.clone())),
        "≡",
    );

    // Alpha-equivalent existential matches, different contract does not
    Checker::new(&d)
        .check_top(
            &Term::Global(Path::new(T, "exists_policy")),
            &mono(exists_alpha),
        )
        .expect("alpha-equivalent existential");
    rejected_by(
        Checker::new(&d).check_top(
            &Term::Global(Path::new(T, "exists_policy")),
            &mono(exists_other),
        ),
        "≡",
    );
}

#[test]
fn existential_packages_separate_witnesses_and_expand_manifest_aliases() {
    let d = module_decls();

    // Opening an existential lets K.render operate on its payload item
    let open_and_render = let_(
        "$e",
        Term::Global(Path::new(T, "exists_policy")),
        let_(
            "K",
            Term::Field(Rc::new(var("$e")), "$module".into()),
            let_(
                "item",
                Term::Field(Rc::new(var("$e")), "$value".into()),
                Term::app(Term::Field(Rc::new(var("K")), "render".into()), var("item")),
            ),
        ),
    );
    assert_eq!(
        Checker::new(&d)
            .infer_top(&open_and_render)
            .expect("open and render")
            .0,
        ty("String")
    );

    // Two opened existentials cannot mix their witnesses
    let mix_existentials = let_(
        "$e1",
        Term::Global(Path::new(T, "exists_policy")),
        let_(
            "K1",
            Term::Field(Rc::new(var("$e1")), "$module".into()),
            let_(
                "$e2",
                Term::Global(Path::new(T, "exists_policy")),
                let_(
                    "item2",
                    Term::Field(Rc::new(var("$e2")), "$value".into()),
                    Term::app(
                        Term::Field(Rc::new(var("K1")), "render".into()),
                        var("item2"),
                    ),
                ),
            ),
        ),
    );
    rejected_by(Checker::new(&d).infer_top(&mix_existentials), "≡");

    // Manifest alias in existential unifies with outer meta; abstract token does not
    let mut c = Checker::new(&d);
    let m = c.fresh();
    let manifest_ex = Type::Exists(
        "K".into(),
        Path::new(T, "ManifestBox"),
        Rc::new(Type::app(exists_member("K", "Box"), ty("Int"))),
    );
    let target_ex = Type::Exists("J".into(), Path::new(T, "ManifestBox"), Rc::new(m.clone()));
    c.unify(&manifest_ex, &target_ex)
        .expect("manifest Box Int solves outer meta to List Int");
    assert_eq!(c.zonk(&m), Type::app(ty("List"), ty("Int")));

    let mut c = Checker::new(&d);
    let m = c.fresh();
    let abstract_ex = Type::Exists(
        "K".into(),
        Path::new(T, "ManifestBox"),
        Rc::new(exists_member("K", "Token")),
    );
    let target_ex = Type::Exists("J".into(), Path::new(T, "ManifestBox"), Rc::new(m));
    rejected_by(c.unify(&abstract_ex, &target_ex), "Module");
}
