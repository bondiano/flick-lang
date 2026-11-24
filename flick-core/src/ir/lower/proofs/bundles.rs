//! A bundle checked: each law of its class and of each superclass, at the
//! bundle's dictionary and types, is proved by the lemma its record names
//! for it. The lemma is applied to the law's parameters, so it must be
//! about exactly the definitions the dictionary has: a lemma of another
//! instance, or of a local dictionary, proves nothing of it.

use std::rc::Rc;
#[cfg(not(target_arch = "wasm32"))]
use std::slice;

use flick_ir::{Dict, Expr, ExprKind};

use super::{Field, Proven, statement};
use crate::diagnostics::Diagnostic;
use crate::logic::{self, Arg, Cert, Failure, Lemma, Rule, Statement, Term, Ty};
#[cfg(not(target_arch = "wasm32"))]
use crate::logic::{
    CandidateVerdict, LemmaAliases, ProofSearchRequest, ProofSearchResult, ProofSearchStatus,
    SEARCH_CANDIDATES, SEARCH_NORMALIZATION, SEARCH_PROBE_STEPS,
};
use crate::source::Span;
use crate::types::{Bundle, GoalType};
#[cfg(not(target_arch = "wasm32"))]
use crate::types::{LawObligation, ProofEvidence, ProofStatus};

/// The laws of a class, by its module and name; `None` where its module's
/// laws are not available to the run.
pub type Laws<'a, 'p> = &'a dyn Fn(&str, &str) -> Option<Vec<&'p Proven>>;

#[cfg(not(target_arch = "wasm32"))]
const MAX_CANDIDATES: usize = 192;
#[cfg(not(target_arch = "wasm32"))]
const MAX_NORMALIZATION: u64 = 12_000;
#[cfg(not(target_arch = "wasm32"))]
const LEMMAS_PER_FIELD: usize = 64;
#[cfg(not(target_arch = "wasm32"))]
const CHECKED_PER_FIELD: usize = 8;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy)]
struct CandidateBudget {
    remaining: usize,
    normalization: u64,
    exhausted: bool,
}

#[cfg(not(target_arch = "wasm32"))]
struct SelectedSearch {
    request: ProofSearchRequest,
    result: Option<ProofSearchResult>,
}

#[cfg(not(target_arch = "wasm32"))]
fn empty_search(request: &ProofSearchRequest) -> ProofSearchResult {
    ProofSearchResult::empty(if request.is_cancelled() {
        ProofSearchStatus::Cancelled
    } else {
        ProofSearchStatus::NoCandidate
    })
}

fn invalid(span: Span, why: String) -> Failure {
    Failure::Invalid { span, why }
}

/// What fails of the bundle `lowered` states, as `bundle` types it.
pub(super) fn check_bundle(
    (lowered, info): &Proven,
    bundle: &Bundle,
    laws: Laws<'_, '_>,
    program: &dyn logic::Program,
    lemmas: &[logic::Lemma],
    #[cfg(not(target_arch = "wasm32"))] mut evidence: Option<(&mut ProofEvidence, &LemmaAliases)>,
    #[cfg(not(target_arch = "wasm32"))] search: Option<&ProofSearchRequest>,
) -> Vec<Diagnostic> {
    let Some(subject) = &lowered.subject else {
        let why = "its constraint resolved to no dictionary".into();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((evidence, _)) = evidence {
            evidence.status = ProofStatus::Blocked;
            evidence.failure = Some(lowered.span);
            if let Some(request) = search {
                evidence.search = Some(empty_search(request));
            }
        }
        return vec![invalid(lowered.span, why).diagnostic(&info.name)];
    };
    #[cfg(not(target_arch = "wasm32"))]
    let (obligations, aliases) = evidence
        .as_mut()
        .map_or((None, None), |(e, a)| (Some(&mut e.obligations), Some(*a)));
    let mut checking = Checking {
        fields: &lowered.fields,
        used: vec![false; lowered.fields.len()],
        span: lowered.span,
        data: &info.data,
        laws,
        program,
        lemmas,
        out: Vec::new(),
        #[cfg(not(target_arch = "wasm32"))]
        obligations,
        #[cfg(not(target_arch = "wasm32"))]
        aliases,
        #[cfg(not(target_arch = "wasm32"))]
        params: &lowered.params,
        #[cfg(not(target_arch = "wasm32"))]
        budget: CandidateBudget {
            remaining: MAX_CANDIDATES,
            normalization: MAX_NORMALIZATION,
            exhausted: false,
        },
        #[cfg(not(target_arch = "wasm32"))]
        search: search.cloned().map(|request| SelectedSearch {
            request,
            result: None,
        }),
    };
    checking.class(bundle, subject, &mut Vec::new());
    let Checking {
        used,
        mut out,
        #[cfg(not(target_arch = "wasm32"))]
        search,
        ..
    } = checking;
    for (field, _) in lowered.fields.iter().zip(used).filter(|(_, used)| !used) {
        let why = format!("`{}` is no law of the classes of the bundle", field.name);
        out.push(invalid(field.span, why));
    }
    let diagnostics: Vec<_> = out.iter().map(|f| f.diagnostic(&info.name)).collect();
    #[cfg(not(target_arch = "wasm32"))]
    if let Some((evidence, _)) = evidence {
        evidence.status = out
            .first()
            .map_or(ProofStatus::Checked, ProofStatus::of_failure);
        evidence.failure = diagnostics.first().and_then(|d| d.span);
        if let Some(s) = search {
            evidence.search = Some(s.result.unwrap_or_else(|| empty_search(&s.request)));
        }
    }
    diagnostics
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn observe_bundle(
    proof: &Proven,
    bundle: &Bundle,
    laws: Laws<'_, '_>,
    program: &dyn logic::Program,
    lemmas: &[logic::Lemma],
    (evidence, aliases): (&mut ProofEvidence, &LemmaAliases),
    search: Option<&ProofSearchRequest>,
) {
    let _ = check_bundle(
        proof,
        bundle,
        laws,
        program,
        lemmas,
        Some((evidence, aliases)),
        search,
    );
    if evidence.status == ProofStatus::Checked {
        evidence.status = ProofStatus::Open;
    }
}

struct Checking<'a, 'p> {
    fields: &'a [Field],
    used: Vec<bool>,
    span: Span,
    data: &'a logic::Data,
    laws: Laws<'a, 'p>,
    program: &'a dyn logic::Program,
    lemmas: &'a [logic::Lemma],
    out: Vec<Failure>,
    #[cfg(not(target_arch = "wasm32"))]
    obligations: Option<&'a mut Vec<LawObligation>>,
    #[cfg(not(target_arch = "wasm32"))]
    aliases: Option<&'a LemmaAliases>,
    #[cfg(not(target_arch = "wasm32"))]
    params: &'a [String],
    #[cfg(not(target_arch = "wasm32"))]
    budget: CandidateBudget,
    #[cfg(not(target_arch = "wasm32"))]
    search: Option<SelectedSearch>,
}

impl Checking<'_, '_> {
    /// The laws of `bundle`'s class at `dict`, under the superclass fields
    /// `path`, then those of its superclasses.
    fn class(&mut self, bundle: &Bundle, dict: &Dict, path: &mut Vec<String>) {
        let Some(laws) = (self.laws)(&bundle.module, &bundle.class) else {
            // a class with laws unread would be certified of none
            self.out.push(Failure::Refused(logic::Refused {
                span: self.span,
                why: format!(
                    "the laws of `{}` are not available to this proof",
                    bundle.class
                ),
            }));
            return;
        };
        for law in laws {
            self.obligation(law, bundle, dict, path);
        }
        let slots: Vec<&String> = bundle.supers.iter().map(|(s, _)| s).collect();
        let shared = (slots.iter().enumerate()).find(|(i, s)| slots[..*i].contains(s));
        if let Some((_, slot)) = shared {
            // a field names no one superclass, nor its dictionary one instance
            self.out.push(Failure::Refused(logic::Refused {
                span: self.span,
                why: format!(
                    "two superclasses of `{}` share the dictionary field `{slot}`",
                    bundle.class
                ),
            }));
            // the fields under it are refused with it, not reported again
            for (field, used) in self.fields.iter().zip(&mut self.used) {
                *used |= field.path.len() > path.len() && field.path.starts_with(path);
            }
            return;
        }
        for (slot, inner) in &bundle.supers {
            let dict = Dict::Super {
                dict: Box::new(dict.clone()),
                class: slot.clone(),
            };
            path.push(slot.clone());
            self.class(inner, &dict, path);
            path.pop();
        }
    }

    fn obligation(&mut self, law: &Proven, bundle: &Bundle, dict: &Dict, path: &[String]) {
        let name = &law.1.name;
        let field = self
            .fields
            .iter()
            .position(|f| f.path == path && f.name == *name);
        let field = field.map(|i| {
            self.used[i] = true;
            &self.fields[i]
        });
        #[cfg(not(target_arch = "wasm32"))]
        let mut shown = None;
        let result = if let Some(field) = field {
            self.law(
                law,
                bundle,
                dict,
                field,
                #[cfg(not(target_arch = "wasm32"))]
                &mut shown,
            )
        } else {
            #[cfg(not(target_arch = "wasm32"))]
            if self.obligations.is_some() {
                shown = self
                    .law_statement(law, bundle, dict)
                    .ok()
                    .map(|s| logic::show_goal(&s.goal));
            }
            Err(invalid(
                self.span,
                format!(
                    "the bundle has no lemma for the law `{name}` of `{}`",
                    bundle.class
                ),
            ))
        };
        #[cfg(not(target_arch = "wasm32"))]
        let suggestions = {
            let mut budget = self.budget;
            let suggestions = self.candidates(law, bundle, dict, &mut budget, None);
            self.budget = budget;
            if let Some(field) = field {
                self.search_field(law, bundle, dict, field);
            }
            suggestions
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(obligations) = &mut self.obligations {
            obligations.push(LawObligation {
                path: path.to_vec(),
                class: bundle.class.clone(),
                class_module: bundle.module.clone(),
                law: name.clone(),
                status: result
                    .as_ref()
                    .err()
                    .map_or(ProofStatus::Checked, ProofStatus::of_failure),
                statement: shown,
                range: field.map(|f| (f.span.start, f.span.end)),
                suggestions,
            });
        }
        if let Err(failure) = result {
            let failure = match field {
                Some(field) => {
                    failure.with_related(field.span, format!("law `{name}` of `{}`", bundle.class))
                }
                None => failure,
            };
            self.out.push(failure);
        }
    }

    fn law_statement(
        &self,
        (law, info): &Proven,
        bundle: &Bundle,
        dict: &Dict,
    ) -> Result<logic::Statement, Failure> {
        let params = info.class.as_ref().map_or(&[][..], |c| &c.params[..]);
        let mut at = law.clone();
        for (l, r) in at.sides.iter_mut().flatten() {
            self.given(l, dict);
            self.given(r, dict);
        }
        // the law's own type variables apart from the bundle's, so that the
        // class's parameters filled in capture none of them
        let at_bundle = |t: &Ty| apart(t, params).instantiate(params, &bundle.args);
        let mut stated = info.clone();
        stated.params = info.params.iter().map(at_bundle).collect();
        if let GoalType::Equal(t) = &info.goal {
            stated.goal = GoalType::Equal(at_bundle(t));
        }
        statement(&at, &stated)
    }

    /// The law of `bundle`'s class at `dict`, by the lemma of `field`.
    fn law(
        &self,
        proved: &Proven,
        bundle: &Bundle,
        dict: &Dict,
        field: &Field,
        #[cfg(not(target_arch = "wasm32"))] shown: &mut Option<String>,
    ) -> Result<(), Failure> {
        let statement = self.law_statement(proved, bundle, dict)?;
        #[cfg(not(target_arch = "wasm32"))]
        if self.obligations.is_some() {
            *shown = Some(logic::show_goal(&statement.goal));
        }
        let ExprKind::Global { module, name, .. } = &field.value.kind else {
            return Err(invalid(
                field.span,
                "a law's field names the proof of it".into(),
            ));
        };
        let lemma =
            (self.lemmas.iter()).find(|l| module == logic::LEMMAS && l.statement.name == *name);
        let Some(lemma) = lemma else {
            let why = format!("`{name}` is no proof checked before this one");
            return Err(invalid(field.span, why));
        };
        let cert = law_certificate(proved, &statement, lemma, field.span, field.value.span)?;
        let data = self.law_data(proved);
        logic::prove(&statement, &cert, &data, self.program, self.lemmas)
    }

    fn law_data(&self, (_, info): &Proven) -> logic::Data {
        let mut data = info.data.clone();
        data.extend(self.data);
        data
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn candidates(
        &self,
        proved: &Proven,
        bundle: &Bundle,
        dict: &Dict,
        budget: &mut CandidateBudget,
        search: Option<&ProofSearchRequest>,
    ) -> Vec<String> {
        let Some(aliases) = self.aliases else {
            return Vec::new();
        };
        if budget.remaining == 0
            || budget.normalization == 0
            || search.is_some_and(ProofSearchRequest::is_cancelled)
        {
            return Vec::new();
        }
        let Ok(statement) = self.law_statement(proved, bundle, dict) else {
            return Vec::new();
        };
        let data = self.law_data(proved);
        let mut found = Vec::new();
        let scanned = if search.is_some() {
            SEARCH_CANDIDATES
        } else {
            LEMMAS_PER_FIELD
        };
        for lemma in self.lemmas.iter().rev().take(scanned) {
            if budget.remaining == 0
                || budget.normalization == 0
                || found.len() >= CHECKED_PER_FIELD
                || search.is_some_and(ProofSearchRequest::is_cancelled)
            {
                break;
            }
            let name = &lemma.statement.name;
            let names = if let Some(names) = aliases.get(name) {
                names.as_slice()
            } else if name.contains('.')
                || matches!(name.as_str(), "refl" | "symm" | "trans" | "cong" | "funext")
                || self
                    .params
                    .iter()
                    .any(|param| param.split('$').next() == Some(name.as_str()))
            {
                continue;
            } else {
                slice::from_ref(name)
            };
            budget.remaining -= 1;
            let Ok(cert) = law_certificate(proved, &statement, lemma, self.span, self.span) else {
                continue;
            };
            let before = if search.is_some() {
                budget.normalization.min(SEARCH_PROBE_STEPS)
            } else {
                budget.normalization
            };
            let mut remaining = before;
            let checked = logic::check_candidate(
                &statement,
                &cert,
                &data,
                self.program,
                self.lemmas,
                &mut remaining,
            );
            budget.normalization -= before - remaining;
            budget.exhausted |= checked == CandidateVerdict::Exhausted;
            if checked == CandidateVerdict::Accepted {
                found.extend(names.iter().take(CHECKED_PER_FIELD - found.len()).cloned());
            }
        }
        found
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn search_field(&mut self, proved: &Proven, bundle: &Bundle, dict: &Dict, field: &Field) {
        let Some(search) = &self.search else {
            return;
        };
        let request = &search.request;
        if search.result.is_some()
            || request.offset < field.span.start
            || field.span.end < request.offset
        {
            return;
        }
        let mut budget = CandidateBudget {
            remaining: SEARCH_CANDIDATES,
            normalization: SEARCH_NORMALIZATION,
            exhausted: false,
        };
        let mut suggestions = self.candidates(proved, bundle, dict, &mut budget, Some(request));
        let status = if request.is_cancelled() {
            suggestions.clear();
            ProofSearchStatus::Cancelled
        } else if !suggestions.is_empty() {
            ProofSearchStatus::Found
        } else if budget.remaining == 0 || budget.normalization == 0 || budget.exhausted {
            ProofSearchStatus::BudgetExhausted
        } else {
            ProofSearchStatus::NoCandidate
        };
        if let Some(search) = &mut self.search {
            search.result = Some(ProofSearchResult {
                status,
                suggestions,
                candidates: SEARCH_CANDIDATES - budget.remaining,
                normalization_steps: SEARCH_NORMALIZATION - budget.normalization,
                span: Some(field.value.span),
            });
        }
    }

    /// `e` with the law's given dictionary `dict`: each superclass of it the
    /// instance it is, where the instance says.
    fn given(&self, e: &mut Expr, dict: &Dict) {
        if let ExprKind::Method { dict: d, .. } = &mut e.kind {
            *d = self.canonical(at(d, dict));
        }
        e.each_child_mut(&mut |c| self.given(c, dict));
    }

    fn canonical(&self, d: Dict) -> Dict {
        let Dict::Super { dict, class } = d else {
            return d;
        };
        let inner = self.canonical(*dict);
        if let Dict::Instance { module, name, args } = &inner
            && args.is_empty()
            && let Some((info, _)) = self.program.instance(module, name)
            && let mut found = info.supers.iter().filter(|(c, _)| *c == class)
            && let (Some((_, own)), None) = (found.next(), found.next())
        {
            return self.canonical(own.clone());
        }
        Dict::Super {
            dict: Box::new(inner),
            class,
        }
    }
}

/// Applies a lemma to the law's parameters, keeping its printed subject
/// and its dictionary identities for the kernel to compare.
fn law_certificate(
    proved: &Proven,
    statement: &Statement,
    lemma: &Lemma,
    field: Span,
    span: Span,
) -> Result<Cert, Failure> {
    let (law, info) = proved;
    let name = &lemma.statement.name;
    // The parameters, premises and equation agree up to a renaming shared by all three.
    let mut renaming = Vec::new();
    let (our_count, ours) = equation_types(statement);
    let (their_count, theirs) = equation_types(&lemma.statement);
    let fits = lemma
        .order
        .iter()
        .map(|(_, p)| *p)
        .eq(law.premise.iter().copied())
        && statement.params.len() == lemma.statement.params.len()
        && statement
            .params
            .iter()
            .zip(&lemma.statement.params)
            .all(|((_, ours), (_, theirs))| renames(theirs, ours, &mut renaming))
        && our_count == their_count
        && ours.zip(theirs).all(|(a, b)| renames(b, a, &mut renaming));
    if !fits {
        let why = format!(
            "`{name}` is not stated over the parameters of the law `{}`",
            info.name
        );
        return Err(invalid(field, why));
    }
    let args = (law.params.iter())
        .zip(&law.premise)
        .map(|(x, premise)| {
            let x: Rc<str> = x.as_str().into();
            if *premise {
                Arg::Proof(Cert {
                    kind: Rule::Fact(x),
                    span,
                })
            } else {
                Arg::Value(Term::Var(x))
            }
        })
        .collect();
    Ok(Cert {
        kind: Rule::Lemma(name.as_str().into(), args),
        span,
    })
}

fn equation_types(statement: &Statement) -> (usize, impl Iterator<Item = &Ty>) {
    let goal = match &statement.goal {
        logic::Goal::Equal(e) => Some(&e.ty),
        logic::Goal::SameType(..) => None,
    };
    let count = statement.premises.len() + usize::from(goal.is_some());
    let types = statement.premises.iter().map(|(_, e)| &e.ty).chain(goal);
    (count, types)
}

/// `d` with the given dictionary it stands on replaced by `dict`.
fn at(d: &Dict, dict: &Dict) -> Dict {
    match d {
        Dict::Given(_) => dict.clone(),
        Dict::Super { dict: inner, class } => Dict::Super {
            dict: Box::new(at(inner, dict)),
            class: class.clone(),
        },
        other => other.clone(),
    }
}

/// `t` with each rigid variable but the class's parameters `keep` renamed
/// apart from any of a bundle's types.
fn apart(t: &Ty, keep: &[String]) -> Ty {
    match t {
        Ty::Rigid(x) if !keep.contains(x) => Ty::Rigid(format!("{x}$law")),
        Ty::Data { module, name, args } => Ty::Data {
            module: module.clone(),
            name: name.clone(),
            args: args.iter().map(|a| apart(a, keep)).collect(),
        },
        Ty::Fun(a, b) => Ty::Fun(Box::new(apart(a, keep)), Box::new(apart(b, keep))),
        Ty::Rigid(_) | Ty::Bool | Ty::Int | Ty::Refused(_) => t.clone(),
    }
}

/// Whether `ours` is `theirs` with its rigid variables renamed one to one.
fn renames<'a>(ours: &'a Ty, theirs: &'a Ty, renaming: &mut Vec<(&'a str, &'a str)>) -> bool {
    match (ours, theirs) {
        (Ty::Rigid(mine), Ty::Rigid(other)) => {
            if let Some((was, is)) =
                (renaming.iter()).find(|(was, is)| *was == mine || *is == other)
            {
                return *was == mine && *is == other;
            }
            renaming.push((mine, other));
            true
        }
        (
            Ty::Data { module, name, args },
            Ty::Data {
                module: other_module,
                name: other_name,
                args: other_args,
            },
        ) => {
            module == other_module
                && name == other_name
                && args.len() == other_args.len()
                && (args.iter().zip(other_args)).all(|(mine, other)| renames(mine, other, renaming))
        }
        (Ty::Fun(param, result), Ty::Fun(other_param, other_result)) => {
            renames(param, other_param, renaming) && renames(result, other_result, renaming)
        }
        (Ty::Bool, Ty::Bool) | (Ty::Int, Ty::Int) => true,
        _ => false,
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use super::{ProofSearchRequest, ProofSearchStatus};
    use crate::ir::{Run, check_proofs};
    use crate::project::builtins::{builtin_laws, builtin_modules_of};
    use crate::types;

    #[test]
    fn explicit_search_checks_one_inherited_law_of_the_selected_instance() {
        let source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../packages/core/proofs/Laws/Option.fl"
        ));
        let mut sources = types::builtin_sources().to_vec();
        sources.push(("Laws.Option", source));
        let run = Run {
            builtins: &|| builtin_modules_of(false),
            builtin_laws: &|| builtin_laws(false),
            project: &|_| None,
            proven: &|_| None,
            all_contracted: &HashSet::new(),
            contracts: &|_| &[],
        };
        let at = source
            .find("identity = map_identity")
            .expect("Functor identity field")
            + "identity = ".len();
        let mut found = None;
        types::check_each(&sources, false, |module, typing| {
            if module.name != "Laws.Option" {
                return;
            }
            let proof = typing
                .proofs
                .iter()
                .find(|p| p.name == "option")
                .expect("bundle");
            let request = ProofSearchRequest {
                declaration: proof.at,
                offset: u32::try_from(at).expect("source length fits in u32"),
                cancelled: Arc::new(AtomicBool::new(false)),
            };
            let mut evidence = Vec::new();
            let (diagnostics, _) = check_proofs(
                module,
                typing,
                &run,
                Some(&mut evidence),
                false,
                Some(&request),
            );
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
            found = evidence.into_iter().find_map(|proof| proof.search);
        });
        let found = found.expect("selected law result");
        assert_eq!(found.status, ProofSearchStatus::Found);
        assert_eq!(found.suggestions, ["map_identity"]);
        let range = found.span.expect("the selected lemma expression").range();
        assert_eq!(&source[range], "map_identity");
    }
}
