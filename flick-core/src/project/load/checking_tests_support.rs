#[test]
fn changing_only_private_type_support_invalidates_consumer_checking() {
    let base = |ty: &str| {
        format!(
            "module A.Base exposing (Packed)\ntype module S =\n  type Box a\n  tag : {ty}\ntype alias Packed = exists K : S. K.Box Int\n"
        )
    };
    let mut host = Memory(BTreeMap::from([
        ("/p/src/A/Base.fl".into(), base("Int")),
        (
            "/p/src/A/Use.fl".into(),
            "module A.Use exposing (Again)\nimport A.Base\ntype alias Again = A.Base.Packed\n"
                .into(),
        ),
    ]));
    let mut cache = ModuleCache::default();
    let (first, diagnostics) = analyzed(&host, Some(&mut cache));
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let (same, diagnostics) = analyzed(&host, Some(&mut cache));
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(Arc::ptr_eq(&first["A.Use"], &same["A.Use"]));
    host.0.insert("/p/src/A/Base.fl".into(), base("String"));
    let (changed, diagnostics) = analyzed(&host, Some(&mut cache));
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(!Arc::ptr_eq(&first["A.Use"], &changed["A.Use"]));
    let (_, cold_diagnostics) = analyzed(&host, None);
    assert_eq!(diagnostics, cold_diagnostics);
}
