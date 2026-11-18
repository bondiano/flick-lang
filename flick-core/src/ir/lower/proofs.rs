//! The proof declarations of a module, lowered apart from it: the sides of
//! each goal and the proof's body, with the parameters in scope. The
//! module's own IR has none of them; a proof runs nowhere.

use super::{Expr, ExprKind, Item, ItemKind, Lit, Lower, Module};
use crate::diagnostics::Diagnostic;
use crate::logic::{self, Failure};
use crate::resolve;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{GoalSyntax, GoalType, ProofInfo, Range, Typing, goal_syntax};
use flick_ir::InstanceInfo;

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
        let lambda = self.function_with(
            &patterns,
            |l| {
                for goal in &goals {
                    sides.push(l.sides(goal));
                }
                body = decl.nodes().find(|n| n.kind == K::Block).map(|b| l.expr(b));
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

/// Each proof of `module`, which checked without errors, checked: what
/// fails to be a certificate of its goal.
pub fn check_proofs(
    module: &resolve::Module,
    typing: &Typing,
    imported: &dyn Fn() -> &'static [Module],
) -> Vec<Diagnostic> {
    if typing.proofs.is_empty() {
        return Vec::new();
    }
    // the definitions a proof unfolds: the module's own, then the imported
    let own = super::lower(module, typing).ok();
    let program = Modules {
        own: own.as_ref(),
        imported: imported(),
    };
    let mut out = Vec::new();
    // the proofs checked so far, in order: what a later one may use
    let mut lemmas = Vec::new();
    for lowered in lower_proofs(module, typing) {
        let Some(info) = typing.proofs.iter().find(|i| i.at == lowered.at) else {
            continue;
        };
        let checked = statement(&lowered, info).and_then(|(s, body)| {
            let cert = logic::cert(body, &lemmas)?;
            logic::prove(&s, &cert, &info.data, &program, &lemmas).map(|()| s)
        });
        match checked {
            Ok(statement) => lemmas.push(logic::Lemma {
                statement,
                order: (lowered.params.iter())
                    .zip(&lowered.premise)
                    .map(|(x, p)| (x.as_str().into(), *p))
                    .collect(),
            }),
            Err(failure) => out.push(failure.diagnostic(&info.name)),
        }
    }
    out
}

/// What `lowered` states, with the body that proves it.
fn statement<'p>(
    lowered: &'p LoweredProof,
    info: &ProofInfo,
) -> Result<(logic::Statement, &'p Expr), Failure> {
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
        GoalType::Malformed => return Err(refused(lowered.span, "its goal is no statement")),
    };
    let body = lowered
        .body
        .as_ref()
        .ok_or_else(|| refused(lowered.span, "it has no proof"))?;
    let statement = logic::Statement {
        name: info.name.clone(),
        span: lowered.span,
        params,
        premises,
        goal,
    };
    Ok((statement, body))
}

/// The compiled modules a proof reads definitions from.
struct Modules<'a> {
    own: Option<&'a Module>,
    imported: &'a [Module],
}

impl Modules<'_> {
    fn module(&self, name: &str) -> Option<&Module> {
        self.own
            .into_iter()
            .chain(self.imported)
            .find(|m| m.name == name)
    }
}

impl logic::Program for Modules<'_> {
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
