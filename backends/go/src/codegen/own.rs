//! The Go types of constructors of the program's own types. A tagged
//! constructor without retained dictionaries that only compiled code makes has
//! a Go type of its own: a zero-size `struct{}` when it has no fields (so
//! constructing one allocates nothing), or a struct holding its fields. Its
//! values are that type and nothing else, so a `match` asks Go for the type
//! and reads a field where it is, with no tag to compare and no list of
//! fields to find. The runtime reads such a value as `flickrt.Constructed`
//! and builds one, where it decodes or generates, through `flickrt.Owned`.
//!
//! A constructor declared over a record, `Node { left : Tree; key : Int }`,
//! holds that record in its value: the header of a `flickrt.Record` over
//! the values beside it, so the record a `match` binds is a pointer into
//! the value and one object is made, not a constructor and a record. The
//! value keeps its list of one field too, for the runtime to read with no
//! list made at every question. Its
//! labels are in the order of the declaration, whatever order a literal
//! writes them in.
//!
//! The types are one file of the program: a constructor is used in many
//! modules and declared, in Go, once.

use std::collections::{BTreeMap, HashMap};
use std::fmt::{self, Write};

use flick_backend_support::Known;
use flick_pretty::{Document, docvec};

use super::arith::Native;
use super::layout::{Code, call};
use super::{Gen, names};
use flick_ir::{Constructor, Rep};
use flick_ir::{Expr, ExprKind, Module, Pat};

/// How a value of a constructor with a Go type of its own holds its fields.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum Held {
    /// No fields: a zero-size value `T{}`.
    Zero,
    /// Fields in declaration order: `Some(native)` is stored unboxed in its
    /// Go scalar type; `None` stays `flickrt.Value`. When every slot is `None`,
    /// the struct keeps `[N]flickrt.Value` for zero-copy `Fields()`.
    Fields(Vec<Option<Native>>),
    /// One field: a record of these labels, which lies in the value.
    Record(Vec<String>),
}

impl Held {
    /// Whether this constructor unboxes at least one field into a native Go
    /// scalar type (`int32`, `float64`, `bool`, or `string`).
    pub(super) fn has_scalars(&self) -> bool {
        matches!(self, Held::Fields(slots) if slots.iter().any(Option::is_some))
    }

    /// The native Go scalar type of field `i`, if it is unboxed.
    pub(super) fn field_native(&self, i: usize) -> Option<Native> {
        match self {
            Held::Fields(slots) => slots.get(i).copied().flatten(),
            Held::Zero | Held::Record(_) => None,
        }
    }
}

/// The Go types a module's code names, each with its tag and its fields.
pub(super) type Owned = BTreeMap<String, (String, Held)>;

/// The constructors declared over a record, by module and name, with the
/// labels of the record in canonical sorted order.
pub(super) type Records = HashMap<(String, String), Vec<String>>;

/// Declaration-site scalar eligibility of each constructor's fields, keyed by
/// `(module, type_name, constructor_name)`. Derived from `Module.types` so
/// polymorphic fields remain `None` (`flickrt.Value`) across all uses.
pub(super) type CtorFields = HashMap<(String, String, String), Vec<Option<Native>>>;

pub(super) fn fields(modules: &[&Module], lang: &Known) -> CtorFields {
    let mut out = CtorFields::new();
    for m in modules {
        for decl in &m.types {
            for ctor in &decl.constructors {
                let slots: Vec<Option<Native>> = ctor
                    .fields
                    .iter()
                    .map(|ty| Native::of(lang, Some(ty)))
                    .collect();
                out.insert(
                    (m.name.clone(), decl.name.clone(), ctor.name.clone()),
                    slots,
                );
            }
        }
    }
    out
}

pub(super) fn records(modules: &[&Module]) -> Records {
    let of = |m: &Module| {
        let named = |(c, labels): (&String, &Vec<String>)| {
            let mut sorted = labels.clone();
            sorted.sort_by(|a, b| super::once::canonical_key_cmp(a, b));
            ((m.name.clone(), c.clone()), sorted)
        };
        m.records.iter().map(named).collect::<Vec<_>>()
    };
    modules.iter().flat_map(|m| of(m)).collect()
}

impl<'a> Gen<'a> {
    /// How `c` holds its fields when it has a Go type of its own, without
    /// registering the type in `self.owned`.
    pub(super) fn own_held(&self, c: &Constructor) -> Option<Held> {
        let own = self.input.own_module(&c.module).is_some();
        if c.rep != Rep::Tagged || c.dicts > 0 || !own {
            return None;
        }
        if c.arity == 0 {
            return Some(Held::Zero);
        }
        let labels = self
            .records
            .and_then(|records| records.get(&(c.module.clone(), c.name.clone())));
        if let Some(labels) = labels
            && c.arity == 1
            && !labels.is_empty()
        {
            return Some(Held::Record(labels.clone()));
        }
        let slots = self
            .fields
            .and_then(|fields| fields.get(&(c.module.clone(), c.ty.clone(), c.name.clone())))
            .filter(|slots| slots.len() == c.arity)
            .cloned()
            .unwrap_or_else(|| vec![None; c.arity]);
        Some(Held::Fields(slots))
    }

    /// The Go type of the values of `c` and how it holds their fields, when
    /// they have a type of their own: a tagged constructor of a module of
    /// the program that holds no dictionary. A constructor of a built-in
    /// module, or one that holds a dictionary, stays a `*flickrt.Data`.
    pub(super) fn own_type(&mut self, c: &Constructor) -> Option<(String, Held)> {
        let held = self.own_held(c)?;
        let ty = names::own_type(&c.module, &c.name);
        if let Some((_, existing)) = self.owned.get(&ty) {
            return Some((ty, existing.clone()));
        }
        let tag = names::string(&self.tag(c));
        self.owned.insert(ty.clone(), (tag, held.clone()));
        Some((ty, held))
    }

    /// Bindings of `pat` that are guaranteed to be extracted with a native Go
    /// scalar type across every branch of `pat`.
    pub(super) fn pat_natives<'p>(&self, pat: &'p Pat) -> Vec<(&'p str, Native)> {
        fn collect<'p>(
            g: &Gen<'_>,
            pat: &'p Pat,
            native: Option<Native>,
            out: &mut Vec<(&'p str, Native)>,
        ) {
            match pat {
                Pat::Wild | Pat::Lit(_) => {}
                Pat::Bind(name) => {
                    if let Some(n) = native {
                        out.push((name.as_str(), n));
                    }
                }
                Pat::Tuple(items) => {
                    for item in items {
                        collect(g, item, None, out);
                    }
                }
                Pat::List(items, rest) => {
                    for item in items {
                        collect(g, item, None, out);
                    }
                    if let Some(rest) = rest {
                        collect(g, rest, None, out);
                    }
                }
                Pat::Record(fields) => {
                    for (_, p) in fields {
                        collect(g, p, None, out);
                    }
                }
                Pat::Con(c, args) => {
                    let held = (c.rep == Rep::Tagged)
                        .then(|| g.own_held(c))
                        .flatten()
                        .filter(Held::has_scalars);
                    for (i, arg) in args.iter().enumerate() {
                        let slot = held.as_ref().and_then(|h| h.field_native(i));
                        collect(g, arg, slot, out);
                    }
                }
                Pat::Or(alts) => {
                    let mut iter = alts.iter();
                    let Some(first) = iter.next() else {
                        return;
                    };
                    let mut common = Vec::new();
                    collect(g, first, native, &mut common);
                    for alt in iter {
                        let mut other = Vec::new();
                        collect(g, alt, native, &mut other);
                        common.retain(|pair| other.contains(pair));
                    }
                    out.extend(common);
                }
            }
        }
        let mut out = Vec::new();
        collect(self, pat, None, &mut out);
        out
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
            None if label == "_tag" => call(a, "flickrt.TagField", [r.doc]).into(),
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
        let test = test(ty);
        if matches!(held, Held::Zero) {
            let _ = write!(
                go,
                "\ntype {ty} struct{{}}\n\n\
                 func ({ty}) TagName() string {{ return {tag} }}\n\n\
                 func ({ty}) Fields() []flickrt.Value {{ return nil }}\n\n\
                 func {test}(v flickrt.Value) bool {{\n\t_, ok := v.({ty})\n\treturn ok\n}}\n\n\
                 var _ = flickrt.Owned({tag}, func([]flickrt.Value) flickrt.Value {{\n\
                 \treturn {ty}{{}}\n}})\n",
            );
            continue;
        }
        let n = match held {
            Held::Zero => unreachable!("handled above"),
            Held::Fields(slots) => slots.len(),
            Held::Record(labels) => labels.len(),
        };
        let (make, of) = (make(ty), of(ty));
        let _ = match held {
            Held::Zero => unreachable!("handled above"),
            Held::Fields(slots) if slots.iter().any(Option::is_some) => {
                write_scalar_struct(&mut go, ty, tag, &make, slots)
            }
            Held::Fields(_) => {
                let params: Vec<String> = (0..n).map(|i| format!("f{i}")).collect();
                let params = params.join(", ");
                write!(
                    go,
                    "\ntype {ty} struct{{ f [{n}]flickrt.Value }}\n\n\
                     func (p *{ty}) Fields() []flickrt.Value {{ return p.f[:] }}\n\n\
                     func {make}({params} flickrt.Value) flickrt.Value {{\n\
                     \treturn &{ty}{{[{n}]flickrt.Value{{{params}}}}}\n}}\n\n\
                     var _ = flickrt.Owned({tag}, func(f []flickrt.Value) flickrt.Value {{\n\
                     \treturn &{ty}{{[{n}]flickrt.Value(f)}}\n}})\n",
                )
            }
            Held::Record(labels) => {
                let params: Vec<String> = (0..n).map(|i| format!("f{i}")).collect();
                let params = params.join(", ");
                let labels: Vec<String> = labels.iter().map(|l| names::string(l)).collect();
                let labels = labels.join(", ");
                write!(
                    go,
                    "\ntype {ty} struct {{\n\trec  flickrt.Record\n\tf    [{n}]flickrt.Value\n\tself [1]flickrt.Value\n}}\n\n\
                     var S_{ty} = flickrt.ShapeOf({labels})\n\n\
                     func (p *{ty}) Fields() []flickrt.Value {{ return p.self[:] }}\n\n\
                     func {make}({params} flickrt.Value) flickrt.Value {{\n\
                     \tp := &{ty}{{f: [{n}]flickrt.Value{{{params}}}}}\n\
                     \tp.rec.Over(S_{ty}, p.f[:])\n\tp.self[0] = &p.rec\n\treturn p\n}}\n\n\
                     func With_{ty}(r flickrt.Value) *{ty} {{\n\tp := &{ty}{{}}\n\
                     \tS_{ty}.Values(r, p.f[:])\n\tp.rec.Over(S_{ty}, p.f[:])\n\tp.self[0] = &p.rec\n\treturn p\n}}\n\n\
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

/// Struct field packing order: pointer-bearing fields first to minimize Go GC
/// `ptrdata`, then scalar fields by descending alignment (`8`, `4`, `2`, `1` bytes).
fn layout_rank(slot: Option<Native>) -> u8 {
    match slot {
        None => 0,
        Some(Native::String) => 1,
        Some(Native::Float | Native::Int64 | Native::UInt64) => 2,
        Some(Native::Int | Native::UInt32 | Native::Float32) => 3,
        Some(Native::Int16 | Native::UInt16) => 4,
        Some(Native::Int8 | Native::UInt8 | Native::Bool) => 5,
    }
}

fn write_scalar_struct(
    go: &mut String,
    ty: &str,
    tag: &str,
    make: &str,
    slots: &[Option<Native>],
) -> fmt::Result {
    let mut order: Vec<usize> = (0..slots.len()).collect();
    order.sort_by_key(|&i| (layout_rank(slots[i]), i));
    writeln!(go, "\ntype {ty} struct {{")?;
    for i in order {
        let go_ty = slots[i].map_or("flickrt.Value", Native::go);
        writeln!(go, "\tf{i} {go_ty}")?;
    }
    writeln!(go, "}}")?;

    let fields_list: Vec<String> = (0..slots.len()).map(|i| format!("p.f{i}")).collect();
    writeln!(
        go,
        "\nfunc (p *{ty}) Fields() []flickrt.Value {{ return []flickrt.Value{{{}}} }}",
        fields_list.join(", ")
    )?;

    let params: Vec<String> = slots
        .iter()
        .enumerate()
        .map(|(i, slot)| format!("f{i} {}", slot.map_or("flickrt.Value", Native::go)))
        .collect();
    let inits: Vec<String> = (0..slots.len()).map(|i| format!("f{i}: f{i}")).collect();
    writeln!(
        go,
        "\nfunc {make}({}) flickrt.Value {{\n\treturn &{ty}{{{}}}\n}}",
        params.join(", "),
        inits.join(", ")
    )?;

    let eqs: Vec<String> = slots
        .iter()
        .enumerate()
        .map(|(i, slot)| match slot {
            Some(Native::Float) => format!("flickrt.EqualFloat(p.f{i}, q.f{i}, strictZero)"),
            Some(Native::Float32) => {
                format!("flickrt.EqualFloat(float64(p.f{i}), float64(q.f{i}), strictZero)")
            }
            Some(
                Native::Int
                | Native::Int8
                | Native::Int16
                | Native::Int64
                | Native::UInt8
                | Native::UInt16
                | Native::UInt32
                | Native::UInt64
                | Native::Bool
                | Native::String,
            ) => format!("p.f{i} == q.f{i}"),
            None => format!("flickrt.EqualWith(p.f{i}, q.f{i}, strictZero)"),
        })
        .collect();
    writeln!(
        go,
        "\nfunc (p *{ty}) EqualConstructed(other flickrt.Value, strictZero bool) (bool, bool) {{\n\
         \tq, ok := other.(*{ty})\n\
         \tif !ok {{\n\t\treturn false, false\n\t}}\n\
         \treturn {}, true\n}}",
        eqs.join(" && ")
    )?;

    let owned_inits: Vec<String> = slots
        .iter()
        .enumerate()
        .map(|(i, slot)| match slot {
            Some(native) => format!("f{i}: flickrt.As{}(f[{i}])", native.name()),
            None => format!("f{i}: f[{i}]"),
        })
        .collect();
    writeln!(
        go,
        "\nvar _ = flickrt.Owned({tag}, func(f []flickrt.Value) flickrt.Value {{\n\
         \treturn &{ty}{{{}}}\n}})",
        owned_inits.join(", ")
    )
}
