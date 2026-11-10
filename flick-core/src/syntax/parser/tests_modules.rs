#[test]
fn module_contract_has_abstract_and_manifest_types_and_dictionary_requirements() {
    let src = "type module HashKey =\n  type Key\n  equality : Eq.Dict Key\n  instance Eq Key from equality\n  type Value = String\n";
    let tree = sexp(src);
    assert!(
        tree.contains("(ModuleTypeDecl type module HashKey = (Members"),
        "{tree}"
    );
    assert!(tree.contains("(ModuleTypeMember type Key)"), "{tree}");
    assert!(tree.contains("(Signature equality :"), "{tree}");
    assert!(tree.contains("(ModuleInstanceReq instance"), "{tree}");
    assert!(
        tree.contains("(ModuleTypeMember type Value = (NameType String))"),
        "{tree}"
    );
}

#[test]
fn module_contract_rejects_implementations_in_its_body() {
    let src = "type module HashKey =\n  let equality = 1\n  type Key\n";
    assert!(!errors(src).is_empty());
    assert!(sexp("type module HashKey =\n  type Key\n").contains("(ModuleTypeMember type Key)"));
}

#[test]
fn named_functor_header_and_application_are_distinct_nodes() {
    let header = sexp("module HashSet.Make(K : HashKey) exposing (Set, empty)\n");
    assert!(
        header.contains("(ModuleHeader module (ModulePath HashSet . Make)"),
        "{header}"
    );
    assert!(
        header.contains("(FunctorParam K : (NameType HashKey))"),
        "{header}"
    );
    let application = sexp("module Names = HashSet.Make(CaseInsensitive)\n");
    assert!(
        application.contains("(ModuleApplyDecl module Names ="),
        "{application}"
    );
    assert!(
        application.contains("(ModulePath HashSet . Make)"),
        "{application}"
    );
    assert!(
        application.contains("(ModulePath CaseInsensitive)"),
        "{application}"
    );
}

#[test]
fn inline_module_has_a_scoped_declaration_body() {
    let source = "module App.Article exposing (page)\nlet key = 1\nmodule Comments exposing (thread) =\n  thread : Int\n  let thread = key\npage : Int\nlet page = Comments.thread\n";
    assert!(errors(source).is_empty(), "{:?}", errors(source));
    let tree = sexp(source);
    assert!(
        tree.contains("(InlineModuleDecl module (ModulePath Comments)"),
        "{tree}"
    );
    assert!(tree.contains("(Members (Signature thread"), "{tree}");
    assert!(tree.contains("(LetDecl let (VarPat thread)"), "{tree}");
}

#[test]
fn module_syntax_records_static_parameters_and_named_applications() {
    let functor = "module HashSet.Make(K : HashKey) exposing (Set)\n";
    let mut map = SourceMap::default();
    let file = map.add("HashSet/Make.fl", functor);
    let parsed = parse_ok(functor);
    let syntax = resolve::module_syntax(&parsed.tree, functor, file);
    let params = &syntax.header.expect("header").parameters;
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "K");
    assert_eq!(params[0].contract, "HashKey");

    let parent =
        "module App.Collections exposing (Names)\nmodule Names = HashSet.Make(CaseInsensitive)\n";
    let file = map.add("App/Collections.fl", parent);
    let parsed = parse_ok(parent);
    let syntax = resolve::module_syntax(&parsed.tree, parent, file);
    assert_eq!(syntax.applications.len(), 1);
    assert_eq!(syntax.applications[0].name, "Names");
    assert_eq!(syntax.applications[0].functor, "HashSet.Make");
    assert_eq!(syntax.applications[0].arguments, ["CaseInsensitive"]);
}

#[test]
fn functor_application_rejects_a_dynamic_module_value_at_the_argument() {
    let source = "module App.Parent exposing (Chosen)\nmodule Chosen = App.Make(packed)\n";
    let parsed = parse_str(source);
    assert!(
        parsed.diagnostics.iter().any(|d| d
            .message
            .contains("a named module, not a `Module Sig` value")),
        "{:?}",
        parsed.diagnostics
    );
}

#[test]
fn module_values_and_existential_results_have_dedicated_syntax() {
    let src = "packed : Module HashKey\nlet packed = pack CaseInsensitive as HashKey\nresult : exists K : HashKey. K.Key\nlet result = unpack packed as K in K.value\n";
    let tree = sexp(src);
    assert!(
        tree.contains("(PackExpr pack (ModulePath CaseInsensitive) as (NameType HashKey))"),
        "{tree}"
    );
    assert!(
        tree.contains("(ExistsType exists K : (NameType HashKey) . (NameType K . Key))"),
        "{tree}"
    );
    assert!(
        tree.contains("(UnpackExpr unpack (NameExpr packed) as K in"),
        "{tree}"
    );
}

#[test]
fn module_contract_can_require_nominal_class_and_service_identities() {
    let src = "type module Runtime =\n  class Eq = Core.Eq\n  service Clock = Time.Clock\n";
    let tree = sexp(src);
    assert!(
        tree.contains("(ModuleNominalMember class Eq = (ModulePath Core . Eq))"),
        "{tree}"
    );
    assert!(
        tree.contains("(ModuleNominalMember service Clock = (ModulePath Time . Clock))"),
        "{tree}"
    );
}

#[test]
fn module_contract_name_is_an_exported_type_level_definition() {
    let src = "module App.Contracts exposing (HashKey)\ntype module HashKey =\n  type Key\n";
    let mut map = SourceMap::default();
    let file = map.add("App/Contracts.fl", src);
    let parsed = parse_ok(src);
    let syntax = resolve::module_syntax(&parsed.tree, src, file);
    let module = resolve::Module {
        name: "App.Contracts",
        file,
        text: src,
        tree: &parsed.tree,
        syntax: &syntax,
        generated: resolve::NO_GENERATED,
    };
    let (interface, diagnostics) = resolve::resolve(&module, &|_| None);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(interface.exports.iter().any(|e| e.entity.name == "HashKey"));
}

#[test]
fn checked_module_contract_is_preserved_in_its_interface() {
    let src = "module App.Contracts exposing (HashKey)\ntype module HashKey =\n  type Key\n  type Value = String\n  equal : Key -> Key -> Bool\n";
    let mut map = SourceMap::default();
    let file = map.add("App/Contracts.fl", src);
    let parsed = parse_ok(src);
    let syntax = resolve::module_syntax(&parsed.tree, src, file);
    let module = resolve::Module {
        name: "App.Contracts",
        file,
        text: src,
        tree: &parsed.tree,
        syntax: &syntax,
        generated: resolve::NO_GENERATED,
    };
    let (mut interface, resolved) = resolve::resolve(&module, &|_| None);
    assert!(resolved.is_empty(), "{resolved:?}");
    let local = |_: &str| true;
    let watched = Watched::none();
    let options = Options {
        local: &local,
        limits: Limits::default(),
        no_let_assert: false,
        thresholds: Thresholds::default(),
        effects: true,
        watched,
        stages_by_class: false,
    };
    let diagnostics = types::check(
        &module,
        &|_| None,
        types::builtins(),
        options,
        &mut interface,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let contract = interface
        .exports
        .iter()
        .find(|e| e.entity.name == "HashKey")
        .and_then(|e| e.definition.as_ref())
        .expect("exported module contract definition");
    assert_eq!(contract.kind.to_string(), "Module");
    match &contract.form {
        types::Form::ModuleContract { requirements } => {
            assert_eq!(requirements.len(), 3);
            assert!(matches!(
                &requirements[0],
                types::ModuleRequirement::Type { name, manifest: None, .. } if name == "Key"
            ));
            assert!(matches!(
                &requirements[1],
                types::ModuleRequirement::Type { name, manifest: Some(_), .. } if name == "Value"
            ));
            assert!(matches!(
                &requirements[2],
                types::ModuleRequirement::Value { name, ty } if name == "equal" && types::show_type(ty).contains("Key")
            ));
        }
        other => panic!("expected module contract, got {other:?}"),
    }
    let written = iface::write(&interface);
    let read = iface::read(written.as_bytes(), "App.Contracts").expect("interface round trip");
    assert_eq!(read, interface);
}

#[test]
fn duplicate_module_contract_members_are_reported() {
    let src = "module App.Contracts exposing (S)\ntype module S =\n  type Key\n  type Key\n";
    let mut map = SourceMap::default();
    let file = map.add("App/Contracts.fl", src);
    let diagnostics = crate::check(&map, file);
    assert!(
        diagnostics.iter().any(|d| d.code == codes::R008),
        "{diagnostics:?}"
    );
}

#[test]
fn module_value_type_accepts_only_module_contracts() {
    let good = "module App.Contracts exposing (S, Packed)\ntype module S =\n  type Key\ntype alias Packed = Module S\n";
    let bad = "module App.Contracts exposing (Wrong)\ntype alias Wrong = Module String\n";
    for (src, want_error) in [(good, false), (bad, true)] {
        let mut map = SourceMap::default();
        let file = map.add("App/Contracts.fl", src);
        let diagnostics = crate::check(&map, file);
        let has_kind_error = diagnostics.iter().any(|d| d.code == codes::T002);
        assert_eq!(has_kind_error, want_error, "{diagnostics:?}");
        if !want_error {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        }
    }
}

#[test]
fn first_class_module_type_requires_manifest_nominal_members() {
    let src = "module App.Contracts exposing (S, Packed)\ntype module S =\n  class Equality a =\n    eq : a -> a -> Bool\ntype alias Packed = Module S\n";
    let mut map = SourceMap::default();
    let file = map.add("App/Contracts.fl", src);
    let diagnostics = crate::check(&map, file);
    assert!(
        diagnostics.iter().any(|d| d.code == codes::T063),
        "{diagnostics:?}"
    );
}

#[test]
fn manifest_class_name_can_be_used_by_later_dictionary_requirement() {
    let src = "module App.Contracts exposing (S)\ntype module S =\n  type Key\n  class Equality = Prelude.Eq\n  equality : Equality.Dict Key\n  instance Equality Key from equality\n";
    let mut map = SourceMap::default();
    let file = map.add("App/Contracts.fl", src);
    let diagnostics = crate::check(&map, file);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let parsed = parse_ok(src);
    let syntax = resolve::module_syntax(&parsed.tree, src, file);
    let module = resolve::Module {
        name: "App.Contracts",
        file,
        text: src,
        tree: &parsed.tree,
        syntax: &syntax,
        generated: resolve::NO_GENERATED,
    };
    let (mut interface, resolved) = resolve::resolve(&module, &|_| None);
    assert!(resolved.is_empty(), "{resolved:?}");
    let local = |_: &str| true;
    let watched = types::Watched::none();
    let options = types::Options {
        local: &local,
        limits: types::Limits::default(),
        no_let_assert: false,
        thresholds: types::Thresholds::default(),
        effects: true,
        watched,
        stages_by_class: false,
    };
    let checked = types::check(
        &module,
        &|_| None,
        types::builtins(),
        options,
        &mut interface,
    );
    assert!(checked.is_empty(), "{checked:?}");
    let requirements = match &interface.exports[0]
        .definition
        .as_ref()
        .expect("contract definition")
        .form
    {
        types::Form::ModuleContract { requirements } => requirements,
        other => panic!("expected module contract, got {other:?}"),
    };
    assert!(requirements.iter().any(|r| matches!(r, types::ModuleRequirement::Dictionary { head, from } if from == "equality" && matches!(head.spine().0, types::Type::Con { module, name } if module == "Prelude" && name == "Eq"))), "{requirements:?}");
}
#[test]
fn higher_order_module_signatures_and_dependent_refinements() {
    let source = "type module Factory = (K : KeyPolicy) -> SetApi with type Element = K.Key\ntype module Adapter = Factory -> Factory\n";
    assert!(errors(source).is_empty(), "{:?}", errors(source));
    let tree = sexp(source);
    assert!(tree.contains("ModuleSignatureArrow"), "{tree}");
    assert!(tree.contains("ModuleSignatureRefinement"), "{tree}");
}

#[test]
fn checked_higher_order_signature_keeps_the_input_type_dependency() {
    let source = "module App.Contracts exposing (Factory, KeyPolicy, SetApi)\ntype module Factory = (K : KeyPolicy) -> SetApi with type Element = K.Key\ntype module SetApi =\n  type Element\n  type Set\n  insert : Element -> Set -> Set\ntype module KeyPolicy =\n  type Key\n";
    let mut sources = SourceMap::default();
    let file = sources.add("App/Contracts.fl", source);
    let diagnostics = crate::check(&sources, file);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn higher_order_module_expressions_and_result_annotations() {
    let source = "module App.Make(F : Factory)(K : KeyPolicy) : SetApi\nmodule Chosen : SetApi = App.Make(HashSet.Make)(Keys)\nmodule Alias = Chosen\nmodule Adapt = functor (F : Factory) -> F\n";
    assert!(errors(source).is_empty(), "{:?}", errors(source));
}

#[test]
fn module_contract_can_require_nested_modules_and_functors() {
    let source = "type module Library =\n  module Make : Factory\n  module Keys : KeyPolicy\n";
    assert!(errors(source).is_empty(), "{:?}", errors(source));
}
