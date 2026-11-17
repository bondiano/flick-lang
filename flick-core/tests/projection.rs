//! The executable build and the editor projection come from one analysis:
//! for every e2e program, the modules the build compiles and those the
//! projection is made from have one fingerprint, which the projection
//! states in its first line.

#![cfg(feature = "ide")]

use std::fs;
use std::io;
use std::path::Path;

use flick_core::diagnostics::Severity;
use flick_core::project::{self, Host, analysis_fingerprint};
use flick_core::projection;
use flick_core::source::SourceMap;
use flick_core::types::builtins;
use flick_test_support::files as fixture_files;

struct Files;

impl Host for Files {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

#[test]
fn projection_and_executable_share_the_analysis() {
    let mut paths = Vec::new();
    paths.extend(fixture_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run"),
        "fl",
    ));
    assert!(paths.len() > 10);
    for path in paths {
        let roots = [path.clone()];
        let mut sources = SourceMap::default();
        let analysis = project::analyze(&Files, &mut sources, None, &roots);
        // warnings are fine: programs may leave names unused or `echo` values
        assert!(
            analysis
                .diagnostics
                .iter()
                .all(|d| d.severity == Severity::Warning),
            "{}: {:?}",
            path.display(),
            analysis.diagnostics
        );
        let (_, compilation) = project::compile(&Files, &mut SourceMap::default(), None, &roots);
        let fingerprint = analysis_fingerprint(&analysis.compilation);
        assert_eq!(
            analysis_fingerprint(&compilation),
            fingerprint,
            "{}",
            path.display()
        );
        let lookup = |m: &str| {
            analysis
                .module(m)
                .map(|a| a.interface.clone())
                .or_else(|| builtins().iter().find(|i| i.module == m).cloned())
        };
        for module in &analysis.modules {
            let text = &sources.get(module.file).text;
            let projected =
                projection::project(module, text, &fingerprint, &lookup, &|_| None, &|m| {
                    projection::relative(&module.name, m)
                });
            assert!(
                projected
                    .text
                    .starts_with(&format!("// flick-analysis {fingerprint}\n"))
            );
        }
    }
}
