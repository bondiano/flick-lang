//! `proof` declarations, typed: the type variables of the statement are
//! rigid, the goal's sides are ordinary expressions of one type, and the
//! proof is an expression whose rules have types of their own. Whether the
//! proof proves the goal is the logic's to decide, after lowering.

use std::iter::repeat;
use std::mem;
use std::slice::from_ref;

use super::Checker;
use super::convert::TyVar;
use super::kinds::Kind;
use super::lang::Lang;
pub use super::proof_bundles::Bundle;
use super::ty::Type;
use super::typing::{Range, Ref, range};
use crate::diagnostics::codes;
use crate::logic::{self, LEMMAS, RULES};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// What the checker settled of one proof declaration.
#[derive(Debug, Clone)]
pub struct ProofInfo {
    pub name: String,
    pub at: Range,
    /// The type of each parameter, a premise's the type of its goal's sides.
    pub params: Vec<logic::Ty>,
    pub goal: GoalType,
    pub data: logic::Data,
    /// For a law, the class whose law it is.
    pub class: Option<Law>,
}

/// A class by its module and name, with its parameters and their kinds.
type ClassOf<'a> = (&'a str, &'a str, &'a [(String, Kind)]);

/// Verification evidence tied to one elaborated declaration.
#[derive(Debug, Clone)]
pub struct ProofEvidence {
    pub at: Range,
    pub status: ProofStatus,
    pub failure: Option<Span>,
    pub steps: Vec<logic::ProofStep>,
    /// Checked terms at the declaration goal, including when the body is empty.
    pub header_suggestions: Vec<String>,
    #[cfg(not(target_arch = "wasm32"))]
    pub search: Option<logic::ProofSearchResult>,
    /// Checked lemmas visible before this declaration, using source qualifiers.
    pub lemmas: Vec<String>,
    pub instance: Option<String>,
    pub obligations: Vec<LawObligation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofStatus {
    Checked,
    Open,
    Blocked,
    BudgetExhausted,
}

#[cfg(not(target_arch = "wasm32"))]
impl ProofStatus {
    pub(crate) fn of_failure(failure: &logic::Failure) -> Self {
        match failure.cause() {
            logic::Failure::Refused(_) => Self::Blocked,
            logic::Failure::Exhausted { .. } => Self::BudgetExhausted,
            _ => Self::Open,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LawObligation {
    pub path: Vec<String>,
    pub class: String,
    pub class_module: String,
    pub law: String,
    pub status: ProofStatus,
    pub statement: Option<String>,
    pub range: Option<Range>,
    /// Checked lemma names for this law of exactly the selected instance.
    pub suggestions: Vec<String>,
}

/// The class a law is of, with the names its parameters have in the law's
/// types.
#[derive(Debug, Clone)]
pub struct Law {
    pub module: String,
    pub class: String,
    pub params: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum GoalType {
    /// `equal l r` or `holds p`, of the sides' type.
    Equal(logic::Ty),
    SameType(logic::Ty, logic::Ty),
    /// `lawful C T`: the laws of `C` and its superclasses at `C T`.
    Lawful(Bundle),
    /// A goal the domain does not have, and why.
    Outside(String),
    /// No goal the checker reads: reported already.
    Malformed,
}

/// The parts of a goal as written.
pub enum GoalSyntax<'n> {
    Equal(&'n Node, &'n Node),
    Holds(&'n Node),
    SameType(&'n Node, &'n Node),
    Other(&'n Node),
}

/// The parts of `goal`, a `Goal` or a `SameTypeGoal` node.
pub fn goal_syntax<'n>(goal: &'n Node, text: &str) -> GoalSyntax<'n> {
    let mut parts = goal.nodes();
    if goal.kind == K::SameTypeGoal
        && let (Some(a), Some(b)) = (parts.next(), parts.next())
    {
        return GoalSyntax::SameType(a, b);
    }
    let Some(expr) = goal.nodes().next() else {
        return GoalSyntax::Other(goal);
    };
    let args: Vec<&Node> = expr.nodes().collect();
    let head = (expr.kind == K::AppExpr)
        .then(|| args.first())
        .flatten()
        .filter(|h| h.kind == K::NameExpr)
        .map(|h| h.text(text));
    match (head.as_deref().map(str::trim), args.as_slice()) {
        (Some("equal"), [_, l, r]) => GoalSyntax::Equal(l, r),
        (Some("holds"), [_, p]) => GoalSyntax::Holds(p),
        _ => GoalSyntax::Other(expr),
    }
}

/// `Proof a`: a proof of an equation between values of type `a`.
fn proof_type(of: Type) -> Type {
    Type::app(Type::con(RULES, "Proof"), of)
}

impl Checker<'_> {
    pub(super) fn check_proofs(&mut self, decls: &[&Node]) {
        for decl in decls.iter().filter(|d| d.kind == K::ProofDecl) {
            self.check_statement(decl, None);
        }
    }

    /// The laws of each class of the module: statements typed as its default
    /// methods are, with the class's parameters rigid and the class given.
    pub(super) fn check_laws(&mut self, decls: &[&Node]) {
        let module = self.env.module.clone();
        for decl in decls.iter().filter(|d| d.kind == K::ClassDecl) {
            let Some(class) = self.con_token(decl) else {
                continue;
            };
            let (Some(info), Some(def)) = (
                self.class_info(&module, &class),
                self.env.type_def(&module, &class).cloned(),
            ) else {
                continue;
            };
            let mut kind = &def.kind;
            let mut kinds = Vec::new();
            while let Kind::Arrow(param, rest) = kind {
                kinds.push((**param).clone());
                kind = rest;
            }
            let params: Vec<(String, Kind)> = (info.params.into_iter())
                .zip(kinds.into_iter().chain(repeat(Kind::Type)))
                .collect();
            let laws = (decl.nodes().filter(|n| n.kind == K::Members))
                .flat_map(Node::nodes)
                .filter(|n| n.kind == K::LawDecl);
            for law in laws {
                self.check_statement(law, Some((&module, &class, &params)));
            }
        }
    }

    /// The class of a law in scope: its parameters rigid at their kinds, and
    /// its constraint given.
    fn law_scope(&mut self, decl: &Node, (module, name, params): ClassOf) -> Law {
        let mut args = Vec::new();
        let mut names = Vec::new();
        for (param, kind) in params {
            let ty = self.store.skolem(param);
            if let Type::Skolem(s) = ty {
                self.cx.law.push(s);
                names.push(self.store.skolem_name(s).to_owned());
            }
            args.push(ty.clone());
            self.cx.tyvars.push(TyVar {
                name: param.clone(),
                ty,
                kind: kind.clone(),
            });
        }
        let given = Type::apps(Type::con(module, name), args);
        self.record_givens(self.span(decl), from_ref(&given));
        self.classes.givens.push(given);
        Law {
            module: module.to_owned(),
            class: name.to_owned(),
            params: names,
        }
    }

    /// A proof, or a law of `class`.
    fn check_statement(&mut self, decl: &Node, class: Option<ClassOf>) {
        let outer = mem::take(&mut self.cx);
        let saved = (
            mem::replace(&mut self.store.level, 1),
            mem::take(&mut self.classes.wanted),
            mem::take(&mut self.classes.givens),
        );
        self.cx.proof = Some(decl.range());
        let law = class.map(|c| self.law_scope(decl, c));
        let bound: Vec<String> = self.cx.tyvars.iter().map(|v| v.name.clone()).collect();
        for name in statement_variables(decl, self.text)
            .into_iter()
            .filter(|n| !bound.contains(n))
        {
            let ty = self.store.skolem(&name);
            self.cx.tyvars.push(TyVar {
                name,
                ty,
                kind: Kind::Type,
            });
        }
        let mut params = Vec::new();
        for param in decl
            .nodes()
            .filter(|n| n.kind == K::Params)
            .flat_map(Node::nodes)
        {
            if param.kind == K::Premise {
                let goal = param
                    .nodes()
                    .find(|n| matches!(n.kind, K::Goal | K::SameTypeGoal));
                let ty = goal.map_or(Type::Error, |g| self.proof_goal(g).0);
                if let Some(name) = param.nodes().find(|n| n.kind == K::VarPat) {
                    self.bind_pattern(name, &proof_type(ty.clone()));
                }
                params.push(ty);
            } else {
                let ty = self.store.fresh();
                self.bind_pattern(param, &ty);
                params.push(ty);
            }
        }
        let goal =
            (decl.nodes()).find(|n| matches!(n.kind, K::Goal | K::SameTypeGoal | K::LawfulGoal));
        let body = decl.nodes().find(|n| n.kind == K::Block);
        let lawful = (goal.filter(|g| g.kind == K::LawfulGoal)).map(|g| self.lawful_goal(g, body));
        let (ty, same) = match goal {
            Some(g) if lawful.is_none() => self.proof_goal(g),
            _ => (Type::Error, None),
        };
        if lawful.is_none()
            && let Some(body) = body
        {
            self.check(body, &proof_type(ty.clone()));
        }
        self.settle_operators();
        self.solve_rows();
        self.solve_classes(true);
        let mut data = logic::Data::new();
        let params = params
            .iter()
            .map(|t| self.logic_type(t, &mut data))
            .collect();
        let goal = if let Some(lawful) = lawful {
            self.bundle_goal(lawful, &mut data)
        } else {
            match (&goal, same) {
                (_, Some((a, b))) => GoalType::SameType(
                    self.logic_type(&a, &mut data),
                    self.logic_type(&b, &mut data),
                ),
                (Some(_), None) if ty != Type::Error => {
                    GoalType::Equal(self.logic_type(&ty, &mut data))
                }
                _ => GoalType::Malformed,
            }
        };
        self.constructors_named(decl, &mut data);
        let name = decl
            .nodes()
            .find(|n| n.kind == K::VarPat)
            .map(|n| n.text(self.text).trim().to_owned())
            .unwrap_or_default();
        let info = ProofInfo {
            name,
            at: decl.range(),
            params,
            goal,
            data,
            class: law,
        };
        if info.class.is_some() {
            self.typing.laws.push(info);
        } else {
            self.typing.proofs.push(info);
        }
        self.cx = outer;
        (self.store.level, self.classes.wanted, self.classes.givens) = saved;
    }

    /// The type of the sides of `goal`, and the two types of a `same_type`.
    fn proof_goal(&mut self, goal: &Node) -> (Type, Option<(Type, Type)>) {
        match goal_syntax(goal, self.text) {
            GoalSyntax::Equal(l, r) => {
                let ty = self.infer(l);
                self.check(r, &ty);
                (ty, None)
            }
            GoalSyntax::Holds(p) => {
                let bool_ = Type::lang(Lang::Bool);
                self.check(p, &bool_);
                (bool_, None)
            }
            GoalSyntax::SameType(a, b) => {
                let (a, b) = (self.annotation(a), self.annotation(b));
                (Type::Error, Some((a, b)))
            }
            GoalSyntax::Other(node) => {
                self.error(
                    codes::T083,
                    self.span(node),
                    "a goal is `equal l r`, `holds p` or `same_type A B`",
                );
                (Type::Error, None)
            }
        }
    }

    /// `M.name` in a proof, of a proof of the imported module `M`: a function
    /// of its parameters, which the logic checks once it replays the proof.
    pub(super) fn imported_lemma(
        &mut self,
        qualifier: &str,
        name: &str,
        span: Span,
    ) -> Option<Type> {
        self.cx.proof?;
        let (module, Some(interface)) = self.env.qualified_module(qualifier)? else {
            return None;
        };
        if !interface.proofs.iter().any(|p| p == name) {
            return None;
        }
        let lemma = format!("{module}.{name}");
        self.typing.names.insert(
            range(span),
            Ref::Value {
                module: LEMMAS.into(),
                name: lemma,
            },
        );
        Some(self.store.fresh())
    }

    /// A rule of the proof being checked, instantiated: `None` for any
    /// other name.
    pub(super) fn proof_rule(&mut self, name: &str, at: Range) -> Option<Type> {
        let (a, b) = (self.store.fresh(), self.store.fresh());
        let p = proof_type;
        let fun = Type::pure_fun;
        let (module, ty) = match name {
            "refl" => (RULES, p(a)),
            "symm" => (RULES, fun(p(a.clone()), p(a))),
            "trans" => (
                RULES,
                fun(a.clone(), fun(p(a.clone()), fun(p(a.clone()), p(a)))),
            ),
            "cong" => (RULES, fun(fun(a.clone(), b.clone()), fun(p(a), p(b)))),
            "funext" => (RULES, fun(fun(a.clone(), p(b.clone())), p(fun(a, b)))),
            _ => {
                // a proof before this one: a function of its parameters; the
                // logic checks what its arguments and its instance are
                let arity = self
                    .typing
                    .proofs
                    .iter()
                    .find(|p| p.name == name)?
                    .params
                    .len();
                let ty = (0..arity).fold(p(a), |ty, _| fun(self.store.fresh(), ty));
                (LEMMAS, ty)
            }
        };
        self.typing.names.insert(
            at,
            Ref::Value {
                module: module.into(),
                name: name.into(),
            },
        );
        Some(ty)
    }
}

/// The type variables the parameters and a `same_type` goal name: the
/// statement holds for every type they stand for.
fn statement_variables(decl: &Node, text: &str) -> Vec<String> {
    fn walk(node: &Node, text: &str, out: &mut Vec<String>) {
        if node.kind == K::NameType {
            let text = node.text(text);
            let name = text.trim();
            if name.starts_with(|c: char| c.is_lowercase()) && !out.iter().any(|n| n == name) {
                out.push(name.to_owned());
            }
        }
        node.nodes().for_each(|n| walk(n, text, out));
    }
    let mut out = Vec::new();
    for part in decl
        .nodes()
        .filter(|n| matches!(n.kind, K::Params | K::SameTypeGoal | K::LawfulGoal))
    {
        walk(part, text, &mut out);
    }
    out
}
