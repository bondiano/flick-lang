// A test companion `A/Base.test.fl` is checked as `A.Base$test`: it sees all
// that `A.Base` declares, other modules see only its exports, and the code
// of `A.Base` exports the rest only to a run that loads the companion.

use crate::diagnostics::Severity as Level;

/// `A.Base`, which keeps `secret` and its type `Hidden` to itself, a
/// companion of it that uses both, and a module of the test root.
fn companion_host(secret: &str) -> Memory {
    let base = format!(
        "module A.Base exposing (x)\ntype Hidden = Hidden Float\nsecret : Hidden -> Float\n{secret}\nx : Float\nlet x = secret (Hidden 1)\n"
    );
    Memory(BTreeMap::from([
        (PathBuf::from("/p/src/A/Base.fl"), base),
        (
            PathBuf::from("/p/src/A/Base.test.fl"),
            "module A.Base\ntests : List Test.Test\nlet tests = [Test.test \"secret\" (fun () -> Test.equal 1 (secret (Hidden 1)))]\n".to_owned(),
        ),
        (
            PathBuf::from("/p/test/A/BaseTest.fl"),
            "module A.BaseTest exposing (tests)\nimport A.Base exposing (x)\ntests : List Test.Test\nlet tests = [Test.test \"x\" (fun () -> Test.equal 1 x)]\n".to_owned(),
        ),
    ]))
}

const SECRET: &str = "let secret h = match h with\n  | Hidden n -> n";

fn companion_roots(companion: bool) -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/p/src/A/Base.fl"),
        PathBuf::from("/p/test/A/BaseTest.fl"),
    ];
    roots.extend(companion.then(|| PathBuf::from("/p/src/A/Base.test.fl")));
    roots
}

#[test]
fn a_companion_sees_what_its_module_keeps_and_its_module_exports_that_only_to_it() {
    let host = companion_host(SECRET);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let roots = companion_roots(true);
    let (diagnostics, compilation) = super::compile(&host, &mut sources, Some(&project), &roots);
    let errors: Vec<_> = (diagnostics.iter())
        .filter(|d| d.severity == Level::Error)
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    let named = |name: &str| compilation.modules.iter().find(|m| m.name == name);
    let companion = named("A.Base$test").expect("the companion is a module of its own");
    assert!(companion.test, "its cases are tests");
    assert!(companion.interface.exports_value("tests"));
    let exported = |c: &super::Compiled| -> Vec<String> {
        (c.ir.exports.values.iter()).map(|e| e.name.clone()).collect()
    };
    let base = named("A.Base").expect("A.Base");
    assert!(exported(base).contains(&"secret".to_owned()), "{:?}", exported(base));
    // its interface stays the public one, which importers and `.iface` read
    assert!(base.interface.find(Namespace::Value, "secret").is_none());

    // a build that does not load the companion exports what `exposing` says
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let roots = companion_roots(false);
    let (_, compilation) = super::compile(&host, &mut sources, Some(&project), &roots);
    let base = (compilation.modules.iter()).find(|m| m.name == "A.Base");
    assert_eq!(base.map(exported), Some(vec!["x".to_owned()]));
}

#[test]
fn a_module_of_the_test_root_sees_only_the_exports() {
    let mut host = companion_host(SECRET);
    host.0.insert(
        PathBuf::from("/p/test/A/BaseTest.fl"),
        "module A.BaseTest exposing (tests)\nimport A.Base\ntests : List Test.Test\nlet tests = [Test.test \"secret\" (fun () -> Test.equal 1 (A.Base.secret (A.Base.Hidden 1)))]\n".to_owned(),
    );
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let roots = companion_roots(true);
    let diagnostics = super::check(&host, &mut sources, Some(&project), &roots);
    let errors: Vec<String> = (diagnostics.iter())
        .filter(|d| d.severity == Level::Error)
        .map(|d| d.message.clone())
        .collect();
    assert!(
        errors.iter().any(|e| e.contains("secret")),
        "the test root cannot name what `A.Base` keeps: {errors:?}"
    );
}

#[test]
fn a_changed_private_declaration_rechecks_the_companion_and_no_other_importer() {
    let analyzed = |host: &Memory, cache: &mut ModuleCache| {
        let mut sources = SourceMap::default();
        let project = library(&mut sources);
        let roots = companion_roots(true);
        let analysis = analyze_with(host, &mut sources, Some(&project), &roots, cache);
        let typings: BTreeMap<String, Arc<Typing>> = (analysis.modules.iter())
            .map(|m| (m.name.clone(), Arc::clone(&m.typing)))
            .collect();
        let errors: Vec<String> = (analysis.diagnostics.iter())
            .filter(|d| d.severity == Level::Error)
            .map(|d| d.message.clone())
            .collect();
        (typings, errors)
    };
    let mut cache = ModuleCache::default();
    let (first, errors) = analyzed(&companion_host(SECRET), &mut cache);
    assert!(errors.is_empty(), "{errors:?}");
    // `secret` now gives a text: its companion compares it with a number
    let changed = "let secret h = match h with\n  | Hidden n -> show n";
    let changed = companion_host(changed).0;
    let edited = Memory(
        (changed.into_iter())
            .map(|(p, text)| (p, text.replace("Hidden -> Float", "Hidden -> String")))
            .map(|(p, text)| (p, text.replace("let x = secret (Hidden 1)", "let x = 1")))
            .collect(),
    );
    let (warm, errors) = analyzed(&edited, &mut cache);
    assert!(!Arc::ptr_eq(&first["A.Base$test"], &warm["A.Base$test"]));
    assert!(Arc::ptr_eq(&first["A.BaseTest"], &warm["A.BaseTest"]));
    assert!(!errors.is_empty(), "`Test.equal 1 (secret …)` no longer checks");
}

/// The edit of the test above, in a process of its own: the companion's
/// saved output is keyed by what it reads of its module, the private parts
/// included, not by the exports alone.
#[test]
fn a_saved_companion_is_rechecked_when_a_private_declaration_changes() {
    let saved: Saved = Arc::default();
    let errors = |host: &Memory| {
        let taken = Arc::default();
        let mut cache = ModuleCache::stored(memory_store("one", &saved, &taken));
        let mut sources = SourceMap::default();
        let project = library(&mut sources);
        let roots = companion_roots(true);
        let output = run(
            host,
            &mut sources,
            Some(&project),
            &roots,
            Mode::Check,
            Some(&mut cache),
            None,
        );
        (output.diagnostics.iter())
            .filter(|d| d.severity == Level::Error)
            .count()
    };
    assert_eq!(errors(&companion_host(SECRET)), 0);
    let changed = "let secret h = match h with\n  | Hidden n -> show n";
    let edited = Memory(
        (companion_host(changed).0.into_iter())
            .map(|(p, text)| (p, text.replace("Hidden -> Float", "Hidden -> String")))
            .map(|(p, text)| (p, text.replace("let x = secret (Hidden 1)", "let x = 1")))
            .collect(),
    );
    assert!(errors(&edited) > 0, "the companion compares a text with a number");
}
