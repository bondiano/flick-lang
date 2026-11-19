//! A bundle, `proof name where lawful C T = { … }`: a lemma for each law of
//! `C` and of each of its superclasses, about the dictionary `C T` resolves
//! to. The record mirrors the dictionary: a superclass is a field holding
//! the record of its own laws.

use super::Checker;
use super::classes::{split, super_slot};
use super::convert::Free;
use super::kinds::Kind;
use super::module_contract::dictionary_type;
use super::proof_decls::GoalType;
use super::ty::Type;
use crate::logic;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The classes a bundle covers: a class with its arguments as types of
/// the domain, and the bundle of each superclass by its dictionary field.
#[derive(Debug, Clone)]
pub struct Bundle {
    pub module: String,
    pub class: String,
    pub args: Vec<logic::Ty>,
    pub supers: Vec<(String, Bundle)>,
}

impl Checker<'_> {
    /// The constraint of a `lawful` goal, wanted where the goal is so that
    /// its dictionary is the bundle's subject, with its record's lemmas
    /// typed; or why it is outside the domain.
    pub(super) fn lawful_goal(&mut self, goal: &Node, body: Option<&Node>) -> Result<Type, String> {
        let mut parts = goal.nodes();
        let constraint = match parts.next() {
            Some(head) => self.ty(head, &Kind::Constraint, Free::Reject),
            None => Type::Error,
        };
        if let Some(record) = body.and_then(record) {
            self.lemma_fields(record);
        }
        if let Some(from) = parts.next() {
            if let Some(dictionary) = dictionary_type(&constraint) {
                self.check(from, &dictionary);
            }
            return Err(
                "a named dictionary is outside the domain: a bundle is about an instance".into(),
            );
        }
        if constraint != Type::Error {
            self.want(constraint.clone(), self.span(goal));
        }
        Ok(constraint)
    }

    /// What a `lawful` goal states, once its types are settled.
    pub(super) fn bundle_goal(
        &mut self,
        lawful: Result<Type, String>,
        data: &mut logic::Data,
    ) -> GoalType {
        match lawful {
            Ok(constraint) => {
                let constraint = self.zonk(&constraint);
                self.bundle(&constraint, data)
                    .map_or(GoalType::Malformed, GoalType::Lawful)
            }
            Err(why) => GoalType::Outside(why),
        }
    }

    fn bundle(&mut self, constraint: &Type, data: &mut logic::Data) -> Option<Bundle> {
        let ((module, class), args) = split(constraint)?;
        let (module, class) = (module.to_owned(), class.to_owned());
        let args: Vec<Type> = args.into_iter().cloned().collect();
        let info = self.class_info(&module, &class)?;
        let subst: Vec<(String, Type)> = (info.params.iter().cloned())
            .zip(args.iter().cloned())
            .collect();
        let mut supers = Vec::new();
        for (i, s) in info.supers.iter().enumerate() {
            supers.push((
                super_slot(&info.supers, i),
                self.bundle(&s.substitute(&subst), data)?,
            ));
        }
        let args = args.iter().map(|a| self.logic_type(a, data)).collect();
        Some(Bundle {
            module,
            class,
            args,
            supers,
        })
    }

    /// Types the value of each field of a bundle's record: a lemma, or the
    /// record of a superclass.
    fn lemma_fields(&mut self, record: &Node) {
        for field in record.nodes().filter(|n| n.kind == K::RecordField) {
            let Some(value) = field.nodes().last() else {
                continue;
            };
            match self::record(value) {
                Some(inner) => self.lemma_fields(inner),
                None => {
                    self.infer(value);
                }
            }
        }
    }
}

/// The record `node` is, through the blocks that hold it.
pub fn record(node: &Node) -> Option<&Node> {
    match node.kind {
        K::RecordExpr => Some(node),
        K::Block | K::ParenExpr => node.nodes().next().and_then(record),
        _ => None,
    }
}
