//! Explanations of proof-specific syntax, using the active checker obligation.

use std::fmt::Write as _;
use std::ptr;

use super::super::{Ide, ProofView, Symbol, proof_symbols::proof_name, tree};
use crate::doc;
use crate::logic::ProofStep;
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::{source::FileId, types::Range};

impl Ide<'_> {
    pub(super) fn proof_hover(&self, file: FileId, offset: u32) -> Option<(String, Range)> {
        let module = self.module(file)?;
        if module.typing.proofs.is_empty() && module.typing.laws.is_empty() {
            return None;
        }
        let text = self.text(file);
        let path = tree::path_to(&module.tree, offset);
        let at = path
            .iter()
            .rposition(|node| matches!(node.kind, K::ProofDecl | K::LawDecl))?;
        let decl = path[at];
        let token = path[at..]
            .iter()
            .rev()
            .find_map(|node| token_at(node, offset))?;
        let range = (token.start, token.end);
        let written = token.text(text);
        let view = self.proof_at(file, offset)?;
        let steps = module
            .typing
            .proof_evidence
            .iter()
            .find(|evidence| evidence.at == decl.range())
            .map_or(&[][..], |evidence| evidence.steps.as_slice());
        if matches!(token.kind, K::ProofKw | K::LawKw) || proof_name(decl) == Some(token) {
            let mut shown = signature(decl, text);
            if let Some(docs) = declaration_docs(&path[..=at], text) {
                let _ = write!(shown, "\n\n{docs}");
            }
            if let Some(class) = view.class {
                let _ = write!(shown, "\n\nLaw of `{class}`.");
            } else {
                let _ = write!(shown, "\n\nVerification is **{}**.", status(&view));
            }
            return Some((with_context(shown, &view), range));
        }
        let site = Site {
            path: &path[at..],
            text,
            offset,
            token,
            steps,
        };
        if let Some((
            Symbol::Proof {
                module,
                class: None,
                name,
            },
            name_range,
        )) = self.symbol_at(file, offset)
            && let Some(source) = self.source(&module)
            && let Some(statement) = source.tree.nodes().find(|candidate| {
                candidate.kind == K::ProofDecl
                    && proof_name(candidate).is_some_and(|token| token.text(&source.text) == name)
            })
        {
            let mut shown = signature(statement, &source.text);
            if let Some(docs) = doc::leading_doc(&source.tree, statement, &source.text) {
                let _ = write!(shown, "\n\n{docs}");
            }
            if let Some(apply) = application(site.path, range) {
                application_details(&mut shown, apply, statement, &source.text, &site);
            }
            return Some((
                with_context(append_note(shown, site.bundle(&view).as_deref()), &view),
                name_range,
            ));
        }
        let note = site.note(&view);
        let ordinary = self
            .symbol_hover(file, offset)
            .or_else(|| self.operator_hover(file, offset))
            .or_else(|| self.head_hover(file, offset));
        if let Some(binding) = view.binders.iter().find(|binding| binding.name == written) {
            let shown = format!("```flick\n{} : {}\n```", binding.name, binding.ty);
            return Some((
                with_context(append_note(shown, note.as_deref()), &view),
                range,
            ));
        }
        if let Some(fact) = view.premises.iter().find(|fact| fact.name == written) {
            let shown = format!("**{}** · `{}`", fact.name, fact.statement);
            return Some((
                with_context(append_note(shown, note.as_deref()), &view),
                range,
            ));
        }
        match (note, ordinary) {
            (Some(note), Some((shown, range))) => {
                Some((with_context(append_note(shown, Some(&note)), &view), range))
            }
            (Some(note), None) => Some((with_context(note, &view), range)),
            (None, Some((shown, range))) => Some((with_context(shown, &view), range)),
            (None, None) => None,
        }
    }
}

struct Site<'a> {
    path: &'a [&'a Node],
    text: &'a str,
    offset: u32,
    token: Token,
    steps: &'a [ProofStep],
}

impl Site<'_> {
    fn note(&self, view: &ProofView<'_>) -> Option<String> {
        self.bundle(view)
            .or_else(|| self.let_binding())
            .or_else(|| {
                self.goal()
                    .or_else(|| self.rule())
                    .or_else(|| self.structure())
                    .map(str::to_owned)
            })
    }

    fn goal(&self) -> Option<&'static str> {
        let goal = self
            .path
            .iter()
            .rev()
            .find(|node| matches!(node.kind, K::Goal | K::SameTypeGoal | K::LawfulGoal))?;
        let first = goal.tokens().into_iter().find(|t| !t.kind.is_trivia())?;
        if self.token != first {
            return None;
        }
        match first.text(self.text) {
            "equal" => Some("Value equality. The two terms must denote the same value."),
            "holds" => Some("A Boolean proposition that must equal `true`."),
            "same_type" => Some("Type equality. Both types must be the same after elaboration."),
            "lawful" => Some("Certifies the laws of the selected instance and its superclasses."),
            _ => None,
        }
    }

    fn rule(&self) -> Option<&'static str> {
        for node in self.path.iter().rev() {
            if node.kind != K::AppExpr {
                continue;
            }
            let mut children = node.nodes();
            let Some(head) = children.next() else {
                continue;
            };
            let role = children.position(|node| contains(node.range(), self.offset));
            if let Some(note) = rule(head.text(self.text).trim(), role) {
                return Some(note);
            }
        }
        rule(self.token.text(self.text), None)
    }

    fn structure(&self) -> Option<&'static str> {
        self.path.iter().rev().find_map(|node| match node.kind {
            K::MatchArm => {
                Some("This branch checks the goal with the scrutinee replaced by its pattern.")
            }
            K::MatchExpr => {
                Some("Checks the goal separately for each constructor of the scrutinee.")
            }
            K::LetDecl => Some(
                "A proof `let` introduces a local fact from the equation its certificate proves.",
            ),
            _ => None,
        })
    }

    fn let_binding(&self) -> Option<String> {
        let index = self.path.iter().rposition(|node| node.kind == K::LetDecl)?;
        let declaration = self.path[index];
        let block = self.path.get(index.checked_sub(1)?)?;
        let binder = declaration.nodes().find(|node| node.kind == K::VarPat)?;
        let binding = binder.child_tokens().find(|token| token.kind == K::Ident)?;
        if self.token != binding && self.token.kind != K::LetKw {
            return None;
        }
        let before = self
            .steps
            .iter()
            .find(|step| inside(declaration.range(), step))?;
        let after = self
            .steps
            .iter()
            .find(|step| step.span.start >= declaration.range().1 && inside(block.range(), step))?;
        // The check of the rest of this block starts with the previous scope
        // plus the equation synthesized for this binder.
        let [(name, equation)] = after.premises.strip_prefix(before.premises.as_slice())? else {
            return None;
        };
        if name != binding.text(self.text) {
            return None;
        }
        Some(format!(
            "A proof `let` introduces a local fact.\n\n**{name}** · `{equation}`"
        ))
    }

    fn bundle(&self, view: &ProofView<'_>) -> Option<String> {
        view.instance.as_ref()?;
        if let Some(law) = view
            .obligations
            .iter()
            .find(|law| law.range.is_some_and(|at| contains(at, self.offset)))
        {
            let path = law
                .path
                .iter()
                .map(String::as_str)
                .chain([law.law.as_str()])
                .collect::<Vec<_>>()
                .join(".");
            return Some(format!(
                "This lemma must prove the instantiated law `{path}` of the selected instance."
            ));
        }
        let fields = self
            .path
            .iter()
            .filter(|node| node.kind == K::RecordField)
            .filter_map(|node| {
                node.child_tokens()
                    .find(|token| matches!(token.kind, K::Ident | K::Con))
            })
            .map(|token| token.text(self.text))
            .collect::<Vec<_>>();
        if !fields.is_empty() {
            let path = fields.join(".");
            if view.obligations.iter().any(|law| {
                fields.len() <= law.path.len()
                    && law
                        .path
                        .iter()
                        .zip(&fields)
                        .all(|(expected, written)| expected == written)
            }) {
                return Some(format!(
                    "The superclass path `{path}` groups its law obligations."
                ));
            }
            return Some(format!(
                "`{path}` has no corresponding law or superclass here."
            ));
        }
        (self.token.text(self.text) != "lawful"
            && self.path.iter().any(|node| node.kind == K::LawfulGoal))
        .then(|| "The law bundle uses the dictionary selected for this class and type.".to_owned())
    }
}

fn application<'a>(path: &[&'a Node], range: Range) -> Option<&'a Node> {
    path.iter().rev().copied().find(|node| {
        node.kind == K::AppExpr
            && node
                .nodes()
                .next()
                .is_some_and(|head| contains(head.range(), range.0))
    })
}

fn application_details(
    shown: &mut String,
    apply: &Node,
    statement: &Node,
    source: &str,
    site: &Site<'_>,
) {
    let args: Vec<_> = apply.nodes().skip(1).collect();
    let written = args
        .iter()
        .map(|arg| format!("`{}`", arg.text(site.text).trim()))
        .collect::<Vec<_>>();
    if !written.is_empty() {
        let _ = write!(shown, "\n\nArguments here · {}.", written.join(", "));
    }
    let params = statement
        .nodes()
        .filter(|node| node.kind == K::Params)
        .flat_map(Node::nodes);
    for (param, arg) in params
        .zip(args)
        .filter(|(param, _)| param.kind == K::Premise)
    {
        let name = param
            .nodes()
            .find(|node| node.kind == K::VarPat)
            .and_then(|node| tree::name_of(node, K::Ident, source));
        let obligation = site
            .steps
            .iter()
            .filter(|step| inside(arg.range(), step))
            .max_by_key(|step| step.span.end - step.span.start);
        if let (Some(name), Some(obligation)) = (name, obligation) {
            let _ = write!(shown, "\n\nPremise `{name}` here · `{}`", obligation.goal);
        }
    }
}

fn declaration_docs(path: &[&Node], text: &str) -> Option<String> {
    let [ancestors @ .., parent, declaration] = path else {
        return None;
    };
    doc::leading_doc(parent, declaration, text).or_else(|| {
        let class = ancestors.last()?;
        // For the first law, the leading comment can precede Members itself.
        if parent.kind == K::Members && ptr::eq(parent.nodes().next()?, *declaration) {
            doc::leading_doc(class, parent, text)
        } else {
            None
        }
    })
}

fn inside((start, end): Range, step: &ProofStep) -> bool {
    start <= step.span.start && step.span.end <= end
}

fn contains((start, end): Range, offset: u32) -> bool {
    start <= offset && offset <= end
}

fn token_at(node: &Node, offset: u32) -> Option<Token> {
    node.child_tokens()
        .find(|token| token.start <= offset && offset < token.end)
        .or_else(|| node.child_tokens().find(|token| token.end == offset))
}

fn signature(node: &Node, text: &str) -> String {
    let (start, end) = node.range();
    let end = node
        .child_tokens()
        .find(|token| token.kind == K::Eq)
        .map_or(end, |token| token.start);
    let header = &text[start as usize..end as usize];
    format!("```flick\n{}\n```", header.trim())
}

fn status(view: &ProofView<'_>) -> &'static str {
    use crate::types::ProofStatus;
    match view.status {
        ProofStatus::Checked => "checked",
        ProofStatus::Open => "open",
        ProofStatus::Blocked => "blocked",
        ProofStatus::BudgetExhausted => "budget exhausted",
    }
}

fn append_note(mut shown: String, note: Option<&str>) -> String {
    if let Some(note) = note {
        let _ = write!(shown, "\n\n{note}");
    }
    shown
}

fn with_context(mut shown: String, view: &ProofView<'_>) -> String {
    if let Some(goal) = &view.goal {
        let _ = write!(shown, "\n\n**Goal** · `{goal}`");
    }
    if let Some(instance) = view.instance {
        let _ = write!(shown, "\n\n**Instance** · `{instance}`");
    }
    for substitution in &view.substitutions {
        let _ = write!(
            shown,
            "\n\n**Branch** · `{} = {}`",
            substitution.name, substitution.value
        );
    }
    if !view.obligations.is_empty() {
        let _ = write!(shown, "\n\n**Laws**");
        for law in view.obligations {
            let mut path = law.path.join(".");
            if !path.is_empty() {
                path.push('.');
            }
            let _ = write!(shown, "\n- `{path}{}`", law.law);
            if let Some(statement) = &law.statement {
                let _ = write!(shown, " · `{statement}`");
            }
        }
    }
    shown
}

fn rule(name: &str, argument: Option<usize>) -> Option<&'static str> {
    match (name, argument) {
        ("refl", None) => Some("Reflexivity. The two sides must normalize to the same term."),
        ("symm", None) => Some("Symmetry reverses the equality."),
        ("symm", Some(0)) => Some("A proof of the reversed equality."),
        ("trans", None) => Some("Transitivity combines two equalities through a middle term."),
        ("trans", Some(0)) => Some("The middle term shared by both equalities."),
        ("trans", Some(1)) => Some("The first proof connects the left side to the middle term."),
        ("trans", Some(2)) => Some("The second proof connects the middle term to the right side."),
        ("cong", None) => Some("Congruence applies the same context to both sides of an equality."),
        ("cong", Some(0)) => Some("The function applied to both sides of the premise."),
        ("cong", Some(1)) => Some("The premise proves equality before applying the context."),
        ("funext", None) => Some("Function extensionality proves agreement at any fresh argument."),
        ("funext", Some(0)) => {
            Some("Introduce a fresh argument and prove equality of the results.")
        }
        _ => None,
    }
}
