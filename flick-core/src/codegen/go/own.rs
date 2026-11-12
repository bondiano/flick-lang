//! The Go types of constructors of the program's own types. A constructor
//! with fields that only compiled code makes has a Go type of its own: its
//! values are that type and nothing else, so a `match` asks Go for the type
//! and reads a field where it is, with no tag to compare and no list of
//! fields to find. The runtime reads such a value as `flickrt.Constructed`
//! and builds one, where it decodes or generates, through `flickrt.Owned`.
//!
//! The types are one file of the program: a constructor is used in many
//! modules and declared, in Go, once.

use std::collections::BTreeMap;
use std::fmt::Write;

use super::{Gen, names};
use crate::types::{Constructor, Rep};

/// The Go types a module's code names, each with its tag and how many
/// fields it has.
pub(super) type Owned = BTreeMap<String, (String, usize)>;

impl Gen<'_> {
    /// The Go type of the values of `c`, when they have one of their own:
    /// a tagged constructor with fields, of a module of the program, that
    /// holds no dictionary. One without fields stays the one `*flickrt.Data`
    /// of its tag, and a built-in type is made by the runtime too.
    pub(super) fn own_type(&mut self, c: &Constructor) -> Option<String> {
        let own = self.own?;
        if c.rep != Rep::Tagged || c.arity == 0 || c.dicts > 0 || !own(&c.module) {
            return None;
        }
        let ty = names::own_type(&c.module, &c.name);
        if !self.owned.contains_key(&ty) {
            let tag = names::string(&self.tag(c));
            self.owned.insert(ty.clone(), (tag, c.arity));
        }
        Some(ty)
    }
}

/// The function making a value of the Go type `ty` from its fields.
pub(super) fn make(ty: &str) -> String {
    format!("New_{ty}")
}

/// The function telling whether a value is of the Go type `ty`.
pub(super) fn test(ty: &str) -> String {
    format!("Is_{ty}")
}

/// The Go file declaring `types`.
pub(super) fn file(types: &Owned) -> String {
    let mut go = String::from("package main\n\nimport \"flick.app/flickrt\"\n");
    for (ty, (tag, arity)) in types {
        let params: Vec<String> = (0..*arity).map(|i| format!("f{i}")).collect();
        let params = params.join(", ");
        let _ = write!(
            go,
            "\ntype {ty} struct{{ f [{arity}]flickrt.Value }}\n\n\
             func (p *{ty}) TagName() string {{ return {tag} }}\n\n\
             func (p *{ty}) Fields() []flickrt.Value {{ return p.f[:] }}\n\n\
             func {make}({params} flickrt.Value) flickrt.Value {{\n\
             \treturn &{ty}{{[{arity}]flickrt.Value{{{params}}}}}\n}}\n\n\
             func {test}(v flickrt.Value) bool {{\n\t_, ok := v.(*{ty})\n\treturn ok\n}}\n\n\
             var _ = flickrt.Owned({tag}, func(f []flickrt.Value) flickrt.Value {{\n\
             \treturn &{ty}{{[{arity}]flickrt.Value(f)}}\n}})\n",
            make = make(ty),
            test = test(ty),
        );
    }
    go
}
