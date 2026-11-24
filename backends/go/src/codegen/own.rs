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
//! holds the closed record shape's Go struct directly by value inside its
//! constructor struct, so scalar fields stay unboxed and only one object is
//! allocated for the constructor value.
//!
//! The types are one file of the program: a constructor is used in many
//! modules and declared, in Go, once.

use std::collections::{BTreeMap, HashMap};
use std::fmt::{self, Write};

use flick_backend_support::Known;

use super::arith::Native;
use super::go_type::TypeDecls;
use super::record::{RecordShape, RecordSlot, equal_with_expr, layout_rank};
use super::{Gen, names};
use flick_ir::{Constructor, Module, Rep};

/// How a value of a constructor with a Go type of its own holds its fields.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum Held {
    /// No fields: a zero-size value `T{}`.
    Zero,
    /// Fields in declaration order: `Some(native)` is stored unboxed in its
    /// Go scalar type; `None` stays `flickrt.Value`. When every slot is `None`,
    /// the struct keeps `[N]flickrt.Value` for zero-copy `Fields()`.
    Fields(Vec<Option<Native>>),
    /// One field: a closed record held by value as its Go struct `R_...`.
    Record(RecordShape),
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
/// closed record shape they hold by value.
pub(super) type Records = HashMap<(String, String), RecordShape>;

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

pub(super) fn records(modules: &[&Module], lang: &Known, decls: &TypeDecls) -> Records {
    let mut out = Records::new();
    for m in modules {
        for decl in &m.types {
            for ctor in &decl.constructors {
                if let [field_ty] = ctor.fields.as_slice()
                    && let Some(shape) = RecordShape::for_ctor_record(lang, decls, field_ty)
                {
                    out.insert((m.name.clone(), ctor.name.clone()), shape);
                }
            }
        }
        for (c, labels) in &m.records {
            if !labels.is_empty() {
                out.entry((m.name.clone(), c.clone()))
                    .or_insert_with(|| RecordShape::from_labels(labels));
            }
        }
    }
    out
}

impl Gen<'_> {
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
        let shape = self
            .records
            .and_then(|records| records.get(&(c.module.clone(), c.name.clone())));
        if let Some(shape) = shape
            && c.arity == 1
        {
            return Some(Held::Record(shape.clone()));
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
    /// they have a type of its own: a tagged constructor of a module of
    /// the program that holds no dictionary. A constructor of a built-in
    /// module, or one that holds a dictionary, stays a `*flickrt.Data`.
    pub(super) fn own_type(&mut self, c: &Constructor) -> Option<(String, Held)> {
        let held = self.own_held(c)?;
        let ty = names::own_type(&c.module, &c.name);
        if let Some((_, existing)) = self.owned.get(&ty) {
            if let Held::Record(shape) = existing {
                self.record_shapes.insert(shape.clone());
            }
            return Some((ty, existing.clone()));
        }
        if let Held::Record(ref shape) = held {
            self.record_shapes.insert(shape.clone());
        }
        let tag = names::string(&self.tag(c));
        self.owned.insert(ty.clone(), (tag, held.clone()));
        Some((ty, held))
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
        let (make, of) = (make(ty), of(ty));
        let _ = match held {
            Held::Zero => unreachable!("handled above"),
            Held::Fields(slots) if slots.iter().any(Option::is_some) => {
                write_scalar_struct(&mut go, ty, tag, &make, slots)
            }
            Held::Fields(slots) => {
                let n = slots.len();
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
            Held::Record(shape) => write_record_ctor(&mut go, ty, tag, &make, &of, shape),
        };
        let _ = write!(
            go,
            "\nfunc (p *{ty}) TagName() string {{ return {tag} }}\n\n\
             func {test}(v flickrt.Value) bool {{\n\t_, ok := v.(*{ty})\n\treturn ok\n}}\n",
        );
    }
    go
}

fn write_record_ctor(
    go: &mut String,
    ty: &str,
    tag: &str,
    make: &str,
    of: &str,
    shape: &RecordShape,
) -> fmt::Result {
    let rec_ty = shape.go_type();
    let as_fn = shape.unbox_fn();
    let eqs: Vec<String> = shape
        .fields
        .iter()
        .enumerate()
        .map(|(i, (_, slot))| equal_with_expr(&format!("p.rec.f{i}"), &format!("q.rec.f{i}"), slot))
        .collect();
    write!(
        go,
        "\ntype {ty} struct {{\n\trec {rec_ty}\n}}\n\n\
         func (p *{ty}) Fields() []flickrt.Value {{ return []flickrt.Value{{p.rec}} }}\n\n\
         func {make}(rec {rec_ty}) flickrt.Value {{\n\treturn &{ty}{{rec: rec}}\n}}\n\n\
         func {of}(r flickrt.Value) flickrt.Value {{\n\treturn &{ty}{{rec: {as_fn}(r)}}\n}}\n\n\
         func (p *{ty}) EqualConstructed(other flickrt.Value, strictZero bool) (bool, bool) {{\n\
         \tq, ok := other.(*{ty})\n\
         \tif !ok {{\n\t\treturn false, false\n\t}}\n\
         \treturn {}, true\n}}\n\n\
         var _ = flickrt.Owned({tag}, func(f []flickrt.Value) flickrt.Value {{ return {of}(f[0]) }})\n",
        eqs.join(" && ")
    )
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
        .map(|(i, slot)| {
            let rslot = slot.map_or(RecordSlot::Value, RecordSlot::Scalar);
            equal_with_expr(&format!("p.f{i}"), &format!("q.f{i}"), &rslot)
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
