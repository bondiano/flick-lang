//! The proof declarations of a module, lowered apart from it: the sides of
//! each goal and the proof's body, with the parameters in scope. The
//! module's own IR has none of them; a proof runs nowhere.

use std::collections::HashSet;

use super::{Expr, ExprKind, Item, ItemKind, Lit, Lower, Module};
use crate::diagnostics::Diagnostic;
#[cfg(not(target_arch = "wasm32"))]
use crate::diagnostics::Severity;
use crate::logic::{self, Failure};
use crate::resolve;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::record as record_of;
use crate::types::{GoalSyntax, GoalType, ProofInfo, Range, Typing, goal_syntax};
#[cfg(not(target_arch = "wasm32"))]
use crate::types::{ProofEvidence, ProofStatus};
use flick_ir::{Dict, InstanceInfo};

mod bundles;
#[cfg(not(target_arch = "wasm32"))]
mod evidence;
#[cfg(not(target_arch = "wasm32"))]
mod lints;
#[cfg(test)]
mod replay;

/// One proof declaration, lowered.
#[derive(Debug, Clone)]
pub struct LoweredProof {
    /// The declaration, as `ProofInfo::at`.
    pub at: Range,
    pub span: Span,
    /// The unique names of the parameters and premises, in order.
    pub params: Vec<String>,
    /// Which of them are premises.
    pub premise: Vec<bool>,
    /// The sides of each premise's goal, then of the goal; a `same_type`
    /// goal has none.
    pub sides: Vec<Option<(Expr, Expr)>>,
    pub body: Option<Expr>,
    /// A bundle's: the dictionary its laws are about, and its lemmas.
    pub subject: Option<Dict>,
    pub fields: Vec<Field>,
}

/// A field of a bundle's record: the lemma `value` for the law `name` of
/// the class the superclass fields `path` lead to.
#[derive(Debug, Clone)]
pub struct Field {
    pub path: Vec<String>,
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

/// The proof declarations of `module`, which checked without errors.
pub fn lower_proofs(module: &resolve::Module, typing: &Typing) -> Vec<LoweredProof> {
    let mut lower = Lower::new(module, typing);
    module
        .tree
        .nodes()
        .filter(|n| n.kind == K::ProofDecl)
        .map(|decl| lower.proof(decl))
        .collect()
}

/// The laws of the classes of `module`, lowered as proofs without a body:
/// a method of the class is one of the given dictionary's.
pub fn lower_laws(module: &resolve::Module, typing: &Typing) -> Vec<LoweredProof> {
    let mut lower = Lower::new(module, typing);
    (module.tree.nodes())
        .filter(|n| n.kind == K::ClassDecl)
        .flat_map(|c| c.nodes().filter(|n| n.kind == K::Members))
        .flat_map(Node::nodes)
        .filter(|n| n.kind == K::LawDecl)
        .map(|decl| lower.proof(decl))
        .collect()
}

impl Lower<'_> {
    fn proof(&mut self, decl: &Node) -> LoweredProof {
        self.fresh_definition();
        let span = self.span(decl);
        let params: Vec<&Node> = decl
            .nodes()
            .filter(|n| n.kind == K::Params)
            .flat_map(Node::nodes)
            .collect();
        // a premise is a parameter by its name
        let patterns: Vec<&Node> = params
            .iter()
            .filter_map(|p| match p.kind {
                K::Premise => p.nodes().find(|n| n.kind == K::VarPat),
                _ => Some(*p),
            })
            .collect();
        let patterns_premise: Vec<bool> = params.iter().map(|p| p.kind == K::Premise).collect();
        let goals: Vec<&Node> = params
            .iter()
            .filter(|p| p.kind == K::Premise)
            .filter_map(|p| {
                p.nodes()
                    .find(|n| matches!(n.kind, K::Goal | K::SameTypeGoal))
            })
            .chain(
                decl.nodes()
                    .find(|n| matches!(n.kind, K::Goal | K::SameTypeGoal)),
            )
            .collect();
        let mut sides = Vec::new();
        let mut body = None;
        let mut fields = Vec::new();
        let lawful = decl.nodes().find(|n| n.kind == K::LawfulGoal);
        let lambda = self.function_with(
            &patterns,
            |l| {
                for goal in &goals {
                    sides.push(l.sides(goal));
                }
                let block = decl.nodes().find(|n| n.kind == K::Block);
                match (lawful, block.and_then(record_of)) {
                    (Some(_), Some(r)) => l.lemma_fields(r, &mut Vec::new(), &mut fields),
                    (Some(_), None) => {}
                    (None, _) => body = block.map(|b| l.expr(b)),
                }
                Expr::new(ExprKind::Lit(Lit::Unit), span)
            },
            None,
            span,
        );
        let params = match lambda.kind {
            ExprKind::Lambda { params, .. } => params
                .into_iter()
                .map(|p| match p.pat {
                    super::super::Pat::Bind(name) => name,
                    _ => String::new(),
                })
                .collect(),
            _ => Vec::new(),
        };
        LoweredProof {
            at: decl.range(),
            span,
            params,
            premise: patterns_premise,
            sides,
            body,
            subject: lawful.and_then(|g| self.dictionaries(g.range()).into_iter().next()),
            fields,
        }
    }

    /// The lemmas of a bundle's `record`, under the superclass fields `path`.
    fn lemma_fields(&mut self, record: &Node, path: &mut Vec<String>, out: &mut Vec<Field>) {
        for field in record.nodes().filter(|n| n.kind == K::RecordField) {
            let name = (field.child_tokens())
                .find(|t| matches!(t.kind, K::Ident | K::Con))
                .map(|t| t.text(self.text).to_owned());
            let (Some(name), Some(value)) = (name, field.nodes().last()) else {
                continue;
            };
            if let Some(inner) = record_of(value) {
                path.push(name);
                self.lemma_fields(inner, path, out);
                path.pop();
            } else {
                out.push(Field {
                    path: path.clone(),
                    name,
                    value: self.expr(value),
                    span: self.span(field),
                });
            }
        }
    }

    fn sides(&mut self, goal: &Node) -> Option<(Expr, Expr)> {
        match goal_syntax(goal, self.text) {
            GoalSyntax::Equal(l, r) => Some((self.expr(l), self.expr(r))),
            GoalSyntax::Holds(p) => Some((
                self.expr(p),
                Expr::new(ExprKind::Lit(Lit::Bool(true)), self.span(p)),
            )),
            GoalSyntax::SameType(..) | GoalSyntax::Other(_) => None,
        }
    }
}

/// A proof as an importer replays it: lowered, with what the checker
/// settled of it.
pub type Proven = (LoweredProof, ProofInfo);

/// What a proof or contract check reads: the built-in modules, the IR of the
/// modules of the run, the proofs of its proof modules, every contracted
/// function declared in the run, and the verified contract summaries of
/// modules checked before this one.
pub struct Run<'p> {
    pub builtins: &'p dyn Fn() -> &'static [Module],
    /// The laws of the built-in classes.
    pub builtin_laws: &'p dyn Fn() -> &'static [Proven],
    pub project: &'p dyn Fn(&str) -> Option<&'p Module>,
    pub proven: &'p dyn Fn(&str) -> Option<&'p [Proven]>,
    pub all_contracted: &'p HashSet<(String, String)>,
    pub contracts: &'p dyn Fn(&str) -> &'p [super::contracts::ProvenContract],
}

/// Each proof of `module`, which checked without errors, checked: what
/// fails to be a certificate of its goal, and the proofs, for importers.
pub fn check_proofs(
    module: &resolve::Module,
    typing: &Typing,
    run: &Run,
    #[cfg(not(target_arch = "wasm32"))] evidence: Option<&mut Vec<ProofEvidence>>,
    #[cfg(not(target_arch = "wasm32"))] unused_premises: bool,
) -> (Vec<Diagnostic>, Vec<Proven>) {
    if typing.proofs.is_empty() && typing.laws.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let laws = laws_of(module, typing);
    if typing.proofs.is_empty() {
        return (Vec::new(), laws);
    }
    let mut proofs = paired(lower_proofs(module, typing), &typing.proofs);
    // the definitions a proof unfolds: the module's own, then the imported
    let own = super::lower(module, typing).ok();
    let program = Modules {
        own: own.as_ref(),
        project: run.project,
        builtins: (run.builtins)(),
    };
    let mut lemmas = imported_lemmas(&proofs, run, 0);
    let of = |m: &str, c: &str| class_laws(run, module.name, &laws, m, c);
    #[cfg(not(target_arch = "wasm32"))]
    let mut evidence = evidence.map(|out| evidence::Recording::new(module, run, &mut lemmas, out));
    #[cfg(not(target_arch = "wasm32"))]
    let mut lints = lints::Lints::new(module, typing, unused_premises);
    let diagnostics = prove_each(
        &proofs,
        &program,
        &mut lemmas,
        Some(&of),
        #[cfg(not(target_arch = "wasm32"))]
        evidence.as_mut(),
        #[cfg(not(target_arch = "wasm32"))]
        Some(&mut lints),
    );
    proofs.extend(laws);
    (diagnostics, proofs)
}

/// Observes a module with syntax or typing errors for editors. Only complete,
/// independently checked certificates can contribute facts to later drafts.
#[cfg(not(target_arch = "wasm32"))]
pub fn observe_proofs(
    module: &resolve::Module,
    typing: &Typing,
    run: &Run,
    diagnostics: &[Diagnostic],
    out: &mut Vec<ProofEvidence>,
) {
    let proofs = paired(
        evidence::lower_drafts(module, typing, diagnostics),
        &typing.proofs,
    );
    let laws = laws_of(module, typing);
    let of = |m: &str, c: &str| class_laws(run, module.name, &laws, m, c);
    // Proof declarations have no runtime IR. Ordinary definitions still
    // provide an exact subject if every error belongs to a proof body.
    let ordinary_checked = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .all(|diagnostic| {
            diagnostic.span.is_some_and(|span| {
                proofs.iter().any(|(proof, _)| {
                    span.file == proof.span.file
                        && proof.span.start <= span.start
                        && span.end <= proof.span.end
                })
            })
        });
    let own = ordinary_checked
        .then(|| super::lower(module, typing).ok())
        .flatten();
    let program = Modules {
        own: own.as_ref(),
        project: run.project,
        builtins: (run.builtins)(),
    };
    let mut lemmas = imported_lemmas(&proofs, run, 0);
    let recording = evidence::Recording::new(module, run, &mut lemmas, out);
    for proof in &proofs {
        let (lowered, info) = proof;
        let mut evidence = recording.begin(lowered, &lemmas);
        let error = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .find_map(|d| {
                d.span.filter(|span| {
                    span.file == lowered.span.file
                        && lowered.span.start <= span.start
                        && span.start <= lowered.span.end
                })
            });
        if let GoalType::Lawful(bundle) = &info.goal {
            bundles::observe_bundle(
                proof,
                bundle,
                &of,
                &program,
                &lemmas,
                (&mut evidence, &recording.aliases),
            );
        } else if let Some(span) = error {
            evidence.failure = Some(span);
            evidence::observe_body(
                lowered,
                info,
                &program,
                &lemmas,
                &recording.aliases,
                &mut evidence,
            );
        } else {
            let checked = verify(
                lowered,
                info,
                &program,
                &lemmas,
                Some((&mut evidence, &recording.aliases)),
            );
            save_lemma(checked, lowered, &mut lemmas);
        }
        recording.out.push(evidence);
    }
}

fn class_laws<'p>(
    run: &'p Run<'_>,
    own: &str,
    own_laws: &'p [Proven],
    module: &str,
    class: &str,
) -> Option<Vec<&'p Proven>> {
    let found = (module == own)
        .then_some(own_laws)
        .or_else(|| (run.proven)(module))
        .or_else(|| {
            let builtin = (run.builtins)().iter().any(|b| b.name == module);
            builtin.then(|| (run.builtin_laws)())
        });
    Some(
        found?
            .iter()
            .filter(|p| {
                p.1.class
                    .as_ref()
                    .is_some_and(|l| l.module == module && l.class == class)
            })
            .collect(),
    )
}

/// The laws of the classes of `module`, which checked without errors.
pub fn laws_of(module: &resolve::Module, typing: &Typing) -> Vec<Proven> {
    paired(lower_laws(module, typing), &typing.laws)
}

/// Each lowered declaration with what the checker settled of it.
fn paired(lowered: Vec<LoweredProof>, infos: &[ProofInfo]) -> Vec<Proven> {
    (lowered.into_iter())
        .filter_map(|l| {
            let info = infos.iter().find(|i| i.at == l.at)?.clone();
            Some((l, info))
        })
        .collect()
}

/// Checks each proof in order; one that checks is a lemma of the later ones.
fn prove_each(
    proofs: &[Proven],
    program: &dyn logic::Program,
    lemmas: &mut Vec<logic::Lemma>,
    laws: Option<bundles::Laws<'_, '_>>,
    #[cfg(not(target_arch = "wasm32"))] mut recording: Option<&mut evidence::Recording<'_>>,
    #[cfg(not(target_arch = "wasm32"))] mut lints: Option<&mut lints::Lints<'_>>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for proven in proofs {
        let (lowered, info) = proven;
        if info.class.is_some() {
            continue;
        }
        #[cfg(not(target_arch = "wasm32"))]
        let mut evidence = recording.as_ref().map(|r| r.begin(lowered, lemmas));
        #[cfg(not(target_arch = "wasm32"))]
        let candidate_scope = evidence
            .as_mut()
            .zip(recording.as_ref().map(|r| &r.aliases));
        if let GoalType::Lawful(bundle) = &info.goal {
            // A bundle is no lemma: an importer has nothing to replay of it.
            if let Some(laws) = laws {
                let found = bundles::check_bundle(
                    proven,
                    bundle,
                    laws,
                    program,
                    lemmas,
                    #[cfg(not(target_arch = "wasm32"))]
                    candidate_scope,
                );
                #[cfg(not(target_arch = "wasm32"))]
                if found.is_empty()
                    && let Some(lints) = &mut lints
                {
                    lints.premises(lowered.at, &mut out);
                }
                out.extend(found);
            }
        } else {
            let checked = verify(
                lowered,
                info,
                program,
                lemmas,
                #[cfg(not(target_arch = "wasm32"))]
                candidate_scope,
            );
            #[cfg(not(target_arch = "wasm32"))]
            if let (Some(lints), Ok(verified)) = (&mut lints, &checked) {
                lints.premises(lowered.at, &mut out);
                lints.steps(&verified.certificate, &mut out);
            }
            if let Some(failure) = save_lemma(checked, lowered, lemmas) {
                out.push(
                    failure
                        .with_related(lowered.span, format!("proof `{}`", info.name))
                        .diagnostic(&info.name),
                );
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((recording, evidence)) = recording.as_mut().zip(evidence) {
            recording.out.push(evidence);
        }
    }
    out
}

struct Verified {
    statement: logic::Statement,
    #[cfg(not(target_arch = "wasm32"))]
    certificate: logic::Cert,
}

fn verify(
    lowered: &LoweredProof,
    info: &ProofInfo,
    program: &dyn logic::Program,
    lemmas: &[logic::Lemma],
    #[cfg(not(target_arch = "wasm32"))] mut evidence: Option<(
        &mut ProofEvidence,
        &logic::LemmaAliases,
    )>,
) -> Result<Verified, Failure> {
    let checked = statement(lowered, info).and_then(|statement| {
        let body = lowered.body.as_ref().ok_or_else(|| {
            Failure::Refused(logic::Refused {
                span: lowered.span,
                why: "it has no proof".into(),
            })
        })?;
        let cert = logic::cert(body, lemmas)?;
        #[cfg(not(target_arch = "wasm32"))]
        match &mut evidence {
            Some((e, aliases)) => {
                logic::prove_with_suggestions(
                    &statement,
                    &cert,
                    &info.data,
                    program,
                    lemmas,
                    aliases,
                    &mut e.steps,
                )?;
            }
            None => logic::prove(&statement, &cert, &info.data, program, lemmas)?,
        }
        #[cfg(target_arch = "wasm32")]
        logic::prove(&statement, &cert, &info.data, program, lemmas)?;
        Ok(Verified {
            statement,
            #[cfg(not(target_arch = "wasm32"))]
            certificate: cert,
        })
    });
    #[cfg(not(target_arch = "wasm32"))]
    if let Some((evidence, aliases)) = evidence {
        match &checked {
            Ok(_) => {
                evidence.status = ProofStatus::Checked;
                if let Some(root) = evidence.steps.first() {
                    evidence.header_suggestions.clone_from(&root.suggestions);
                }
            }
            Err(failure) => {
                evidence.status = ProofStatus::of_failure(failure);
                evidence.failure = failure.diagnostic(&info.name).span;
                evidence::observe_body(lowered, info, program, lemmas, aliases, evidence);
            }
        }
    }
    checked
}

fn save_lemma(
    checked: Result<Verified, Failure>,
    lowered: &LoweredProof,
    lemmas: &mut Vec<logic::Lemma>,
) -> Option<Failure> {
    match checked {
        Ok(Verified { statement, .. }) => {
            lemmas.push(logic::Lemma {
                statement,
                order: (lowered.params.iter())
                    .zip(&lowered.premise)
                    .map(|(x, p)| (x.as_str().into(), *p))
                    .collect(),
            });
            None
        }
        Err(failure) => Some(failure),
    }
}

/// The lemmas `proofs` name of other proof modules, each replayed against
/// the IR of its module: no success is taken on trust, and one that does not
/// check again is no lemma.
fn imported_lemmas(proofs: &[Proven], run: &Run, depth: u32) -> Vec<logic::Lemma> {
    let mut modules = Vec::new();
    for (lowered, _) in proofs {
        let fields = lowered.fields.iter().map(|f| &f.value);
        (lowered.body.iter().chain(fields)).for_each(|b| lemma_modules(b, &mut modules));
    }
    modules.sort();
    modules.dedup();
    replay_lemmas(&modules, run, depth)
}

fn replay_lemmas(modules: &[String], run: &Run, depth: u32) -> Vec<logic::Lemma> {
    let mut out = Vec::new();
    // imports are acyclic; the bound only keeps a malformed graph finite
    if depth > 16 {
        return out;
    }
    for name in modules {
        let (Some(proofs), Some(ir)) = ((run.proven)(name), (run.project)(name)) else {
            continue;
        };
        let program = Modules {
            own: Some(ir),
            project: run.project,
            builtins: (run.builtins)(),
        };
        let mut lemmas = imported_lemmas(proofs, run, depth + 1);
        let imported = lemmas.len();
        // the module's own diagnostics are its own check's to report
        let _ = prove_each(
            proofs,
            &program,
            &mut lemmas,
            None,
            #[cfg(not(target_arch = "wasm32"))]
            None,
            #[cfg(not(target_arch = "wasm32"))]
            None,
        );
        out.extend(lemmas.drain(imported..).map(|mut l| {
            l.statement.name = format!("{name}.{}", l.statement.name);
            l
        }));
    }
    out
}

/// The modules of the lemmas `e` names as `M.name`.
fn lemma_modules(e: &Expr, out: &mut Vec<String>) {
    if let ExprKind::Global { module, name, .. } = &e.kind
        && module == logic::LEMMAS
        && let Some((m, _)) = name.rsplit_once('.')
    {
        out.push(m.to_owned());
    }
    e.each_child(&mut |c| lemma_modules(c, out));
}

/// What `lowered` states.
fn statement(lowered: &LoweredProof, info: &ProofInfo) -> Result<logic::Statement, Failure> {
    let refused = |span, why: &str| {
        Failure::Refused(logic::Refused {
            span,
            why: why.into(),
        })
    };
    let equation =
        |ty: &logic::Ty, sides: &Option<(Expr, Expr)>| -> Result<logic::Equation, Failure> {
            let Some((l, r)) = sides else {
                return Err(refused(
                    lowered.span,
                    "a premise is an equation of values, `equal l r` or `holds p`",
                ));
            };
            Ok(logic::Equation {
                ty: ty.clone(),
                left: logic::term(l).map_err(Failure::Refused)?,
                right: logic::term(r).map_err(Failure::Refused)?,
            })
        };
    let mut params = Vec::new();
    let mut premises = Vec::new();
    let mut sides = lowered.sides.iter();
    for ((name, premise), ty) in lowered
        .params
        .iter()
        .zip(&lowered.premise)
        .zip(&info.params)
    {
        if *premise {
            let side = sides
                .next()
                .ok_or_else(|| refused(lowered.span, "a premise has no goal"))?;
            premises.push((name.as_str().into(), equation(ty, side)?));
        } else {
            params.push((name.as_str().into(), ty.clone()));
        }
    }
    let goal = match &info.goal {
        GoalType::Equal(ty) => logic::Goal::Equal(equation(ty, sides.next().unwrap_or(&None))?),
        GoalType::SameType(a, b) => logic::Goal::SameType(a.clone(), b.clone()),
        GoalType::Outside(why) => return Err(refused(lowered.span, why)),
        GoalType::Lawful(_) | GoalType::Malformed => {
            return Err(refused(lowered.span, "its goal is no statement"));
        }
    };
    let statement = logic::Statement {
        name: info.name.clone(),
        span: lowered.span,
        params,
        premises,
        goal,
    };
    Ok(statement)
}

/// The compiled modules a proof or contract reads definitions from.
pub(super) struct Modules<'a, 'p: 'a> {
    pub(super) own: Option<&'a Module>,
    /// The modules of the run checked before this one.
    pub(super) project: &'a dyn Fn(&str) -> Option<&'p Module>,
    pub(super) builtins: &'static [Module],
}

impl<'a> Modules<'a, '_> {
    fn module(&self, name: &str) -> Option<&'a Module> {
        (self.own.filter(|m| m.name == name))
            .or_else(|| (self.project)(name))
            .or_else(|| self.builtins.iter().find(|m| m.name == name))
    }
}

impl logic::Program for Modules<'_, '_> {
    fn item(&self, module: &str, name: &str) -> Option<&Item> {
        self.module(module)?.items.iter().find(|i| match &i.kind {
            ItemKind::Value { name: n, .. } => n == name,
            ItemKind::Foreign(f) => f.name == name,
            _ => false,
        })
    }

    fn instance(&self, module: &str, name: &str) -> Option<(&InstanceInfo, &[(String, Expr)])> {
        self.module(module)?
            .items
            .iter()
            .find_map(|i| match &i.kind {
                ItemKind::Instance { info, methods }
                    if info.module == module && info.name == name =>
                {
                    Some((&**info, methods.as_slice()))
                }
                _ => None,
            })
    }
}
