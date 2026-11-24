//! `flick prove`: the proof modules of the project's proof root and every
//! source or test module declaring contracts or proofs, checked with the
//! modules they import. `check`, `build` and `test` never prove contracts and
//! never read the proof root, so code that asks for no proof pays for none.

use std::fs;
use std::path::Path;

use flick_core::project;
use flick_core::source::SourceMap;
use flick_core::syntax::{self, cst::Node, kind::SyntaxKind as K};

use crate::sources::{self, FsHost};

/// What `flick prove` verifies: `paths`, or the project's proof root together
/// with any source/test files that declare contracts or proofs. `None` when
/// the project has neither: there is nothing to prove.
pub fn proof_paths(paths: &[String], explicit: Option<&str>) -> Option<Vec<String>> {
    if !paths.is_empty() {
        return Some(paths.to_vec());
    }
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    // without a project, checking reports that there is none
    let Ok(Some(project)) =
        project::discover(&host, &mut sources, &host.cwd, explicit.map(Path::new))
    else {
        return Some(Vec::new());
    };
    let mut selected = Vec::new();
    let proof_root = project.proof_root();
    if proof_root.is_dir() {
        let mut proof_files = Vec::new();
        sources::source_files(&proof_root.to_string_lossy(), &mut proof_files);
        if !proof_files.is_empty() {
            selected.push(proof_root.to_string_lossy().into_owned());
        }
    }
    for root in [project.source_root(), project.test_root()] {
        if !root.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        sources::source_files(&root.to_string_lossy(), &mut files);
        for file in files {
            if has_contract_or_proof(&file) {
                selected.push(file);
            }
        }
    }
    if selected.is_empty() {
        println!(
            "no proofs: `{}` has no proof modules",
            project.config.proof_dir
        );
        return None;
    }
    Some(selected)
}

fn has_contract_or_proof(path: &str) -> bool {
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    if !text.contains("where") && !text.contains("proof") {
        return false;
    }
    let mut sources = SourceMap::default();
    let file = sources.add(path, text.as_str());
    let parse = syntax::parse(&text, file);
    tree_has_contract_or_proof(&parse.tree)
}

fn tree_has_contract_or_proof(node: &Node) -> bool {
    if matches!(node.kind, K::Contract | K::ProofDecl) {
        return true;
    }
    node.nodes().any(tree_has_contract_or_proof)
}
