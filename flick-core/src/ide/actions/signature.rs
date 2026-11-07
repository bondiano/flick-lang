//! "Add a signature" for a `let` of a value, whose type is inferred.

use super::super::{Ide, tree};
use super::{Action, edit, indent_at, line_start};
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Kind, Type, show_type, without_value_captures};
use crate::{diagnostics::Severity, resolve, syntax, types};

impl Ide<'_> {
    /// Propose a complete GADT signature for a type-level `todo` only after
    /// checking the proposed replacement as ordinary source.
    pub(super) fn infer_todo_signature(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let hole = path.iter().rev().find(|n| n.kind == K::TodoType)?;
        let sig = path.iter().rev().find(|n| n.kind == K::Signature)?;
        let name = tree::name_of(sig, K::Ident, text)?;
        let decl = m.tree.nodes().find(|n| {
            n.kind == K::LetDecl
                && n.range().0 > sig.range().1
                && n.nodes()
                    .find(|n| n.kind == K::VarPat)
                    .and_then(|p| tree::name_of(p, K::Ident, text))
                    == Some(name)
        })?;
        let params = decl.nodes().find(|n| n.kind == K::Params)?;
        if params.nodes().count() != 1 || !decl.tokens().iter().any(|t| t.kind == K::MatchKw) {
            return None;
        }
        let first_arm = descendant(decl, K::MatchArm)?;
        let constructor = descendant(first_arm, K::ConPat)?
            .child_tokens()
            .filter(|t| t.kind == K::Con)
            .last()?
            .text(text);
        for source in &self.analysis.modules {
            let source_text = self.text(source.file);
            for ty in source.tree.nodes().filter(|n| n.kind == K::TypeDecl) {
                let variants: Vec<_> = ty.nodes().filter(|n| n.kind == K::Variant).collect();
                if !variants.iter().any(|v| {
                    v.child_tokens()
                        .find(|t| t.kind == K::Con)
                        .is_some_and(|t| t.text(source_text) == constructor)
                }) {
                    continue;
                }
                let type_name = ty
                    .child_tokens()
                    .find(|t| t.kind == K::Con)?
                    .text(source_text);
                let vars: Vec<_> = ty
                    .nodes()
                    .filter(|n| n.kind == K::TypeParam)
                    .filter_map(|n| n.child_tokens().find(|t| t.kind == K::Ident))
                    .map(|t| t.text(source_text))
                    .collect();
                if vars.is_empty() {
                    continue;
                }
                let prefixes =
                    if source.name == m.name {
                        vec![String::new()]
                    } else if let Some(import) = resolve::module_syntax(&m.tree, text, file)
                        .imports
                        .into_iter()
                        .find(|i| i.module == source.name)
                    {
                        let qualified = format!("{}.", import.qualifier());
                        if import.exposing.iter().any(|item| {
                            item.name == type_name || item.kind == resolve::ItemKind::All
                        }) {
                            vec![String::new(), qualified]
                        } else {
                            vec![qualified]
                        }
                    } else {
                        continue;
                    };
                let inferred = self
                    .analysis
                    .diagnostics
                    .iter()
                    .find(|d| {
                        d.span
                            .is_some_and(|s| s.file == file && (s.start, s.end) == hole.range())
                    })
                    .and_then(|d| d.message.split('`').nth(3))
                    .and_then(|s| s.rsplit_once(" -> ").map(|(_, result)| result.to_owned()));
                let results: Vec<String> = vars
                    .iter()
                    .map(|v| (*v).to_owned())
                    .chain(inferred)
                    .chain(
                        ["Unit", "Bool", "Int", "Float", "BigInt", "String"]
                            .into_iter()
                            .map(str::to_owned),
                    )
                    .collect();
                for prefix in prefixes {
                    for result in &results {
                        let full = format!(
                            "forall {}. {prefix}{type_name} {} -> {result}",
                            vars.join(" "),
                            vars.join(" ")
                        );
                        let replacement = if hole.range()
                            == sig.nodes().find(|n| crate::types::is_type(n.kind))?.range()
                        {
                            full
                        } else {
                            result.clone()
                        };
                        let (start, end) = hole.range();
                        let mut candidate = text.to_owned();
                        candidate.replace_range(start as usize..end as usize, &replacement);
                        if !self.signature_candidate_checks(file, &candidate) {
                            continue;
                        }
                        return Some(Action {
                            title: format!("replace `todo` with `{replacement}`"),
                            kind: "quickfix",
                            edits: vec![edit(file, start, end, replacement)],
                        });
                    }
                }
            }
        }
        None
    }

    fn signature_candidate_checks(&self, file: FileId, text: &str) -> bool {
        let Some(m) = self.module(file) else {
            return false;
        };
        let parsed = syntax::parse(text, file);
        if parsed
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
        {
            return false;
        }
        let syntax = resolve::module_syntax(&parsed.tree, text, file);
        let module = resolve::Module {
            name: &m.name,
            file,
            text,
            tree: &parsed.tree,
            syntax: &syntax,
            generated: resolve::NO_GENERATED,
        };
        let imported = |name: &str| self.interface(name);
        let (mut interface, resolved) = resolve::resolve(&module, &imported);
        if resolved.iter().any(|d| d.severity == Severity::Error) {
            return false;
        }
        let local = |_: &str| true;
        let options = types::Options {
            local: &local,
            limits: Default::default(),
            no_let_assert: false,
            thresholds: Default::default(),
            effects: true,
        };
        !types::check(
            &module,
            &imported,
            types::builtins(),
            options,
            &mut interface,
        )
        .iter()
        .any(|d| d.severity == Severity::Error)
    }

    /// For the header of a `let name = …` with no signature above it; a
    /// function without one gets this from its `FLICK-T004` fix.
    pub(super) fn add_signature(&self, file: FileId, offset: u32) -> Option<Action> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let i = path.iter().rposition(|n| n.kind == K::LetDecl)?;
        let (decl, parent) = (path[i], *path.get(i.checked_sub(1)?)?);
        let eq = decl.child_tokens().find(|t| t.kind == K::Eq)?;
        if offset > eq.start || decl.nodes().any(|n| n.kind == K::Params) {
            return None;
        }
        let pattern = decl.nodes().find(|n| n.kind == K::VarPat)?;
        let name = pattern.child_tokens().find(|t| t.kind == K::Ident)?;
        let siblings: Vec<_> = parent.nodes().collect();
        let at = siblings.iter().position(|n| n.range() == decl.range())?;
        let signed = at > 0
            && siblings[at - 1].kind == K::Signature
            && tree::name_of(siblings[at - 1], K::Ident, text) == Some(name.text(text));
        if signed {
            return None;
        }
        let ty = m.typing.types.get(&(name.start, name.end))?;
        // a capture row cannot name a type variable; the written signature
        // implies those projections through the parameters
        let types: Vec<&str> = match ty {
            Type::Forall { vars, .. } => vars
                .iter()
                .filter(|v| matches!(v.kind, Kind::Type))
                .map(|v| v.name.as_str())
                .collect(),
            _ => Vec::new(),
        };
        let shown = show_type(&without_value_captures(ty, &types));
        // a type the checker did not finish has holes; it is no signature
        if shown
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|w| w == "_")
        {
            return None;
        }
        let name = name.text(text);
        let start = line_start(text, decl.range().0);
        let indent = indent_at(text, decl.range().0);
        Some(Action {
            title: format!("add the signature `{name} : {shown}`"),
            kind: "refactor.rewrite",
            edits: vec![edit(
                file,
                start,
                start,
                format!("{indent}{name} : {shown}\n"),
            )],
        })
    }
}

fn descendant(node: &crate::syntax::cst::Node, kind: K) -> Option<&crate::syntax::cst::Node> {
    node.nodes().find_map(|n| {
        if n.kind == kind {
            Some(n)
        } else {
            descendant(n, kind)
        }
    })
}
