//! `schema T` and `codec s { field = … }`. A `codec` block gives every field
//! of the record `s` describes exactly once, each with a `FieldCodec`; it is
//! a `PureCodec Json R` when no field needs services to decode, a
//! `Codec rd none Json R` otherwise. A refined type's `parse` of a constant
//! is checked when the program compiles, if the compiler knows the check.

use super::Checker;
use super::ty::{Row, Type};
use super::typing::{CodecBlock, Ref, range};
use crate::consteval::{self, Const};
use crate::diagnostics::codes;
use crate::source::SourceMap;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::{self, literal};
use crate::types::lang::Lang;

impl Checker<'_> {
    /// `schema T`: the witness `T` derives.
    pub fn schema_expr(&mut self, node: &Node) -> Type {
        let span = self.span(node);
        let parts: Vec<String> = node
            .child_tokens()
            .filter(|t| t.kind == K::Con)
            .map(|t| t.text(self.text).to_owned())
            .collect();
        let Some((name, qualifier)) = parts.split_last() else {
            return Type::Error;
        };
        let Some((module, _)) = self.find_type(node, &qualifier.join("."), name) else {
            return Type::Error;
        };
        let entity = super::witness_entity(&module, name);
        let Some(scheme) = self.env.value(&entity).cloned() else {
            self.error(
                codes::D001,
                span,
                format!("`{name}` derives no Schema: add `deriving Schema` to its declaration"),
            );
            return Type::Error;
        };
        let ty = self.instantiate(&scheme, span);
        self.record_name(
            span,
            Ref::Value {
                module,
                name: entity.name,
            },
        );
        ty
    }

    /// `codec s { field = … }`.
    pub fn codec_expr(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let span = self.span(node);
        let Some(head) = node.nodes().next() else {
            return Type::Error;
        };
        let decoded = self.fresh();
        let schema = self.infer(head);
        self.expect(
            &schema,
            &Type::app(Type::lang(Lang::Schema), decoded.clone()),
            self.span(head),
        );
        let fields: Vec<(String, &Node, &Node)> = node
            .nodes()
            .filter(|n| n.kind == K::RecordExpr)
            .flat_map(Node::nodes)
            .filter(|n| n.kind == K::RecordField)
            .filter_map(|f| {
                let label = f.child_tokens().find(|t| t.kind == K::Ident)?;
                let value = f.nodes().next()?;
                Some((label.text(self.text).to_owned(), f, value))
            })
            .collect();
        let record = match self.whnf(&self.zonk(&decoded)) {
            Type::Record(row) if row.tail.is_none() => row,
            Type::Error => return Type::Error,
            other => {
                let shown = self.show(&other);
                self.error(
                    codes::D007,
                    self.span(head),
                    format!(
                        "a `codec` block describes a record, but this is a schema of `{shown}`"
                    ),
                );
                return Type::Error;
            }
        };
        let (rows, given) = self.codec_fields(
            &fields,
            &record,
            #[cfg(feature = "ide")]
            &decoded,
        );
        let missing: Vec<String> = record
            .entries
            .iter()
            .filter(|(l, _)| !given.contains(l))
            .map(|(l, _)| format!("`{l}`"))
            .collect();
        if !missing.is_empty() {
            self.error(
                codes::D002,
                span,
                format!("the codec misses the field {}", missing.join(", ")),
            );
        }
        let name = match head.kind {
            K::SchemaExpr => head
                .child_tokens()
                .filter(|t| t.kind == K::Con)
                .last()
                .map_or_else(String::new, |t| t.text(self.text).to_owned()),
            _ => String::new(),
        };
        self.typing.codecs.insert(range(span), CodecBlock { name });
        self.codec_type(&rows, &fields, decoded, expected)
    }

    /// Checks each field of a block against its label's type in `record`:
    /// the decoding services of each, and the labels given.
    fn codec_fields(
        &mut self,
        fields: &[(String, &Node, &Node)],
        record: &Row,
        #[cfg(feature = "ide")] decoded: &Type,
    ) -> (Vec<Type>, Vec<String>) {
        let mut rows = Vec::new();
        let mut given: Vec<String> = Vec::new();
        let mut wires: Vec<String> = Vec::new();
        for (label, field, value) in fields {
            #[cfg(feature = "ide")]
            if let Some(token) = field.child_tokens().find(|t| t.kind == K::Ident) {
                self.note_field(self.token_span(token), decoded);
            }
            let Some(ty) = record.get(label).cloned() else {
                self.error(
                    codes::D004,
                    self.span(field),
                    format!("the record has no field `{label}`"),
                );
                continue;
            };
            if given.contains(label) {
                self.error(
                    codes::D003,
                    self.span(field),
                    format!("`{label}` is given twice"),
                );
                continue;
            }
            given.push(label.clone());
            let rd = self.fresh();
            let codec = Type::apps(Type::lang(Lang::FieldCodec), [rd.clone(), ty]);
            self.check(value, &codec);
            rows.push(rd);
            if let Some(wire) = wire_name(value, self.text) {
                if wires.contains(&wire) {
                    self.error(
                        codes::D005,
                        self.span(value),
                        format!("the wire name \"{wire}\" is already a field's"),
                    );
                }
                wires.push(wire);
            }
        }
        (rows, given)
    }

    /// A `PureCodec` when no field needs services or one is expected, a
    /// `Codec` of the fields' services otherwise.
    fn codec_type(
        &mut self,
        rows: &[Type],
        fields: &[(String, &Node, &Node)],
        decoded: Type,
        expected: Option<&Type>,
    ) -> Type {
        let pure_wanted = expected
            .map(|e| self.whnf(e))
            .is_some_and(|e| e.spine().0.is_lang(Lang::PureCodec));
        let json = Type::lang(Lang::Json);
        if pure_wanted || rows.iter().all(|r| self.empty_row(r)) {
            for (r, (label, _, value)) in rows.iter().zip(fields) {
                if self.unify(r, &Type::empty_row()).is_err() {
                    let shown = self.show(r);
                    self.error(
                        codes::T003,
                        self.span(value),
                        format!("decoding `{label}` needs `{shown}`, so the codec is no `PureCodec`: use a `Codec`"),
                    );
                }
            }
            return Type::apps(Type::lang(Lang::PureCodec), [json, decoded]);
        }
        let services = rows
            .iter()
            .cloned()
            .reduce(|a, b| Type::Union(Box::new(a), Box::new(b)))
            .unwrap_or_else(Type::empty_row);
        Type::apps(
            Type::lang(Lang::Codec),
            [services, Type::empty_row(), json, decoded],
        )
    }

    /// Whether the row `r` is known to be empty.
    fn empty_row(&self, r: &Type) -> bool {
        let norm = self.store.normalize(r);
        norm.entries.is_empty() && norm.tails.is_empty()
    }

    /// `T.parse c` of a constant `c`: the check of `T`, applied when the
    /// program compiles if the compiler knows it.
    pub fn refined_constant(&mut self, head: &Node, arg: &Node) {
        let Some(Ref::Value { module, name }) = self.typing.names.get(&head.range()).cloned()
        else {
            return;
        };
        let Some(ty) = name.strip_suffix("$parse") else {
            return;
        };
        if module != self.env.module {
            return;
        }
        let Some(constant) = consteval::eval(arg, self.text) else {
            return;
        };
        let Some(predicate) = self.predicates.get(ty).copied() else {
            return;
        };
        if holds(predicate, self.text, &constant) == Some(false) {
            self.error(
                codes::D006,
                self.span(arg),
                format!(
                    "this constant fails the check of `{ty}`, so `{ty}.parse` of it always fails"
                ),
            );
        }
    }
}

/// The wire name of a field: the string literal `field "x" …` starts with.
fn wire_name(value: &Node, text: &str) -> Option<String> {
    let mut found = None;
    walk(value, &mut |n| {
        if found.is_none() && n.kind == K::AppExpr {
            let mut parts = n.nodes();
            let head = parts.next()?;
            let arg = parts.next()?;
            let name = head.text(text);
            if matches!(name.trim(), "field" | "optional_field" | "field_effect")
                && arg.kind == K::Literal
                && let Some(t) = arg.child_tokens().next()
                && t.kind == K::String
            {
                found = Some(literal::string_value(t.text(text)));
            }
        }
        Some(())
    });
    found
}

fn walk(node: &Node, f: &mut impl FnMut(&Node) -> Option<()>) {
    let _ = f(node);
    for child in node.nodes() {
        walk(child, f);
    }
}

/// Whether `constant` passes the predicate of a refined type, when the
/// compiler can tell: a known check, or a `fun` of literals.
fn holds(predicate: &Node, text: &str, constant: &Const) -> Option<bool> {
    let body = predicate.nodes().next()?;
    let written: String = body.text(text).split_whitespace().collect();
    match (written.as_str(), constant) {
        ("Checks.email", Const::String(s)) => return Some(email(s)),
        ("Checks.non_empty", Const::String(s)) => return Some(!s.is_empty()),
        ("Checks.positive", Const::Number(n)) => return Some(*n > 0.0),
        _ => {}
    }
    if body.kind != K::FunExpr {
        return None;
    }
    let param = body.nodes().find(|n| n.kind == K::VarPat)?;
    let param = param.text(text).trim().to_owned();
    let inner = body.nodes().find(|n| n.kind == K::Block)?;
    let literal = match constant {
        Const::Number(n) if n.is_finite() => format!("({})", js_number(*n)),
        Const::String(s) => serde_json::to_string(s).ok()?,
        Const::Bool(b) => b.to_string(),
        _ => return None,
    };
    // the body with the constant in place of the parameter, folded as a literal
    let mut source = String::new();
    for token in inner.tokens() {
        let written = token.text(text);
        if token.kind == K::Ident && written == param {
            source.push_str(&literal);
        } else {
            source.push_str(written);
        }
    }
    let src = format!("let x = {}\n", source.trim());
    let mut map = SourceMap::default();
    let file = map.add("<check>", src.as_str());
    let parsed = syntax::parse(&src, file);
    let decl = parsed.tree.nodes().next()?;
    let value = decl.nodes().find(|n| n.kind == K::Block)?;
    match consteval::eval(value, &src)? {
        Const::Bool(b) => Some(b),
        _ => None,
    }
}

fn js_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{n:.0}")
    } else {
        format!("{n}")
    }
}

/// `Checks.email`, as the runtime checks it: one `@` with text on both
/// sides, no spaces, and a dot inside the domain.
fn email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !s.chars().any(char::is_whitespace)
        && !domain.contains('@')
        && domain
            .char_indices()
            .any(|(i, c)| c == '.' && i > 0 && i + 1 < domain.len())
}
