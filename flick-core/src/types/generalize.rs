//! Generalization: a binding built
//! purely becomes polymorphic over the variables that are not free in its
//! environment; the result of a runtime call stays monomorphic, even of `->{}`.

use std::collections::HashMap;

use super::bindings::LetParts;
use super::classes::Outcome;
use super::kinds::Kind;
use super::ty::{Binder, Type};
use super::typing::passes_dictionary;
use super::unify::Store;
use super::{Checker, Domain};
use crate::diagnostics::codes;
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
            self.polymorphic_cell(Some(name), scheme, self.span(body));
        }
    }

    /// Quantifies the variables of `ty` that are deeper than the current level.
    /// `operands` counts the operators and the `==` written before the
    /// binding: their operands were lowered by the bindings they are in, or
    /// are of none the binding can reach.
    pub fn generalize(&mut self, ty: &Type, operands: (usize, usize)) -> Type {
        self.settle_rows();
        // a variable that an unsolved class constraint mentions stays monomorphic
        self.solve_classes(false);
        for w in self.classes.wanted.clone() {
            self.store.lower(&w.constraint);
        }
        // A pending operator keeps its operand monomorphic while settling
        // an overload or an Ord constraint. List concatenation depends only
        // on the List constructor, so its element type can be generalized.
        for op in self.cx.operators.iter().skip(operands.0) {
            if op.domain == Domain::Concat
                && self.borrowed_whnf(&op.ty).spine().0.is_lang(Lang::List)
            {
                continue;
            }
            keep_operand(
                &mut self.store,
                &mut self.cx.kept_operands,
                &op.ty,
                op.span,
                op.op,
            );
        }
        // Equality uses only the operand's head to choose identity or
        // structural comparison. A pending row projection can still change it.
        for (ty, span, op, _) in self.equalities.iter().skip(operands.1) {
            let operand = self.borrowed_whnf(ty);
            let (head, _) = operand.spine();
            let kept = match head {
                Type::Meta(_) => head.clone(),
                _ if head.is_lang(Lang::Errors) || head.is_lang(Lang::ServiceImpl) => {
                    operand.into_owned()
                }
                _ => continue,
            };
            keep_operand(
                &mut self.store,
                &mut self.cx.kept_operands,
                &kept,
                *span,
                op,
            );
        }
        // a value a pending hold still checks stays monomorphic: its unknown
        // rows, once quantified, would count as variables it captures
        for hold in &self.cx.regions.holds {
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
        let mut kinds = kinds_of(&ty, &|h| param_kinds(self, h));
        let mut names = self.free_names(&ty);
        let mut vars = Vec::new();
        for m in metas {
            let name = names.next().expect("names are endless");
            vars.push(Binder {
                name: name.clone(),
                kind: kinds.remove(&m).unwrap_or(Kind::Type),
            });
            self.store.set(m, Type::Var(name));
        }
        Type::Forall {
            vars,
            constraints: Vec::new(),
            body: Box::new(self.zonk(&ty)),
        }
    }

    /// `ty` as a signature to write: every unknown named and the classes still
    /// required of those unknowns as constraints. Show it with
    /// [`Checker::show_written`] to qualify names as the module can write them.
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
        let settled: Vec<(u32, Lang)> = (self.cx.operators.iter())
            .filter_map(|op| match self.zonk(&op.ty) {
                Type::Meta(meta) => meta
                    .inference_type_root()
                    .filter(|m| !self.store.is_numeric(*m))
                    .map(|m| (m, op.domain.names()[0])),
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
        let substituted = |t: &Type| replace_metas(t, &subst);
        // a capture row cannot name a type variable, and the written signature
        // implies its projections through the stages' parameters
        let kinds = if subst.iter().any(|(_, value)| matches!(value, Type::Var(_))) {
            kinds_of(&ty, &|h| param_kinds(self, h))
        } else {
            HashMap::new()
        };
        let types: Vec<String> = subst
            .iter()
            .filter_map(|(m, v)| match v {
                Type::Var(name) if !kinds.contains_key(m) => Some(name.clone()),
                _ => None,
            })
            .collect();
        let names: Vec<&str> = types.iter().map(String::as_str).collect();
        let body = without_value_captures(&substituted(&ty), &names);
        if constraints.is_empty() {
            return body;
        }
        Type::Forall {
            vars: Vec::new(),
            constraints: constraints
                .iter()
                .map(|c| without_value_captures(&substituted(c), &names))
                .collect(),
            body: Box::new(body),
        }
    }

    /// Whether every pending operator will accept its operands once settled.
    pub fn operators_fit(&self) -> bool {
        self.cx
            .operators
            .iter()
            .all(|op| match self.zonk(&op.ty).spine().0 {
                Type::Meta(_) | Type::Error => true,
                head => op.domain.allows(head),
            })
    }

    /// Variable names not used in `ty` or by a variable in scope.
    fn free_names(&self, ty: &Type) -> impl Iterator<Item = String> + use<> {
        let mut taken: Vec<String> = self.cx.tyvars.iter().map(|v| v.name.clone()).collect();
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
            elided,
        } => {
            let ty = without_value_captures(ty, types);
            let row = without_value_row(captures, types);
            // it restated only variables: `^{}` would say it holds nothing
            if is_empty_row(&row) && !is_empty_row(captures) {
                return ty;
            }
            Type::Captured {
                ty: Box::new(ty),
                captures: Box::new(row),
                value: *value,
                elided: *elided,
            }
        }
        Type::Bounded { ty, captures } => Type::Bounded {
            ty: Box::new(without_value_captures(ty, types)),
            captures: Box::new(without_value_row(captures, types)),
        },
        other => other.map_children(&mut |c| without_value_captures(c, types)),
    }
}

fn is_empty_row(row: &Type) -> bool {
    matches!(row, Type::Row(r) if r.entries.is_empty() && r.tail.is_none())
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
            ..super::ty::Row::default()
        }),
        Type::Union(a, b) => Type::Union(
            Box::new(without_value_row(a, types)),
            Box::new(without_value_row(b, types)),
        ),
        other => other.clone(),
    }
}

/// Keeps `ty`, an operand type of the operator `op` at `span`, from being
/// generalized, noting each variable it keeps there.
fn keep_operand(
    store: &mut Store,
    kept: &mut Vec<(u32, Span, &'static str)>,
    ty: &Type,
    span: Span,
    op: &'static str,
) {
    kept.extend((store.deep_metas(&store.zonk(ty)).into_iter()).map(|m| (m, span, op)));
    store.lower(ty);
}

/// The unknowns of `t`, in order of appearance.
pub(super) fn metas_of(t: &Type) -> Vec<u32> {
    let mut metas = Vec::new();
    t.visit(&mut |t| {
        if let Some(m) = t.inference_meta_root()
            && !metas.contains(&m)
        {
            metas.push(m);
        }
    });
    metas
}

pub(super) fn replace_metas(t: &Type, subst: &[(u32, Type)]) -> Type {
    match t {
        Type::Meta(meta) if meta.inference_type_root().is_some() => subst
            .iter()
            .find(|(x, _)| Some(*x) == meta.inference_type_root())
            .map_or_else(|| t.clone(), |(_, v)| v.clone()),
        _ => t.map_children(&mut |c| replace_metas(c, subst)),
    }
}

/// The kinds of the parameters of a type constructor, by its definition.
fn param_kinds(checker: &Checker<'_>, head: &Type) -> Vec<Kind> {
    let Type::Con { module, name } = head else {
        return Vec::new();
    };
    let mut kinds = Vec::new();
    let mut kind = checker.env.type_def(module, name).map(|d| &d.kind);
    while let Some(Kind::Arrow(param, result)) = kind {
        kinds.push((**param).clone());
        kind = Some(result);
    }
    kinds
}

/// Records kind evidence in the original preorder, keeping the last kind
/// assigned to each root. Roots without evidence have kind `Type`.
fn kinds_of(ty: &Type, params: &dyn Fn(&Type) -> Vec<Kind>) -> HashMap<u32, Kind> {
    let mut kinds = HashMap::new();
    let mut remember = |t: &Type, kind: Kind| {
        row_metas(t, &mut |root| {
            if kind == Kind::Type {
                kinds.remove(&root);
            } else {
                kinds.insert(root, kind.clone());
            }
        });
    };
    ty.visit(&mut |t| match t {
        Type::Record(row) => {
            if let Some(tail) = row.tail.as_deref() {
                remember(tail, Kind::Row(Box::new(Kind::Type)));
            }
        }
        Type::Captured { captures, .. } | Type::Bounded { captures, .. } => {
            remember(captures, Kind::CaptureRow);
        }
        Type::Fun {
            effects: Some(e), ..
        } => {
            // At this arrow a root shared by both rows is a service root.
            remember(&e.errors, Kind::ErrorRow);
            remember(&e.services, Kind::ServiceRow);
            if let Some(provides) = &e.provides {
                remember(provides, Kind::ServiceRow);
            }
        }
        Type::App { .. } => {
            let (head, args) = t.spine();
            let rows: Vec<Kind> = if head.is_lang(Lang::Eff) {
                vec![Kind::ServiceRow, Kind::ErrorRow]
            } else if head.is_lang(Lang::Layer) {
                vec![Kind::ServiceRow, Kind::ErrorRow, Kind::ServiceRow]
            } else {
                params(head)
            };
            for (arg, row) in args.iter().zip(rows) {
                if !matches!(row, Kind::Meta(_)) {
                    remember(arg, row);
                }
            }
        }
        _ => {}
    });
    kinds
}

/// The root of an argument or leaves of its row expression. The types of
/// row entries are visited separately as children of the enclosing type.
fn row_metas(ty: &Type, f: &mut impl FnMut(u32)) {
    match ty {
        Type::Meta(meta) => {
            if let Some(root) = meta.inference_type_root() {
                f(root);
            }
        }
        Type::Row(row) => {
            if let Some(tail) = row.tail.as_deref() {
                row_metas(tail, f);
            }
        }
        Type::Union(a, b) => {
            row_metas(a, f);
            row_metas(b, f);
        }
        Type::Without { row, .. } => row_metas(row, f),
        _ => {}
    }
}

#[cfg(test)]
mod tests;
