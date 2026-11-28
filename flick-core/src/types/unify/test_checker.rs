use crate::types::ForeignBackendSpec;
use crate::resolve::{self, Module};
use crate::source::SourceMap;
use crate::syntax::{self, cst::Node};
use crate::types::{self, Checker, Limits, Options, StaticContext, Thresholds, Watched};

pub(in crate::types) fn with_checker<T>(check: impl FnOnce(&mut Checker<'_>) -> T) -> T {
    with_source("module Probe exposing ()\n", |checker, _| check(checker))
}

pub(in crate::types) fn with_source<T>(
    text: &str,
    check: impl for<'a> FnOnce(&mut Checker<'a>, &'a Node) -> T,
) -> T {
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let module = Module {
        name: "Probe",
        file,
        text,
        tree: &parsed.tree,
        syntax: &syntax,
        generated: resolve::NO_GENERATED,
    };
    let options = Options {
        local: &|name| name == "Probe",
        limits: Limits::default(),
        no_let_assert: false,
        thresholds: Thresholds::default(),
        watched: Watched::none(),
        stages_by_class: false,
        foreign_backend: ForeignBackendSpec::default_js(),
        validate_foreign_metadata: true,
        native_overrides: None,
        plugin_attributes: &[],
    };
    let mut checker = types::checker_in(
        &module,
        &|_| None,
        types::builtin_env(false),
        options,
        StaticContext::default(),
    );
    check(&mut checker, &parsed.tree)
}
