//! Record expressions: literals, field access and `{ r with f = v }`.

use super::Checker;
use super::env::Form;
use super::ty::{Row, Type};
use crate::diagnostics::suggest::closest;
use crate::diagnostics::{Diagnostic, codes};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// The type of a field of a record whose fields are already known,
    /// found without copying the record type: reading every field of a wide
    /// record stays linear.
    fn known_field(&self, record: &Type, label: &str) -> Option<Type> {
        let record = self.store.resolve(record);
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
            labels.extend(row.entries.into_iter().map(|(l, _)| l));
            match row.tail {
                Some(tail) => ty = self.whnf(&tail),
                None => break,
            }
        }
        labels
    }

    pub(super) fn field(&mut self, node: &Node, target: &Node) -> Type {
        let Some(label) = node.child_tokens().find(|t| t.kind == K::Ident) else {
            return Type::Error;
        };
        let label_span = self.token_span(label);
        let label = label.text(self.text).to_owned();
        let record = self.infer(target);
        if let Some(field) = self.known_field(&record, &label) {
            return field;
        }
        let field = self.fresh();
        let rest = self.fresh();
        let shape = Type::Record(Row {
            entries: vec![(label.clone(), field.clone())],
            tail: Some(Box::new(rest)),
        });
        if self.unify(&self.whnf(&record), &shape).is_err() {
            let shown = self.show(&record);
            let labels = self.labels(&record);
            let guess = closest(&label, labels.iter().map(String::as_str));
            self.diagnostics.push(
                Diagnostic::error(
                    codes::T003,
                    format!("`{shown}` has no field `{label}`"),
                    self.span(node),
                )
                .did_you_mean(label_span, guess.map(str::to_owned)),
            );
            return Type::Error;
        }
        field
    }

    /// A record literal; `expected` gives the types of fields it mentions.
    pub(super) fn record(&mut self, node: &Node, expected: Option<&Row>) -> Type {
        let mut entries: Vec<(String, Type)> = Vec::new();
        for field in node.nodes().filter(|n| n.kind == K::RecordField) {
            let Some(label) = field.child_tokens().find(|t| t.kind == K::Ident) else {
                continue;
            };
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
                entries.push((name, ty));
            }
        }
        Type::Record(Row::closed(entries))
    }

    /// `{ r with f = v }` replaces existing fields with values of the same type.
    pub(super) fn record_update(&mut self, node: &Node, children: &[&Node]) -> Type {
        let record = self.infer(children[0]);
        let mut seen: Vec<String> = Vec::new();
        for field in node.nodes().filter(|n| n.kind == K::RecordField) {
            let Some(label) = field.child_tokens().find(|t| t.kind == K::Ident) else {
                continue;
            };
            let name = label.text(self.text).to_owned();
            if seen.contains(&name) {
                self.duplicate_label(&name, self.token_span(label));
                continue;
            }
            let ty = self.fresh();
            let rest = self.fresh();
            let shape = Type::Record(Row {
                entries: vec![(name.clone(), ty.clone())],
                tail: Some(Box::new(rest)),
            });
            if self.unify(&self.whnf(&record), &shape).is_err() {
                let shown = self.show(&record);
                let labels = self.labels(&record);
                let guess = closest(&name, labels.iter().map(String::as_str));
                self.error_guess(
                    codes::T003,
                    self.token_span(label),
                    format!(
                        "`{shown}` has no field `{name}`; `with` only replaces existing fields"
                    ),
                    guess,
                );
            } else if let Some(value) = field.nodes().next() {
                self.check(value, &ty);
            }
            seen.push(name);
        }
        record
    }
}
