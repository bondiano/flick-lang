//! Checked recommendations and structural completions for proof source.

use std::collections::{BTreeMap, HashSet};

use super::actions::Action;
use super::complete::{Completion, CompletionKind, item};
use super::proofs::{ProofView, declaration_at, goal_of};
use super::{Help, Ide, tree};
use crate::diagnostics::Edit;
use crate::logic::LEMMAS;
use crate::project::Analyzed;
use crate::source::{FileId, Span};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{LawObligation, Range, Ref, record};

const HEADS: &[(&str, &str)] = &[
    ("equal", "Equality of two values"),
    ("holds", "Boolean proposition"),
    ("same_type", "Equality of two types"),
    ("lawful", "Laws of a class instance"),
];

const STRUCTURES: &[(&str, &str)] = &[
    ("symm", "symm ${1:_}"),
    ("trans", "trans ${1:middle} ${2:_} ${3:_}"),
    ("cong", "cong (fun ${1:x} -> ${2:x}) ${3:_}"),
    ("funext", "funext (fun ${1:x} -> ${2:_})"),
    ("match", "match ${1:value} with\n| ${2:pattern} -> ${3:_}"),
];

impl Ide<'_> {
    pub(super) fn proof_completion(
        &self,
        file: FileId,
        at: u32,
        dotted: bool,
    ) -> Option<Vec<Completion>> {
        let module = self.module(file)?;
        let text = self.text(file);
        let decl = declaration_at(&module.tree, text, at)?;
        let path = tree::path_to(decl, at);
        let body = decl.child_tokens().find(|t| t.kind == K::Eq);
        if body.is_none_or(|token| at < token.end) {
            return goal_heads(decl, &path, at, dotted);
        }
        let proof = self.proof_at(file, at)?;
        let qualifier = dotted.then(|| qualifier_before(text, at)).flatten();
        if goal_of(decl).is_some_and(|goal| goal.kind == K::LawfulGoal) {
            return Some(bundle_completion(&proof, &path, text, at, qualifier));
        }
        if self.ordinary_proof_slot(module, &path, text, at) {
            return None;
        }
        Some(term_completion(&proof, qualifier))
    }

    pub(super) fn proof_signature_help(&self, file: FileId, at: u32) -> Option<Help> {
        let module = self.module(file)?;
        let text = self.text(file);
        let declaration = declaration_at(&module.tree, text, at)?;
        let path = tree::path_to(declaration, at);
        for node in path.into_iter().rev() {
            let Some((head, active)) = call(node, at) else {
                continue;
            };
            let name = head.text(text);
            let roles: Vec<String> = match name.trim() {
                "symm" => vec!["proof of the reversed goal".into()],
                "trans" => vec![
                    "middle value".into(),
                    "proof from left to middle".into(),
                    "proof from middle to right".into(),
                ],
                "cong" => vec![
                    "function context".into(),
                    "proof before applying the context".into(),
                ],
                "funext" => vec!["proof at a fresh argument".into()],
                _ => self
                    .lemma_parameters(module, head)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(premise, name)| {
                        if premise {
                            format!("premise {name}")
                        } else {
                            name
                        }
                    })
                    .collect(),
            };
            if roles.is_empty() {
                continue;
            }
            let active = active.min(roles.len() - 1);
            let mut label = name.trim().to_owned();
            let mut params = Vec::new();
            for role in roles {
                label.push(' ');
                let start = to_u32(label.len());
                label.push_str(&role);
                params.push((start, to_u32(label.len())));
            }
            return Some(Help {
                label,
                params,
                active,
            });
        }
        None
    }

    fn ordinary_proof_slot(&self, module: &Analyzed, path: &[&Node], text: &str, at: u32) -> bool {
        if let Some(split) = path.iter().rev().find(|node| node.kind == K::MatchExpr)
            && split
                .nodes()
                .next()
                .is_some_and(|scrutinee| contains(scrutinee.range(), at))
        {
            return true;
        }
        path.iter()
            .rev()
            .filter_map(|node| call(node, at))
            .any(|(head, arg)| {
                if matches!(head.text(text).trim(), "trans" | "cong") {
                    arg == 0
                } else {
                    self.lemma_parameters(module, head)
                        .and_then(|params| params.get(arg).map(|(premise, _)| !premise))
                        .unwrap_or_default()
                }
            })
    }

    fn lemma_parameters(&self, current: &Analyzed, head: &Node) -> Option<Vec<(bool, String)>> {
        let Ref::Value { module, name } = current.typing.names.get(&head.range())? else {
            return None;
        };
        if module != LEMMAS {
            return None;
        }
        let (module, name) = name.rsplit_once('.').unwrap_or((&current.name, name));
        let defined = self.analysis.module(module)?;
        let at = defined
            .typing
            .proofs
            .iter()
            .find(|proof| proof.name == name)?
            .at;
        let decl = tree::find(&defined.tree, at)?;
        let text = self.text(defined.file);
        Some(
            decl.nodes()
                .filter(|node| node.kind == K::Params)
                .flat_map(Node::nodes)
                .map(|param| (param.kind == K::Premise, param.text(text).trim().into()))
                .collect(),
        )
    }

    pub(super) fn scaffold_laws(&self, file: FileId, at: u32) -> Option<Action> {
        let module = self.module(file)?;
        let text = self.text(file);
        let decl = declaration_at(&module.tree, text, at)?;
        let proof = self.proof_at(file, at)?;
        let body = decl.nodes().find(|n| n.kind == K::Block).and_then(record)?;
        let mut edits = Vec::new();
        scaffold(file, text, body, proof.obligations, &[], &mut edits);
        (!edits.is_empty()).then(|| Action {
            title: "Add missing law fields".into(),
            kind: "quickfix",
            edits,
        })
    }
}

fn goal_heads(decl: &Node, path: &[&Node], at: u32, dotted: bool) -> Option<Vec<Completion>> {
    let after_where = decl
        .child_tokens()
        .any(|t| t.kind == K::WhereKw && t.end <= at);
    let in_premise = path.iter().any(|node| node.kind == K::Premise);
    if dotted || !after_where && !in_premise {
        return None;
    }
    let goal = path
        .iter()
        .rev()
        .find(|node| matches!(node.kind, K::Goal | K::SameTypeGoal | K::LawfulGoal))
        .copied()
        .or_else(|| goal_of(decl));
    if let Some(goal) = goal
        && goal
            .tokens()
            .into_iter()
            .find(|t| !t.kind.is_trivia())
            .is_some_and(|t| t.end < at)
    {
        return None;
    }
    Some(
        HEADS
            .iter()
            .filter(|(head, _)| !in_premise || *head != "lawful")
            .map(|(head, detail)| item(head, CompletionKind::Keyword, Some((*detail).into())))
            .collect(),
    )
}

fn term_completion(proof: &ProofView<'_>, qualifier: Option<&str>) -> Vec<Completion> {
    let checked = proof.suggestions.iter().filter_map(|suggestion| {
        let name = match qualifier {
            Some(alias) => suggestion.strip_prefix(alias)?.strip_prefix('.')?,
            None => suggestion,
        };
        Some(item(
            name,
            CompletionKind::Value,
            Some("Closes the goal".into()),
        ))
    });
    let mut out: Vec<_> = checked.collect();
    let mut seen: HashSet<_> = out.iter().map(|i| i.label.clone()).collect();
    for lemma in proof.lemmas {
        let name = match qualifier {
            Some(alias) => lemma.strip_prefix(alias).and_then(|n| n.strip_prefix('.')),
            None => Some(lemma.as_str()),
        };
        if let Some(name) = name
            && seen.insert(name.into())
        {
            out.push(ranked(name, CompletionKind::Value, "Checked lemma", 1));
        }
    }
    if qualifier.is_some() {
        return out;
    }
    for fact in &proof.premises {
        if seen.insert(fact.name.clone()) {
            out.push(ranked(
                &fact.name,
                CompletionKind::Local,
                &fact.statement,
                1,
            ));
        }
    }
    for binder in &proof.binders {
        if seen.insert(binder.name.clone()) {
            out.push(ranked(&binder.name, CompletionKind::Local, &binder.ty, 2));
        }
    }
    if seen.insert("refl".into()) {
        out.push(ranked(
            "refl",
            CompletionKind::Keyword,
            "Try reflexivity",
            3,
        ));
    }
    if proof
        .declared_goal
        .as_deref()
        .is_none_or(|goal| !goal.starts_with("same_type"))
    {
        out.extend(
            STRUCTURES
                .iter()
                .filter(|(name, _)| !seen.contains(*name))
                .map(|(name, snippet)| {
                    let mut item =
                        ranked(name, CompletionKind::Keyword, "Leaves proof obligations", 4);
                    item.snippet = Some((*snippet).into());
                    item
                }),
        );
    }
    out
}

fn bundle_completion(
    proof: &ProofView<'_>,
    path: &[&Node],
    text: &str,
    at: u32,
    qualifier: Option<&str>,
) -> Vec<Completion> {
    let Some((record, parents)) = law_record(path, text) else {
        return Vec::new();
    };
    let field = record
        .nodes()
        .filter(|node| node.kind == K::RecordField)
        .find(|node| {
            let (start, end) = node.range();
            contains((start, end), at)
                || end < at
                    && node.child_tokens().any(|token| token.kind == K::Eq)
                    && node
                        .nodes()
                        .next()
                        .is_none_or(|body| body.range() == (0, 0))
                    && text
                        .get(end as usize..at as usize)
                        .is_some_and(|gap| gap.trim().is_empty())
        });
    if let Some((field, name)) = field.and_then(|field| Some((field, field_name(field, text)?)))
        && field
            .child_tokens()
            .any(|token| token.kind == K::Eq && token.end <= at)
    {
        return proof
            .obligations
            .iter()
            .find(|law| law.path == parents && law.law == name)
            .into_iter()
            .flat_map(|law| &law.suggestions)
            .filter_map(|name| match qualifier {
                Some(alias) => name.strip_prefix(alias)?.strip_prefix('.'),
                None => Some(name),
            })
            .map(|name| item(name, CompletionKind::Value, Some("Proves this law".into())))
            .collect();
    }
    if qualifier.is_some() {
        return Vec::new();
    }
    let existing: HashSet<_> = record
        .nodes()
        .filter_map(|node| field_name(node, text))
        .collect();
    missing(proof.obligations, &parents, &existing)
        .into_keys()
        .map(|name| {
            let mut entry = item(
                name,
                CompletionKind::Field,
                Some("Missing law or superclass".into()),
            );
            let rhs = if proof
                .obligations
                .iter()
                .any(|law| law.path == parents && law.law == name)
            {
                "${1:_}"
            } else {
                "{ ${1} }"
            };
            entry.snippet = Some(format!("{name} = {rhs}"));
            entry
        })
        .collect()
}

fn scaffold(
    file: FileId,
    text: &str,
    record_node: &Node,
    obligations: &[LawObligation],
    parents: &[&str],
    out: &mut Vec<Edit>,
) {
    let mut fields = HashSet::new();
    for field in record_node
        .nodes()
        .filter(|node| node.kind == K::RecordField)
    {
        let Some(name) = field_name(field, text) else {
            continue;
        };
        fields.insert(name);
        if let Some(inner) = field.nodes().last().and_then(record) {
            let mut parent = parents.to_vec();
            parent.push(name);
            scaffold(file, text, inner, obligations, &parent, out);
        }
    }
    let missing = missing(obligations, parents, &fields);
    if missing.is_empty() {
        return;
    }
    let Some(close) = record_node.child_tokens().find(|t| t.kind == K::RBrace) else {
        return;
    };
    let already_separated = record_node
        .child_tokens()
        .filter(|t| t.end <= close.start)
        .last()
        .is_some_and(|t| t.kind == K::Semi);
    let prefix = if fields.is_empty() || already_separated {
        " "
    } else {
        "; "
    };
    let body = missing
        .into_iter()
        .map(|(name, child)| format!("{name} = {}", child.render()))
        .collect::<Vec<_>>()
        .join("; ");
    out.push(Edit {
        span: Span::new(file, close.start, close.start),
        replacement: format!("{prefix}{body} "),
    });
}

enum Scaffold<'a> {
    Law,
    Super(BTreeMap<&'a str, Scaffold<'a>>),
}

impl Scaffold<'_> {
    fn render(self) -> String {
        match self {
            Self::Law => "_".into(),
            Self::Super(children) => {
                let entries = children
                    .into_iter()
                    .map(|(name, child)| format!("{name} = {}", child.render()))
                    .collect::<Vec<_>>()
                    .join("; ");
                format!("{{ {entries} }}")
            }
        }
    }
}

fn missing<'a>(
    obligations: &'a [LawObligation],
    parents: &[&str],
    existing: &HashSet<&str>,
) -> BTreeMap<&'a str, Scaffold<'a>> {
    let mut missing = BTreeMap::new();
    for law in obligations.iter().filter(|law| {
        law.path
            .iter()
            .map(String::as_str)
            .take(parents.len())
            .eq(parents.iter().copied())
            && law.path.len() >= parents.len()
    }) {
        let mut path = law
            .path
            .iter()
            .map(String::as_str)
            .skip(parents.len())
            .chain([law.law.as_str()]);
        let Some(head) = path.next() else { continue };
        if !existing.contains(head) {
            add_scaffold(&mut missing, head, path);
        }
    }
    missing
}

fn add_scaffold<'a>(
    out: &mut BTreeMap<&'a str, Scaffold<'a>>,
    head: &'a str,
    mut path: impl Iterator<Item = &'a str>,
) {
    if let Some(next) = path.next() {
        if let Scaffold::Super(children) = out
            .entry(head)
            .or_insert_with(|| Scaffold::Super(BTreeMap::new()))
        {
            add_scaffold(children, next, path);
        }
    } else {
        out.insert(head, Scaffold::Law);
    }
}

fn law_record<'a>(path: &[&'a Node], text: &'a str) -> Option<(&'a Node, Vec<&'a str>)> {
    let last = path
        .iter()
        .rposition(|node| matches!(node.kind, K::RecordExpr | K::RecordUpdate))?;
    let names = path[..last]
        .iter()
        .filter_map(|node| field_name(node, text))
        .collect();
    Some((path[last], names))
}

fn field_name<'a>(node: &Node, text: &'a str) -> Option<&'a str> {
    (node.kind == K::RecordField)
        .then(|| {
            node.child_tokens()
                .find(|t| matches!(t.kind, K::Ident | K::Con))
                .map(|t| t.text(text))
        })
        .flatten()
}

fn qualifier_before(text: &str, dot: u32) -> Option<&str> {
    let before = text.get(..dot as usize)?;
    let qualifier = before
        .rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .next()?;
    (!qualifier.is_empty()).then_some(qualifier)
}

fn ranked(label: &str, kind: CompletionKind, detail: &str, sort: u8) -> Completion {
    let mut item = item(label, kind, Some(detail.into()));
    item.sort = sort;
    item
}

fn call(node: &Node, at: u32) -> Option<(&Node, usize)> {
    if node.kind != K::AppExpr {
        return None;
    }
    let mut nodes = node.nodes();
    let head = nodes.next()?;
    (head.range().1 < at).then(|| (head, nodes.take_while(|arg| arg.range().1 < at).count()))
}

fn contains((start, end): Range, at: u32) -> bool {
    start <= at && at <= end
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).expect("proof labels fit in u32")
}
