use super::*;

use flick_core::diagnostics::Severity;
use std::path::Path;

use crate::backends::Selected;
use flick_backend::{BackendId, DeclaredInputs, Limits, validate_request};

#[test]
fn a_backend_receives_a_closed_graph_with_real_source_provenance() {
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run/codegen/gadt_rows.fl");
    let (diagnostics, compilation) = project::compile(&host, &mut sources, None, &[root]);
    assert!(
        !diagnostics.iter().any(|d| d.severity == Severity::Error),
        "{diagnostics:?}"
    );
    let output = BuildPlan {
        purpose: Purpose::Build,
        backend: Selected::Native,
        out_dir: "unused-provenance-output".into(),
        entries: vec![("main".into(), "Main".into())],
        typescript: false,
        go: false,
        suites: vec![],
        extras: build::Extras::default(),
        release: false,
    };
    let prepared = Prepared::new(
        &host,
        &mut sources,
        None,
        &compilation,
        &output,
        BackendPin {
            identity: BackendId {
                id: "test/provenance".into(),
                version: "1.0.0".into(),
            },
            sha256: flick_backend::sha256(b"test component"),
        },
    );
    let mut inputs = DeclaredInputs::new(&prepared.request.inputs, |id| {
        prepared.bytes.get(id).cloned()
    })
    .expect("all side inputs have their declared bytes");
    let program = validate_request(&prepared.request, &mut inputs, Limits::default())
        .expect("the combined IR, source table and module metadata agree");
    assert!(program.modules.len() > 100);
    let count = sources.files().count();
    let again = Prepared::new(
        &host,
        &mut sources,
        None,
        &compilation,
        &output,
        prepared.request.backend.clone(),
    );
    assert_eq!(
        sources.files().count(),
        count,
        "preparing again reuses source identities"
    );
    assert_eq!(again.request.program, prepared.request.program);
    assert_eq!(again.request.sources, prepared.request.sources);
    assert_eq!(again.bytes, prepared.bytes);
    for module in &program.modules {
        let role = prepared
            .request
            .modules
            .iter()
            .find(|m| m.name == module.name)
            .expect("metadata")
            .role;
        module.clone().each_span_mut(&mut |span| {
            let index = span.file.index() as usize;
            let source = &prepared.request.sources[index];
            let actual = sources.get(prepared.files[index]);
            assert_eq!(source.name, actual.path);
            assert_eq!(prepared.bytes[&source.input], actual.text.as_bytes());
            assert!(actual.text.get(span.range()).is_some());
            if role == ModuleRole::Builtin {
                assert_eq!(source.name, format!("<builtin>/{}.fl", module.name));
            } else {
                assert_eq!(role, ModuleRole::Own);
                assert!(!source.name.starts_with("<builtin>"));
            }
        });
    }
}
