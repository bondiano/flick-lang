//! The Go types of constructors of the program's own types. A constructor
//! with fields that only compiled code makes has a Go type of its own: its
//! values are that type and nothing else, so a `match` asks Go for the type
//! and reads a field where it is, with no tag to compare and no list of
//! fields to find. The runtime reads such a value as `flickrt.Constructed`
//! and builds one, where it decodes or generates, through `flickrt.Owned`.
//!
//! A constructor declared over a record, `Node { left : Tree; key : Int }`,
//! holds that record in its value: the header of a `flickrt.Record` over
//! the values beside it, so the record a `match` binds is a pointer into
//! the value and one object is made, not a constructor and a record. Its
//! labels are in the order of the declaration, whatever order a literal
//! writes them in.
//!
//! The types are one file of the program: a constructor is used in many
//! modules and declared, in Go, once.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

use flick_pretty::{Document, docvec};

use super::layout::{Code, call};
use super::{Gen, names};
use crate::ir::{Expr, ExprKind, Module};
use crate::types::{Constructor, Rep};

/// How a value of a constructor with a Go type of its own holds its fields.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum Held {
    /// So many fields, each a value.
    Fields(usize),
    /// One field: a record of these labels, which lies in the value.
    Record(Vec<String>),
}

/// The Go types a module's code names, each with its tag and its fields.
pub(super) type Owned = BTreeMap<String, (String, Held)>;

/// The constructors declared over a record, by module and name, with the
/// labels of the record in the order declared.
pub(super) type Records = HashMap<(String, String), Vec<String>>;

pub(super) fn records(modules: &[&Module]) -> Records {
    let of = |m: &Module| {
        let named =
            |(c, labels): (&String, &Vec<String>)| ((m.name.clone(), c.clone()), labels.clone());
        m.records.iter().map(named).collect::<Vec<_>>()
    };
    modules.iter().flat_map(|m| of(m)).collect()
}

impl<'a> Gen<'a> {
    /// The Go type of the values of `c` and how it holds their fields, when
    /// they have a type of their own: a tagged constructor with fields, of
    /// a module of the program, that holds no dictionary. One without
    /// fields stays the one `*flickrt.Data` of its tag, and a built-in type
    /// is made by the runtime too.
    pub(super) fn own_type(&mut self, c: &Constructor) -> Option<(String, Held)> {
        let own = self.own?;
        if c.rep != Rep::Tagged || c.arity == 0 || c.dicts > 0 || !own(&c.module) {
            return None;
        }
        let ty = names::own_type(&c.module, &c.name);
        if let Some((_, held)) = self.owned.get(&ty) {
            return Some((ty, held.clone()));
        }
        let labels = self
            .records
            .and_then(|records| records.get(&(c.module.clone(), c.name.clone())));
        let held = match labels {
            Some(labels) if c.arity == 1 && !labels.is_empty() => Held::Record(labels.clone()),
            _ => Held::Fields(c.arity),
        };
        let tag = names::string(&self.tag(c));
        self.owned.insert(ty.clone(), (tag, held.clone()));
        Some((ty, held))
    }

    /// The field `label` of `record`. A local bound to the record that lies
    /// in a value of a constructor has its labels in the order declared, so
    /// the field is read by its place; any other record by its label.
    pub(super) fn field(&mut self, record: &Expr, label: &str) -> Code<'a> {
        let a = self.a;
        let r = self.expr(record);
        let held = match &record.kind {
            ExprKind::Local(name) => self.held.get(&names::local(name)),
            _ => None,
        };
        match held.and_then(|labels| labels.iter().position(|l| l == label)) {
            Some(place) => call(a, "flickrt.FieldAt", [r.doc, docvec![a, place]]).into(),
            None => call(a, "flickrt.Field", [r.doc, self.string(label)]).into(),
        }
    }

    /// A constructor over a record that lies in its value, applied where
    /// that record is written: the values of a literal go to their places
    /// in the value, and `{ r with … }` is a copy of `r` in a new value
    /// with those places set. No record apart from the value is made.
    pub(super) fn over_record(&mut self, e: &Expr) -> Option<Code<'a>> {
        let ExprKind::App { func, arg, .. } = &e.kind else {
            return None;
        };
        let ExprKind::Con(c) = &func.kind else {
            return None;
        };
        let (ty, Held::Record(labels)) = self.own_type(c)? else {
            return None;
        };
        let a = self.a;
        let places = |fields: &[(String, Expr)]| -> Option<Vec<usize>> {
            let place = |(label, _): &(String, Expr)| labels.iter().position(|l| l == label);
            fields.iter().map(place).collect()
        };
        match &arg.kind {
            ExprKind::Record(fields) => {
                let places = places(fields)?;
                let mut written = places.clone();
                written.sort_unstable();
                if written.iter().copied().ne(0..labels.len()) {
                    // a label written twice, or not every label
                    return None;
                }
                let values: Vec<&Expr> = fields.iter().map(|(_, v)| v).collect();
                let mut go = self.ordered(&values);
                if places.iter().copied().ne(0..labels.len()) {
                    // written in another order than declared: evaluated as written
                    go = go.into_iter().map(|v| self.spill(v)).collect();
                }
                let mut sorted: Vec<(usize, Document<'a>)> =
                    places.into_iter().zip(go.iter().map(|v| v.doc)).collect();
                sorted.sort_unstable_by_key(|(place, _)| *place);
                let args = sorted.into_iter().map(|(_, v)| v);
                Some(call(a, a.alloc_str(&make(&ty)), args).into())
            }
            ExprKind::Update(base, fields) => {
                let places = places(fields)?;
                let mut all: Vec<&Expr> = vec![base];
                all.extend(fields.iter().map(|(_, v)| v));
                let mut go = self.ordered(&all).into_iter();
                let copy = a.alloc_str(&format!("With_{ty}"));
                let mut made = call(a, copy, go.next().map(|base| base.doc));
                for (v, place) in go.zip(places) {
                    made = docvec![a, made, ".Set(", place, ", ", v.doc, ")"];
                }
                Some(docvec![a, "flickrt.Value(", made, ")"].into())
            }
            _ => None,
        }
    }
}

/// The function making a value of the Go type `ty` from its fields.
pub(super) fn make(ty: &str) -> String {
    format!("New_{ty}")
}

/// The function making a value of the Go type `ty` over a record.
pub(super) fn of(ty: &str) -> String {
    format!("Of_{ty}")
}

/// The function telling whether a value is of the Go type `ty`.
pub(super) fn test(ty: &str) -> String {
    format!("Is_{ty}")
}

/// The Go file declaring `types`.
pub(super) fn file(types: &Owned) -> String {
    let mut go = String::from("package main\n\nimport \"flick.app/flickrt\"\n");
    for (ty, (tag, held)) in types {
        let n = match held {
            Held::Fields(n) => *n,
            Held::Record(labels) => labels.len(),
        };
        let params: Vec<String> = (0..n).map(|i| format!("f{i}")).collect();
        let params = params.join(", ");
        let (make, of, test) = (make(ty), of(ty), test(ty));
        let _ = match held {
            Held::Fields(_) => write!(
                go,
                "\ntype {ty} struct{{ f [{n}]flickrt.Value }}\n\n\
                 func (p *{ty}) Fields() []flickrt.Value {{ return p.f[:] }}\n\n\
                 func {make}({params} flickrt.Value) flickrt.Value {{\n\
                 \treturn &{ty}{{[{n}]flickrt.Value{{{params}}}}}\n}}\n\n\
                 var _ = flickrt.Owned({tag}, func(f []flickrt.Value) flickrt.Value {{\n\
                 \treturn &{ty}{{[{n}]flickrt.Value(f)}}\n}})\n",
            ),
            Held::Record(labels) => {
                let labels: Vec<String> = labels.iter().map(|l| names::string(l)).collect();
                let labels = labels.join(", ");
                write!(
                    go,
                    "\ntype {ty} struct {{\n\trec flickrt.Record\n\tf   [{n}]flickrt.Value\n}}\n\n\
                     var S_{ty} = flickrt.ShapeOf({labels})\n\n\
                     func (p *{ty}) Fields() []flickrt.Value {{ return []flickrt.Value{{&p.rec}} }}\n\n\
                     func {make}({params} flickrt.Value) flickrt.Value {{\n\
                     \tp := &{ty}{{f: [{n}]flickrt.Value{{{params}}}}}\n\
                     \tp.rec.Over(S_{ty}, p.f[:])\n\treturn p\n}}\n\n\
                     func With_{ty}(r flickrt.Value) *{ty} {{\n\tp := &{ty}{{}}\n\
                     \tS_{ty}.Values(r, p.f[:])\n\tp.rec.Over(S_{ty}, p.f[:])\n\treturn p\n}}\n\n\
                     func (p *{ty}) Set(i int, v flickrt.Value) *{ty} {{\n\tp.f[i] = v\n\treturn p\n}}\n\n\
                     func {of}(r flickrt.Value) flickrt.Value {{ return With_{ty}(r) }}\n\n\
                     var _ = flickrt.Owned({tag}, func(f []flickrt.Value) flickrt.Value {{ return {of}(f[0]) }})\n",
                )
            }
        };
        let _ = write!(
            go,
            "\nfunc (p *{ty}) TagName() string {{ return {tag} }}\n\n\
             func {test}(v flickrt.Value) bool {{\n\t_, ok := v.(*{ty})\n\treturn ok\n}}\n",
        );
    }
    go
}
