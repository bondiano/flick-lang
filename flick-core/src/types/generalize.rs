//! Generalization: a binding built
//! purely becomes polymorphic over the variables that are not free in its
//! environment; the result of a runtime call stays monomorphic, even of `->{}`.

use super::Checker;
use super::bindings::LetParts;
use super::classes::Outcome;
use super::env::Found;
use super::kinds::Kind;
use super::ty::{Binder, Type};
use super::typing::passes_dictionary;
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::lang::Lang;

impl Checker<'_> {
    /// A polymorphic type over a value computed by a runtime call is
    /// rejected: each use would share one result at different types.
    pub fn polymorphic_runtime(
        &mut self,
        name: Option<&str>,
        scheme: &Type,
        runtime: bool,
        span: Span,
    ) {
        let Type::Forall {
            vars, constraints, ..
        } = scheme
        else {
            return;
        };
        // a dictionary makes the value a function of it, run again at each use
        let dictionaries = constraints.iter().any(passes_dictionary);
        if runtime && (!vars.is_empty() || dictionaries) {
            let shown = self.show(scheme);
            let (what, value) = match name {
                Some(name) => (format!("`{name}`"), "its value"),
                None => ("this".to_owned(), "it"),
            };
            let kind = if vars.is_empty() {
                "constrained"
            } else {
                "polymorphic"
            };
            self.error(
                codes::T013,
                span,
                format!(
                    "{what} is computed by a runtime call, so {value} cannot have the {kind} type `{shown}`; make it a function or build it with `task`"
                ),
            );
        }
    }

    /// What a signature of `name` may not quantify: the result of a runtime
    /// call, or an atom held by a value.
    pub fn polymorphic_signature(
        &mut self,
        name: &str,
        scheme: &Type,
        runtime: bool,
        decl: &Node,
        parts: &LetParts,
    ) {
        self.polymorphic_runtime(Some(name), scheme, runtime, self.span(decl));
        if let (false, false, Some(body)) = (runtime, parts.is_function(), parts.body) {
            self.polymorphic_atom(Some(name), scheme, self.span(body));
        }
    }

    /// Quantifies the variables of `ty` that are deeper than the current level.
    pub fn generalize(&mut self, ty: &Type) -> Type {
        self.settle_rows();
        // a variable that an unsolved class constraint mentions stays monomorphic
        self.solve_classes(false);
        for w in self.classes.wanted.clone() {
            self.store.lower(&w.constraint);
        }
        // a value a pending hold still checks stays monomorphic: its unknown
        // rows, once quantified, would count as variables it captures
        for hold in &self.regions.holds {
            for (_, held) in &hold.held {
                self.store.lower(held);
            }
        }
        let ty = self.zonk(ty);
        // an integer literal's type stays monomorphic: settling it picks one type
        let mut metas = self.store.deep_metas(&ty);
        metas.retain(|m| !self.store.is_numeric(*m));
        if metas.is_empty() {
            return ty;
        }
        let mut names = self.free_names(&ty);
        let mut vars = Vec::new();
        for m in metas {
            let name = names.next().expect("names are endless");
            vars.push(Binder {
                name: name.clone(),
                kind: kind_of(&ty, m),
            });
            self.store.set(m, Type::Var(name));
        }
        Type::Forall {
            vars,
            constraints: Vec::new(),
            body: Box::new(self.zonk(&ty)),
        }
    }

    /// `ty` as a signature to write: every unknown named, the classes still
    /// required of those unknowns as constraints, and every type by the
    /// name the module can write it with.
    pub fn signature_of(&mut self, ty: &Type) -> Type {
        let ty = self.zonk(ty);
        let metas = metas_of(&ty);
        let mut constraints: Vec<Type> = Vec::new();
        for w in self.classes.wanted.clone() {
            let c = self.zonk(&w.constraint);
            let own = metas_of(&c);
            if own.is_empty() || !own.iter().all(|m| metas.contains(m)) || constraints.contains(&c)
            {
                continue;
            }
            if !matches!(self.entails(&c, &w.givens), Outcome::Solved(_)) {
                constraints.push(c);
            }
        }
        // an operand of an arithmetic operator nothing determined becomes
        // the operator's first allowed type once the declaration is settled
        // a number literal's type may still be settled otherwise, so it stays open
        let settled: Vec<(u32, Lang)> = (self.operators.iter())
            .filter_map(|op| match self.zonk(&op.ty) {
                Type::Meta(m) if !self.store.is_numeric(m) => Some((m, op.domain.names()[0])),
                _ => None,
            })
            .collect();
        let mut names = self.free_names(&ty);
        let subst: Vec<(u32, Type)> = metas
            .into_iter()
            .map(|m| match settled.iter().find(|(x, _)| *x == m) {
                Some((_, item)) => (m, Type::lang(*item)),
                None => (m, Type::Var(names.next().expect("names are endless"))),
            })
            .collect();
        let written = |t: &Type| self.written(&replace_metas(t, &subst));
        // a capture row cannot name a type variable, and the written signature
        // implies its projections through the stages' parameters
        let types: Vec<String> = subst
            .iter()
            .filter(|(m, _)| matches!(kind_of(&ty, *m), Kind::Type))
            .filter_map(|(_, v)| match v {
                Type::Var(name) => Some(name.clone()),
                _ => None,
            })
            .collect();
        let names: Vec<&str> = types.iter().map(String::as_str).collect();
        let body = without_value_captures(&written(&ty), &names);
        if constraints.is_empty() {
            return body;
        }
        Type::Forall {
            vars: Vec::new(),
            constraints: constraints
                .iter()
                .map(|c| without_value_captures(&written(c), &names))
                .collect(),
            body: Box::new(body),
        }
    }

    /// Whether every pending operator will accept its operands once settled.
    pub fn operators_fit(&self) -> bool {
        self.operators
            .iter()
            .all(|op| match self.zonk(&op.ty).spine().0 {
                Type::Meta(_) | Type::Error => true,
                head => op.domain.allows(head),
            })
    }

    /// `t` with each type not in scope by its plain name qualified the way
    /// the module's imports let it be written.
    fn written(&self, t: &Type) -> Type {
        match t {
            Type::Con { module, name } if !name.contains('.') => {
                let plain = matches!(
                    self.env.find(Namespace::Type, name),
                    Found::Entity(e) if e.module == *module
                );
                match self.env.qualifier_of(module) {
                    Some(q) if !plain => Type::con(module, &format!("{q}.{name}")),
                    _ => t.clone(),
                }
            }
            _ => t.map_children(&mut |c| self.written(c)),
        }
    }

    /// Variable names not used in `ty` or by a variable in scope.
    fn free_names(&self, ty: &Type) -> impl Iterator<Item = String> + use<> {
        let mut taken: Vec<String> = self.tyvars.iter().map(|v| v.name.clone()).collect();
        ty.visit(&mut |t| match t {
            Type::Var(name) => taken.push(name.clone()),
            Type::Skolem(s) => taken.push(self.store.skolem_name(*s).to_owned()),
            _ => {}
        });
        (0..).filter_map(move |i: u32| {
            let letter = char::from(b'a' + u8::try_from(i % 26).expect("< 26"));
            let name = if i < 26 {
                letter.to_string()
            } else {
                format!("{letter}{}", i / 26)
            };
            (!taken.contains(&name)).then_some(name)
        })
    }
}

/// Drops capture-row entries that only restate a type variable: a written
/// signature implies them and the source cannot name them.
pub fn without_value_captures(t: &Type, types: &[&str]) -> Type {
    match t {
        Type::Captured {
            ty,
            captures,
            value,
        } => Type::Captured {
            ty: Box::new(without_value_captures(ty, types)),
            captures: Box::new(without_value_row(captures, types)),
            value: *value,
        },
        Type::Bounded { ty, captures } => Type::Bounded {
            ty: Box::new(without_value_captures(ty, types)),
            captures: Box::new(without_value_row(captures, types)),
        },
        other => other.map_children(&mut |c| without_value_captures(c, types)),
    }
}

fn without_value_row(row: &Type, types: &[&str]) -> Type {
    let projection =
        |atom: &Type| matches!(atom, Type::Var(name) if types.contains(&name.as_str()));
    match row {
        Type::Row(r) => Type::Row(super::ty::Row {
            entries: r
                .entries
                .iter()
                .filter(|(_, atom)| !projection(atom))
                .cloned()
                .collect(),
            tail: r.tail.clone(),
        }),
        Type::Union(a, b) => Type::Union(
            Box::new(without_value_row(a, types)),
            Box::new(without_value_row(b, types)),
        ),
        other => other.clone(),
    }
}

/// The unknowns of `t`, in order of appearance.
fn metas_of(t: &Type) -> Vec<u32> {
    let mut metas = Vec::new();
    t.visit(&mut |t| {
        if let Type::Meta(m) = t
            && !metas.contains(m)
        {
            metas.push(*m);
        }
    });
    metas
}

pub(super) fn replace_metas(t: &Type, subst: &[(u32, Type)]) -> Type {
    match t {
        Type::Meta(m) => subst
            .iter()
            .find(|(x, _)| x == m)
            .map_or_else(|| t.clone(), |(_, v)| v.clone()),
        _ => t.map_children(&mut |c| replace_metas(c, subst)),
    }
}

/// The kind of a variable by where it occurs: a record tail is a field row,
/// the rows of a runtime arrow are service and error rows.
fn kind_of(ty: &Type, m: u32) -> Kind {
    fn is(t: &Type, m: u32) -> bool {
        match t {
            Type::Meta(x) => *x == m,
            Type::Row(row) => row.tail.as_deref().is_some_and(|t| is(t, m)),
            Type::Union(a, b) => is(a, m) || is(b, m),
            _ => false,
        }
    }
    let is = |t: &Type| is(t, m);
    let mut kind = Kind::Type;
    ty.visit(&mut |t| match t {
        Type::Record(row) if row.tail.as_deref().is_some_and(is) => {
            kind = Kind::Row(Box::new(Kind::Type));
        }
        Type::Captured { captures, .. } | Type::Bounded { captures, .. } if is(captures) => {
            kind = Kind::CaptureRow;
        }
        Type::Fun {
            effects: Some(e), ..
        } => {
            if is(&e.services) {
                kind = Kind::ServiceRow;
            } else if is(&e.errors) {
                kind = Kind::ErrorRow;
            }
        }
        Type::App { .. } => {
            let (head, args) = t.spine();
            let rows: &[Kind] = if head.is_lang(Lang::Eff) {
                &[Kind::ServiceRow, Kind::ErrorRow]
            } else if head.is_lang(Lang::Layer) {
                &[Kind::ServiceRow, Kind::ErrorRow, Kind::ServiceRow]
            } else {
                &[]
            };
            for (arg, row) in args.iter().zip(rows) {
                if is(arg) {
                    kind = row.clone();
                }
            }
        }
        _ => {}
    });
    kind
}
