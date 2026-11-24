//! Source identities shared by hover, definitions and references in proofs.

use super::{Ide, Location, Symbol, tree};
use crate::logic;
#[cfg(not(target_arch = "wasm32"))]
use crate::project::Analyzed;
use crate::source::FileId;
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;

pub(super) fn proof_name(decl: &Node) -> Option<Token> {
    decl.nodes()
        .find(|node| node.kind == K::VarPat)?
        .child_tokens()
        .find(|token| token.kind == K::Ident)
}

pub(super) fn laws(class: &Node) -> impl Iterator<Item = &Node> {
    class
        .nodes()
        .filter(|node| node.kind == K::Members)
        .flat_map(Node::nodes)
        .filter(|node| node.kind == K::LawDecl)
}

pub(super) fn lemma_symbol(module: &str, name: &str) -> Symbol {
    let (module, name) = name.rsplit_once('.').unwrap_or((module, name));
    Symbol::Proof {
        module: module.to_owned(),
        class: None,
        name: name.to_owned(),
    }
}

impl Ide<'_> {
    pub(super) fn proof_symbol_at(
        &self,
        file: FileId,
        offset: u32,
    ) -> Option<(Symbol, (u32, u32))> {
        let module = self.module(file)?;
        if module.typing.proofs.is_empty() && module.typing.laws.is_empty() {
            return None;
        }
        let text = self.text(file);
        let path = tree::path_to(&module.tree, offset);
        let decl = path
            .iter()
            .rev()
            .find(|node| matches!(node.kind, K::ProofDecl | K::LawDecl))?;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(field) = Self::law_field_at(module, decl, &path, offset) {
            return Some(field);
        }
        let name = proof_name(decl)?;
        let class = path
            .iter()
            .find(|node| node.kind == K::ClassDecl)
            .and_then(|node| tree::name_of(node, K::Con, text))
            .map(str::to_owned);
        (name.start <= offset && offset <= name.end).then(|| {
            (
                Symbol::Proof {
                    module: module.name.clone(),
                    class,
                    name: name.text(text).to_owned(),
                },
                (name.start, name.end),
            )
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn law_field_at(
        module: &Analyzed,
        decl: &Node,
        path: &[&Node],
        offset: u32,
    ) -> Option<(Symbol, (u32, u32))> {
        let record = path.iter().rev().find(|node| node.kind == K::RecordField)?;
        let key = law_key(record)?;
        if offset < key.start || offset > key.end {
            return None;
        }
        let proof = module
            .typing
            .proof_evidence
            .iter()
            .find(|proof| proof.at == decl.range())?;
        let law = proof
            .obligations
            .iter()
            .find(|law| law.range == Some(record.range()))?;
        Some((
            Symbol::Proof {
                module: law.class_module.clone(),
                class: Some(law.class.clone()),
                name: law.law.clone(),
            },
            (key.start, key.end),
        ))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn law_references(
        &self,
        module: &str,
        class: &str,
        name: &str,
    ) -> Vec<super::rename::Occurrence> {
        self.analysis
            .modules
            .iter()
            .flat_map(|source| {
                source
                    .typing
                    .proof_evidence
                    .iter()
                    .flat_map(|proof| &proof.obligations)
                    .filter(|law| {
                        law.class_module == module && law.class == class && law.law == name
                    })
                    .filter_map(|law| {
                        let range = law.range?;
                        let tree = tree::find(&source.tree, range)?;
                        let field = (tree.kind == K::RecordField).then_some(tree)?;
                        let key = law_key(field)?;
                        Some((source.file, (key.start, key.end)))
                    })
            })
            .collect()
    }

    pub(super) fn proof_definition(
        &self,
        module: &str,
        class: Option<&str>,
        name: &str,
    ) -> Option<Location> {
        let source = self.source(module)?;
        let found = |n: &Node| proof_name(n).filter(|t| t.text(&source.text) == name);
        let token = if let Some(class) = class {
            let class = source.tree.nodes().find(|n| {
                n.kind == K::ClassDecl && tree::name_of(n, K::Con, &source.text) == Some(class)
            })?;
            laws(class).find_map(found)?
        } else {
            source
                .tree
                .nodes()
                .filter(|n| n.kind == K::ProofDecl)
                .find_map(found)?
        };
        Some(Location {
            target: source.target,
            start: token.start,
            end: token.end,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn law_key(field: &Node) -> Option<Token> {
    field
        .child_tokens()
        .find(|token| matches!(token.kind, K::Ident | K::Con))
}

/// Internal proof names are identities, even when a source alias was used.
pub(super) fn is_lemma(module: &str) -> bool {
    module == logic::LEMMAS
}
