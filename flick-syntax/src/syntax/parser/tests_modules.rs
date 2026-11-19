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
fn named_parameterized_module_header_and_application_are_distinct_nodes() {
    let header = sexp("module HashSet.Make(K : HashKey) exposing (Set, empty)\n");
    assert!(
        header.contains("(ModuleHeader module (ModulePath HashSet . Make)"),
        "{header}"
    );
    assert!(
        header.contains("(ModuleParam K : (NameType HashKey))"),
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
fn parameterized_module_application_rejects_a_dynamic_module_value_at_the_argument() {
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
fn higher_order_module_signatures_and_dependent_refinements() {
    let source = "type module Factory = (K : KeyPolicy) -> SetApi with type Element = K.Key\ntype module Adapter = Factory -> Factory\n";
    assert!(errors(source).is_empty(), "{:?}", errors(source));
    let tree = sexp(source);
    assert!(tree.contains("ModuleSignatureArrow"), "{tree}");
    assert!(tree.contains("ModuleSignatureRefinement"), "{tree}");
}

#[test]
fn higher_order_module_expressions_and_result_annotations() {
    let source = "module App.Make(F : Factory)(K : KeyPolicy) : SetApi\nmodule Chosen : SetApi = App.Make(HashSet.Make)(Keys)\nmodule Alias = Chosen\nmodule Adapt = module (F : Factory) -> F\n";
    assert!(errors(source).is_empty(), "{:?}", errors(source));
}

#[test]
fn module_contract_can_require_nested_modules_and_parameterized_modules() {
    let source = "type module Library =\n  module Make : Factory\n  module Keys : KeyPolicy\n";
    assert!(errors(source).is_empty(), "{:?}", errors(source));
}
