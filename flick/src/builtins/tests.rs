use super::*;

#[test]
fn the_builtin_ir_cache_keeps_source_text_and_identities() {
    let bundle = build::builtin_bundle_of(false);
    let cached = CachedModules::new(bundle);
    let mut bytes = Vec::new();
    ciborium::into_writer(&cached, &mut bytes).expect("cache writes");
    let read: CachedModules = ciborium::from_reader(&bytes[..]).expect("cache reads");
    let restored = read.checked().expect("all spans retain their source text");
    assert_eq!(restored.modules, bundle.modules);
    assert_eq!(restored.sources.len(), bundle.sources.len());
    for (a, b) in restored.sources.iter().zip(&bundle.sources) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.text, b.text);
    }
}

#[test]
fn a_legacy_ir_only_builtin_cache_must_be_regenerated() {
    let cached = CachedModules::new(build::builtin_bundle_of(false));
    let old = cached.program.to_cbor();
    assert!(ciborium::from_reader::<CachedModules, _>(&old[..]).is_err());
}

#[test]
fn a_builtin_cache_cannot_drop_its_lowering_source() {
    let mut cached = CachedModules::new(build::builtin_bundle_of(false));
    cached.sources.clear();
    assert!(cached.checked().is_none());

    let mut cached = CachedModules::new(build::builtin_bundle_of(false));
    for text in &mut cached.sources {
        text.clear();
    }
    assert!(
        cached.checked().is_none(),
        "out-of-bounds spans invalidate cached lowering"
    );
}
