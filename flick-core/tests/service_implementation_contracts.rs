//! A local service's implementation type is available while declarations
//! refer to one another, including inside a private persisted contract.

use std::io;
use std::path::Path;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::iface;
use flick_core::project::{self, Host};
use flick_core::source::SourceMap;

struct Source<'a>(&'a str);

impl Host for Source<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if path == Path::new("/Probe.fl") {
            Ok(self.0.as_bytes().to_vec())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

fn compile(text: &str) -> (Vec<Diagnostic>, project::Compilation) {
    project::compile(
        &Source(text),
        &mut SourceMap::default(),
        None,
        &["/Probe.fl".into()],
    )
}

#[test]
fn private_service_implementation_contract_round_trips_without_an_error_marker() {
    for declarations in [
        "service Clock =\n  tick : Unit ->{} Int\ntype module S =\n  handler : Clock.Impl\n",
        "type module S =\n  handler : Clock.Impl\nservice Clock =\n  tick : Unit ->{} Int\n",
        "type alias Handler = Clock.Impl\ntype module S =\n  handler : Handler\nservice Clock =\n  tick : Unit ->{} Int\n",
        "type module S =\n  handler : Store.Impl Int\nservice Store a =\n  read : Unit ->{} a\n",
        "type module S =\n  handler : Clock.Impl\nservice Clock =\n  tick : Unit ->{} Answer\ntype alias Answer = Int\n",
        "opaque type alias Handler = Option Clock.Impl\ntype module S =\n  handler : Clock.Impl\nservice Clock =\n  tick : Unit ->{} Handler\n",
        "type Node = Node Clock.Impl\ntype module S =\n  handler : Clock.Impl\nservice Clock =\n  tick : Unit ->{} Node\n",
    ] {
        let text = format!(
            "module Probe exposing (Packed)\n{declarations}type alias Packed = exists K : S. Unit\n"
        );
        let (diagnostics, compilation) = compile(&text);
        assert!(
            diagnostics.iter().all(|d| d.severity != Severity::Error),
            "valid source: {diagnostics:?}"
        );
        let interface = &compilation
            .modules
            .iter()
            .find(|m| m.name == "Probe")
            .expect("checked provider")
            .interface;
        // Error markers cannot be serialized. A successful check must not leave
        // one hidden inside the public alias's private dependency closure.
        let encoded = iface::write(interface);
        assert_eq!(
            iface::read(encoded.as_bytes(), "Probe"),
            Ok(interface.clone())
        );
    }
}

#[test]
fn the_completed_implementation_record_is_enforced_not_an_error_wildcard() {
    for (result, expected) in [("42", None), ("\"wrong\"", Some(codes::T061))] {
        let text = format!(
            "module Probe exposing (packed)\nservice Clock =\n  tick : Unit ->{{}} Int\ntype module S =\n  handler : Clock.Impl\nmodule Good exposing (handler) =\n  handler : {{ tick : Unit ->{{}} {} }}\n  let handler = {{ tick = fun () -> {result} }}\npacked : Module S\nlet packed = pack Good as S\n",
            if expected.is_none() { "Int" } else { "String" }
        );
        let (diagnostics, _) = compile(&text);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if let Some(code) = expected {
            assert!(errors.iter().any(|d| d.code == code), "{errors:?}");
        } else {
            assert!(errors.is_empty(), "{errors:?}");
        }
    }
}

#[test]
fn implementation_headers_preserve_service_parameter_kinds_and_alias_saturation() {
    for (application, code) in [("Store.Impl", codes::T002), ("Store.Impl Int", codes::T002)] {
        let text = format!(
            "module Probe exposing (S)\nservice Store (f : Type -> Type) =\n  read : Unit ->{{}} f Int\ntype module S =\n  type Value = {application}\n"
        );
        let (diagnostics, _) = compile(&text);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.iter().any(|d| d.code == code),
            "{application}: {errors:?}"
        );
    }
    let (diagnostics, compilation) = compile(
        "module Probe exposing (S)\nservice Store (f : Type -> Type) =\n  read : Unit ->{} f Int\ntype module S =\n  type Value = Store.Impl List\n",
    );
    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "{diagnostics:?}"
    );
    for module in compilation.modules {
        let encoded = iface::write(&module.interface);
        assert_eq!(
            iface::read(encoded.as_bytes(), &module.name),
            Ok(module.interface)
        );
    }
    // The argument's kind is correct here: only the real alias saturation
    // rule, not a Type-vs-arrow kind mismatch, can reject Store.Impl.
    let (diagnostics, _) = compile(
        "module Probe exposing (..)\ntype Wrap (f : Type -> Type) = Wrap (f Int)\nservice Store a =\n  read : Unit ->{} a\ntype alias Bad = Wrap Store.Impl\n",
    );
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, codes::T002);
    assert!(errors[0].message.contains("needs 1 type argument"));
    let (diagnostics, _) = compile(
        "module Probe exposing (..)\ntype Wrap (f : Type -> Type) = Wrap (f Int)\ntype Ordinary a\ntype alias Good = Wrap Ordinary\n",
    );
    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "{diagnostics:?}"
    );
}

#[test]
fn generated_transparent_alias_cycles_are_diagnosed_at_the_service_name() {
    for body in ["Clock.Impl", "Handler"] {
        let text = format!(
            "module Probe exposing (..)\ntype alias Handler = Clock.Impl\nservice Clock =\n  tick : Unit ->{{}} {body}\n"
        );
        let (diagnostics, _) = compile(&text);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, codes::R007);
        let span = errors[0].span.expect("service name");
        let start = text.find("service Clock").expect("service declaration") + "service ".len();
        assert_eq!(
            (span.start as usize, span.end as usize),
            (start, start + "Clock".len())
        );
        assert!(errors[0].message.contains("Clock.Impl"));
    }
}
