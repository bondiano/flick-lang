//! `proof` declarations, typed: the type variables of the statement are
//! rigid, the goal's sides are ordinary expressions of one type, and the
//! proof is an expression whose rules have types of their own. Whether the
//! proof proves the goal is the logic's to decide, after lowering.

use std::mem;

use super::Checker;
use super::convert::TyVar;
use super::kinds::Kind;
use super::lang::Lang;
use super::ty::Type;
use super::typing::{Range, Ref};
use crate::diagnostics::codes;
use crate::logic::{self, LEMMAS, RULES};
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
}

#[derive(Debug, Clone)]
pub enum GoalType {
    /// `equal l r` or `holds p`, of the sides' type.
    Equal(logic::Ty),
    SameType(logic::Ty, logic::Ty),
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
            self.check_proof(decl);
        }
    }

    fn check_proof(&mut self, decl: &Node) {
        let outer = mem::take(&mut self.cx);
        let saved = (
            mem::replace(&mut self.store.level, 1),
            mem::take(&mut self.classes.wanted),
            mem::take(&mut self.classes.givens),
        );
        self.cx.proof = Some(decl.range());
        for name in statement_variables(decl, self.text) {
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
        let goal = decl
            .nodes()
            .find(|n| matches!(n.kind, K::Goal | K::SameTypeGoal));
        let (ty, same) = goal.map_or((Type::Error, None), |g| self.proof_goal(g));
        if let Some(body) = decl.nodes().find(|n| n.kind == K::Block) {
            self.check(body, &proof_type(ty.clone()));
        }
        self.settle_operators();
        self.solve_classes(true);
        self.solve_rows();
        let mut data = logic::Data::new();
        let params = params
            .iter()
            .map(|t| self.logic_type(t, &mut data))
            .collect();
        let goal = match (&goal, same) {
            (_, Some((a, b))) => GoalType::SameType(
                self.logic_type(&a, &mut data),
                self.logic_type(&b, &mut data),
            ),
            (Some(_), None) if ty != Type::Error => {
                GoalType::Equal(self.logic_type(&ty, &mut data))
            }
            _ => GoalType::Malformed,
        };
        self.constructors_named(decl, &mut data);
        let name = decl
            .nodes()
            .find(|n| n.kind == K::VarPat)
            .map(|n| n.text(self.text).trim().to_owned())
            .unwrap_or_default();
        self.typing.proofs.push(ProofInfo {
            name,
            at: decl.range(),
            params,
            goal,
            data,
        });
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
        .filter(|n| matches!(n.kind, K::Params | K::SameTypeGoal))
    {
        walk(part, text, &mut out);
    }
    out
}
