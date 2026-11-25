//! Record expressions: literals, field access, accessors `.a.b` and
//! `{ r with f = v }`.

use super::Checker;
use super::env::Form;
use super::ty::{Row, Type};
use super::typing::Range;
use crate::diagnostics::suggest::closest;
use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;
use crate::syntax::cst::{Node, Token};
use crate::syntax::fields;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Elem;

/// The keys of an accessor's parameter and of the value of its body, as
/// the keys of its closure's own: its first `.` and its last token, the
/// label of its last step. No expression spans a token alone, not even an
/// accessor of one step.
pub fn accessor_keys(node: &Node) -> (Range, Range) {
    let mut tokens = (node.nodes())
        .flat_map(Node::child_tokens)
        .filter(|t| !t.kind.is_trivia());
    let first = tokens.next();
    let key = |t: Option<Token>| t.map_or(node.range(), |t| (t.start, t.end));
    (key(first), key(tokens.last().or(first)))
}

impl Checker<'_> {
    /// The type of a field of a record whose fields are already known,
    /// found without copying the record type: reading every field of a wide
    /// record stays linear.
    fn known_field(&self, record: &Type, label: &str) -> Option<Type> {
        let record = self.store.resolve(record);
        if let Type::Captured { ty, captures, .. } | Type::Bounded { ty, captures } = &record {
            return self
                .known_field(ty, label)
                .map(|field| self.data_field(&field, captures));
        }
        let (head, args) = record.spine();
        let (row, subst) = match head {
            Type::Record(row) => (row, Vec::new()),
            Type::Con { module, name } => match &self.env.type_def(module, name)?.form {
                Form::Alias {
                    params,
                    body: Type::Record(row),
                    ..
                } if params.len() == args.len() => {
                    let subst: Vec<(String, Type)> = params
                        .iter()
                        .cloned()
                        .zip(args.iter().map(|a| (*a).clone()))
                        .collect();
                    (row, subst)
                }
                _ => return None,
            },
            _ => return None,
        };
        row.get(label).map(|t| t.substitute(&subst))
    }

    /// The labels a record type is known to have, for "did you mean".
    fn labels(&self, record: &Type) -> Vec<String> {
        let mut labels = Vec::new();
        let mut ty = self.whnf(record);
        while let Type::Record(row) = ty {
            labels.extend(row.entries.into_iter().map(|(l, _)| l.to_string()));
            match row.tail {
                Some(tail) => ty = self.whnf(&tail),
                None => break,
            }
        }
        labels
    }

    /// `target.label`. A path with an optional step, `user.?address.city`,
    /// is checked as a whole from its outermost step: its root once, and
    /// each step from the value before it.
    pub(super) fn field(&mut self, node: &Node, target: &Node) -> Type {
        let (root, steps) = fields::steps(node);
        if steps.iter().any(|s| fields::is_optional(s)) {
            let Some(root) = root else {
                return Type::Error;
            };
            let value = self.infer(root);
            return self.read_steps(value, &steps);
        }
        let record = self.infer(target);
        self.read_steps(record, &[node])
    }

    /// The type of the field `label` of a value of type `record`, read at
    /// `at`: a polymorphic field is instantiated there, and a missing one
    /// reported there.
    pub(super) fn field_of(&mut self, record: &Type, label: Token, at: Span) -> Type {
        let label_span = self.token_span(label);
        let label = label.text(self.text).to_owned();
        #[cfg(feature = "ide")]
        self.note_field(label_span, record);
        // a polymorphic field (`S.Impl`, a dictionary) is used at an instance,
        // with the dictionaries of its constraints, like a name
        if let Some(field) = self.known_field(record, &label) {
            return self.instantiate(&field, at);
        }
        let field = self.fresh();
        let rest = self.fresh();
        let shape = Type::Record(Row {
            entries: vec![(Elem::named(&label), field.clone())],
            tail: Some(Box::new(rest)),
            ..Row::default()
        });
        if self.unify(&self.whnf(record), &shape).is_err() {
            let shown = self.show(record);
            self.diagnostics.push(
                Diagnostic::error(codes::T003, format!("`{shown}` has no field `{label}`"), at)
                    .did_you_mean(label_span, || {
                        let labels = self.labels(record);
                        closest(&label, labels.iter().map(String::as_str)).map(str::to_owned)
                    }),
            );
            return Type::Error;
        }
        self.instantiate(&field, at)
    }

    /// `.a.b`, checked against `expected` when it is known: the function
    /// `fun r -> r.a.b` with a parameter no one can name. Each step reads
    /// the field of the value before it, the first one of the parameter.
    pub(super) fn accessor(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let steps: Vec<&Node> = (node.nodes())
            .filter(|n| n.kind == K::AccessorStep)
            .collect();
        let (param, result) = accessor_keys(node);
        let at = Span::new(self.file, result.0, result.1);
        let mut body = |c: &mut Self, x: Option<&Type>, rest: Option<Type>| {
            let value = x.cloned().unwrap_or(Type::Error);
            c.note_value(param, &value);
            let value = c.read_steps(value, &steps);
            match rest {
                Some(rest) => {
                    c.expect(&value, &rest, at);
                    rest
                }
                None => value,
            }
        };
        let ty = self.function_typed(&[None], &mut body, expected, node);
        // an editor shows the whole path's type there, and the first step's
        // receiver is its parameter, also while that step has no label yet
        #[cfg(feature = "ide")]
        self.note_type(self.span(node), &ty);
        ty
    }

    /// The labels of the literal `node` checked against `expected`, noted
    /// with that type as it is written, an alias too: the declaration an
    /// editor relates them to.
    #[cfg(feature = "ide")]
    pub(super) fn note_literal(&mut self, node: &Node, expected: &Type) {
        for field in node.nodes().filter(|n| n.kind == K::RecordField) {
            if let Some(label) =
                (field.child_tokens()).find(|t| matches!(t.kind, K::Ident | K::Con))
            {
                self.note_field(self.token_span(label), expected);
            }
        }
    }

    /// A record literal; `expected` gives the types of fields it mentions.
    pub(super) fn record(&mut self, node: &Node, expected: Option<&Row>) -> Type {
        let mut entries: Vec<(Elem, Type)> = Vec::new();
        #[cfg(feature = "ide")]
        let mut labels = Vec::new();
        for field in node.nodes().filter(|n| n.kind == K::RecordField) {
            let Some(label) = field
                .child_tokens()
                .find(|t| matches!(t.kind, K::Ident | K::Con))
            else {
                continue;
            };
            #[cfg(feature = "ide")]
            labels.push(self.token_span(label));
            let name = label.text(self.text).to_owned();
            let Some(value) = field.nodes().next() else {
                continue;
            };
            let ty = match expected.and_then(|row| row.get(&name)) {
                Some(ty) => {
                    let ty = ty.clone();
                    self.field_path.push(name.clone());
                    self.check(value, &ty);
                    self.field_path.pop();
                    ty
                }
                None => self.infer(value),
            };
            if entries.iter().any(|(l, _)| *l == name) {
                self.duplicate_label(&name, self.token_span(label));
            } else {
                entries.push((Elem::Named(name), ty));
            }
        }
        let record = Type::Record(Row::closed(entries));
        #[cfg(feature = "ide")]
        {
            // an editor shows the fields as the expected type declares them
            let declared = expected.map_or_else(|| record.clone(), |row| Type::Record(row.clone()));
            for span in labels {
                self.note_field(span, &declared);
            }
        }
        if expected.is_none() {
            let captures = self.captures_of(&record);
            self.data_value(&record, captures)
        } else {
            record
        }
    }

    /// `{ r with f = v }` replaces existing fields with values of the same type.
    pub(super) fn record_update(&mut self, node: &Node, children: &[&Node]) -> Type {
        let record = self.infer(children[0]);
        let mut seen: Vec<String> = Vec::new();
        for field in node.nodes().filter(|n| n.kind == K::RecordField) {
            let Some(label) = field
                .child_tokens()
                .find(|t| matches!(t.kind, K::Ident | K::Con))
            else {
                continue;
            };
            let name = label.text(self.text).to_owned();
            #[cfg(feature = "ide")]
            self.note_field(self.token_span(label), &record);
            if seen.contains(&name) {
                self.duplicate_label(&name, self.token_span(label));
                continue;
            }
            let ty = self.fresh();
            let rest = self.fresh();
            let shape = Type::Record(Row {
                entries: vec![(Elem::named(&name), ty.clone())],
                tail: Some(Box::new(rest)),
                ..Row::default()
            });
            if self.unify(&self.whnf(&record), &shape).is_err() {
                let shown = self.show(&record);
                self.error_guess(
                    codes::T003,
                    self.token_span(label),
                    format!(
                        "`{shown}` has no field `{name}`; `with` only replaces existing fields"
                    ),
                    |this| {
                        let labels = this.labels(&record);
                        closest(&name, labels.iter().map(String::as_str)).map(str::to_owned)
                    },
                );
            } else if let Some(value) = field.nodes().next() {
                self.check(value, &ty);
            }
            seen.push(name);
        }
        record
    }
}

#[cfg(test)]
#[path = "records_tests.rs"]
mod tests;
