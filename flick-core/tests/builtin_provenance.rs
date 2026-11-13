//! A closed backend graph must not alias the separate source maps used
//! while checking built-in modules.

use std::collections::BTreeSet;

use flick_core::{build, types};

#[test]
fn builtin_modules_have_distinct_source_identities() {
    for stages_by_class in [false, true] {
        let mut seen = BTreeSet::new();
        let modules = build::builtin_modules_of(stages_by_class);
        assert!(modules.len() > 100);
        for module in modules {
            let mut files = BTreeSet::new();
            module.clone().each_span_mut(&mut |s| {
                files.insert(s.file);
            });
            for file in files {
                assert!(
                    seen.insert(file),
                    "{} aliases another module's source",
                    module.name
                );
            }
        }
    }
}

#[test]
fn lowered_spans_use_the_actual_expanded_source_text() {
    for stages_by_class in [false, true] {
        let bundle = build::builtin_bundle_of(stages_by_class);
        let mut generated = 0;
        for module in &bundle.modules {
            let source = bundle
                .sources
                .iter()
                .find(|s| s.name == format!("<builtin>/{}.fl", module.name))
                .expect("each built-in keeps its lowering source");
            let original = types::builtin_sources()
                .iter()
                .find(|(name, _)| *name == module.name)
                .expect("declared module")
                .1;
            if source.text != original {
                generated += 1;
                assert!(
                    source.text.len() > original.len(),
                    "generated declarations are retained"
                );
            }
            module.clone().each_span_mut(&mut |span| {
                let actual = &bundle.sources[span.file.index() as usize];
                assert_eq!(actual.name, source.name);
                assert!(span.start <= span.end);
                assert!(
                    actual.text.get(span.range()).is_some(),
                    "{}: {span:?}",
                    module.name
                );
            });
        }
        assert!(
            generated > 0,
            "the test covers generated built-in declarations"
        );
    }
}
