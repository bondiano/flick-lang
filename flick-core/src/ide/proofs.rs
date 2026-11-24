//! Position-based goals and facts from the current analysis.

use serde::Serialize;

use super::{Ide, tree};
use crate::source::FileId;
use crate::syntax::{cst::Node, kind::SyntaxKind as K};
#[cfg(target_arch = "wasm32")]
use crate::types::ProofEvidence;
use crate::types::{
    GoalSyntax, GoalType, LawObligation, ProofInfo, ProofStatus, Range, goal_syntax,
};

#[derive(Debug, Serialize)]
pub struct ProofBinder {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

#[derive(Debug, Serialize)]
pub struct ProofFact {
    pub name: String,
    pub statement: String,
}

#[derive(Debug, Serialize)]
pub struct ProofSubstitution {
    pub name: String,
    pub value: String,
}

/// The declaration and the innermost obligation observed at the cursor.
/// Ranges in the shared query are UTF-8 byte offsets in the queried file.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofView<'a> {
    pub name: &'a str,
    pub range: Range,
    pub class: Option<&'a str>,
    pub status: ProofStatus,
    pub declared_goal: Option<String>,
    pub goal: Option<String>,
    pub goal_range: Option<Range>,
    pub failure_range: Option<Range>,
    pub binders: Vec<ProofBinder>,
    pub premises: Vec<ProofFact>,
    pub substitutions: Vec<ProofSubstitution>,
    pub lemmas: &'a [String],
    pub instance: Option<&'a str>,
    pub obligations: &'a [LawObligation],
    pub normalization_steps: Option<u64>,
}

impl Ide<'_> {
    pub fn proof_at(&self, file: FileId, offset: u32) -> Option<ProofView<'_>> {
        let module = self.analysis.by_file(file)?;
        let text = self.text(file);
        let declaration = declaration_at(&module.tree, text, offset)?;
        let at = declaration.range();
        let info = module
            .typing
            .proofs
            .iter()
            .chain(&module.typing.laws)
            .find(|info| info.at == at)?;
        #[cfg(not(target_arch = "wasm32"))]
        let evidence = module.typing.proof_evidence.iter().find(|e| e.at == at);
        #[cfg(target_arch = "wasm32")]
        let evidence: Option<&ProofEvidence> = None;
        let step_offset = step_offset(declaration, text, offset);
        let step = evidence
            .into_iter()
            .flat_map(|e| &e.steps)
            .filter(|s| {
                s.span.file == file && s.span.start <= step_offset && step_offset <= s.span.end
            })
            .min_by_key(|s| s.span.end - s.span.start);
        let obligations = evidence.map_or(&[][..], |e| e.obligations.as_slice());
        let obligation = obligations.iter().find(|o| {
            o.range
                .is_some_and(|(start, end)| start <= offset && offset <= end)
        });
        let declared_goal = goal_of(declaration).and_then(|node| show_goal(node, text));
        let (binders, premises) = step.map_or_else(
            || header_scope(declaration, info, text, offset),
            |step| {
                (
                    step.binders
                        .iter()
                        .map(|(name, ty)| ProofBinder {
                            name: name.clone(),
                            ty: ty.clone(),
                        })
                        .collect(),
                    step.premises
                        .iter()
                        .map(|(name, statement)| ProofFact {
                            name: name.clone(),
                            statement: statement.clone(),
                        })
                        .collect(),
                )
            },
        );
        let goal = step
            .map(|s| s.goal.clone())
            .or_else(|| obligation.and_then(|o| o.statement.clone()))
            .or_else(|| declared_goal.clone());
        let goal_range = step
            .map(|s| (s.span.start, s.span.end))
            .or_else(|| obligation.and_then(|o| o.range))
            .or_else(|| goal_of(declaration).map(Node::range));
        Some(ProofView {
            name: &info.name,
            range: at,
            class: info.class.as_ref().map(|law| law.class.as_str()),
            status: evidence.map_or_else(
                || match info.goal {
                    GoalType::Outside(_) | GoalType::Malformed => ProofStatus::Blocked,
                    _ => ProofStatus::Open,
                },
                |e| e.status,
            ),
            declared_goal,
            goal,
            goal_range,
            failure_range: evidence
                .and_then(|e| e.failure)
                .filter(|span| span.file == file)
                .map(|span| (span.start, span.end)),
            binders,
            premises,
            substitutions: step.map_or_else(Vec::new, |step| {
                step.substitutions
                    .iter()
                    .map(|(name, value)| ProofSubstitution {
                        name: name.clone(),
                        value: value.clone(),
                    })
                    .collect()
            }),
            lemmas: evidence.map_or(&[], |e| &e.lemmas),
            instance: evidence.and_then(|e| e.instance.as_deref()),
            obligations,
            normalization_steps: step.map(|s| s.normalization_steps),
        })
    }
}

fn declaration_at<'a>(root: &'a Node, text: &str, offset: u32) -> Option<&'a Node> {
    tree::path_to(root, offset)
        .into_iter()
        .rev()
        .find(|n| matches!(n.kind, K::ProofDecl | K::LawDecl))
        .or_else(|| {
            // Just after an empty body, whitespace belongs to the cursor's draft.
            let decl = root.nodes().take_while(|n| n.range().1 < offset).last()?;
            let draft = decl.kind == K::ProofDecl
                && decl.has_token(K::Eq)
                && decl
                    .nodes()
                    .find(|n| n.kind == K::Block)
                    .is_none_or(|block| block.nodes().next().is_none());
            let gap = text.get(decl.range().1 as usize..offset as usize)?;
            (draft && gap.trim().is_empty()).then_some(decl)
        })
}

/// A branch pattern or fresh binder describes the obligation of its body.
fn step_offset(declaration: &Node, text: &str, offset: u32) -> u32 {
    let path = tree::path_to(declaration, offset);
    for (index, node) in path.iter().enumerate().rev() {
        match node.kind {
            K::MatchArm => {}
            K::FunExpr
                if path[..index]
                    .iter()
                    .rev()
                    .find(|n| n.kind == K::AppExpr)
                    .and_then(|n| n.nodes().next())
                    .is_some_and(|head| head.text(text).trim() == "funext") => {}
            _ => continue,
        }
        if let Some(body) = node.nodes().find(|n| n.kind == K::Block)
            && offset < body.range().0
        {
            return body.range().0;
        }
    }
    offset
}

fn goal_of(node: &Node) -> Option<&Node> {
    node.nodes()
        .find(|n| matches!(n.kind, K::Goal | K::SameTypeGoal | K::LawfulGoal))
}

pub(super) fn show_goal(node: &Node, text: &str) -> Option<String> {
    let shown = match goal_syntax(node, text) {
        GoalSyntax::Equal(left, right) => {
            format!("{} = {}", left.text(text).trim(), right.text(text).trim())
        }
        GoalSyntax::Holds(expr) => format!("{} = true", expr.text(text).trim()),
        GoalSyntax::SameType(..) | GoalSyntax::Other(_) => node.text(text).trim().to_owned(),
    };
    (!shown.is_empty()).then_some(shown)
}

fn header_scope(
    declaration: &Node,
    info: &ProofInfo,
    text: &str,
    offset: u32,
) -> (Vec<ProofBinder>, Vec<ProofFact>) {
    let mut binders = Vec::new();
    let mut premises = Vec::new();
    for (param, ty) in declaration
        .nodes()
        .filter(|n| n.kind == K::Params)
        .flat_map(Node::nodes)
        .zip(&info.params)
        .take_while(|(param, _)| param.range().0 <= offset)
    {
        let Some(name) = parameter_name(param, text) else {
            continue;
        };
        if param.kind == K::Premise {
            if let Some(statement) = goal_of(param).and_then(|g| show_goal(g, text)) {
                premises.push(ProofFact { name, statement });
            }
        } else {
            binders.push(ProofBinder {
                name,
                ty: ty.to_string(),
            });
        }
    }
    (binders, premises)
}

fn parameter_name(node: &Node, text: &str) -> Option<String> {
    if node.kind == K::VarPat {
        return node
            .child_tokens()
            .find(|t| t.kind == K::Ident)
            .map(|t| t.text(text).to_owned());
    }
    node.nodes().find_map(|child| parameter_name(child, text))
}

#[cfg(test)]
mod tests {
    use super::super::tests::fixture_of;
    use super::ProofStatus;

    #[test]
    fn proof_drafts_keep_checked_earlier_lemmas_and_the_declared_goal() {
        let source = "module App.Main exposing (..)\n\
proof early where equal true true = refl\n\
proof wrong where equal true false = refl\n\
proof edit (x : Bool) (y : Bool) (h : equal x y) where equal y x = ";
        let fixture = fixture_of(&[("src/App/Main.fl", source)]);
        let ide = fixture.ide();
        let offset = u32::try_from(source.len()).expect("small");
        let proof = ide
            .proof_at(fixture.file("App.Main"), offset)
            .expect("proof draft");
        assert_eq!(proof.status, ProofStatus::Open, "{proof:?}");
        assert_eq!(proof.declared_goal.as_deref(), Some("y = x"));
        assert_eq!(proof.lemmas, ["early"]);
        assert_eq!(proof.premises[0].statement, "x = y");
    }
}
