//! The row primitives of records. The compiler solves them from the
//! fields of known records; a program cannot declare instances of them.

use super::solve::{Givens, Outcome};
use super::{Dict, has_metas};
use crate::types::Checker;
use crate::types::lang::Lang;
use crate::types::ty::{Row, Type};

/// What is known about the rest of a record's fields.
enum Rest {
    /// It has no other fields.
    Closed,
    /// Its other fields are not inferred yet.
    Unknown,
    /// Its other fields are a row variable.
    Open,
}

fn rest(row: &Row) -> Rest {
    match row.tail.as_deref() {
        None => Rest::Closed,
        Some(Type::Meta(_)) => Rest::Unknown,
        Some(_) => Rest::Open,
    }
}

impl Checker<'_> {
    pub(super) fn primitive(
        &mut self,
        c: &Type,
        class: Lang,
        args: &[Type],
        givens: &Givens,
        chain: &mut Vec<Type>,
    ) -> Outcome {
        let refuse = |reason: String| Outcome::Missing(c.clone(), Some(reason));
        let unknown = || {
            if has_metas(c) {
                Outcome::Stuck
            } else {
                Outcome::Missing(c.clone(), None)
            }
        };
        // the record is open: the field may be in the part nobody knows yet
        let elsewhere = |row: &Row, what: String| match rest(row) {
            Rest::Unknown => Outcome::Stuck,
            Rest::Closed => Outcome::Missing(c.clone(), Some(what)),
            Rest::Open => Outcome::Missing(
                c.clone(),
                Some(format!("{what}, and its other fields are not known")),
            ),
        };
        let solved = |this: &Self| primitive(this.show(c), Vec::new());
        match (class, args) {
            (Lang::HasField, [Type::Symbol(label), Type::Record(row), field]) => {
                match row.get(label) {
                    Some(t) if self.unify(field, t).is_ok() => solved(self),
                    Some(t) => refuse(format!("field `{label}` has type `{}`", self.show(t))),
                    None => elsewhere(row, format!("the record has no field `{label}`")),
                }
            }
            (Lang::Lacks, [Type::Symbol(label), Type::Record(row)]) => {
                if row.get(label).is_some() {
                    return refuse(format!("the record already has field `{label}`"));
                }
                match rest(row) {
                    Rest::Closed => solved(self),
                    Rest::Unknown => Outcome::Stuck,
                    Rest::Open => refuse(format!(
                        "the record is open, so its other fields may include `{label}`"
                    )),
                }
            }
            (Lang::Remove, [Type::Symbol(label), Type::Record(row), out]) => {
                if row.get(label).is_none() {
                    return elsewhere(row, format!("the record has no field `{label}`"));
                }
                let entries = row
                    .entries
                    .iter()
                    .filter(|(l, _)| l != label)
                    .cloned()
                    .collect();
                let removed = Type::Record(Row {
                    entries,
                    tail: row.tail.clone(),
                    ..Row::default()
                });
                self.result(c, out, &removed)
            }
            (Lang::Project, [Type::Record(row), Type::Record(out)]) => {
                if matches!(rest(out), Rest::Unknown) {
                    return Outcome::Stuck;
                }
                for (label, t) in &out.entries {
                    match row.get(label) {
                        Some(field) if self.unify(t, field).is_ok() => {}
                        Some(field) => {
                            let shown = self.show(field);
                            return refuse(format!("field `{label}` has type `{shown}`"));
                        }
                        None => {
                            return elsewhere(row, format!("the record has no field `{label}`"));
                        }
                    }
                }
                // shortcut: an open `out` keeps only its known fields at run time
                let labels = out.entries.iter().map(|(l, _)| l.clone());
                primitive(self.show(c), labels.collect())
            }
            (Lang::UnionDisjoint, [Type::Record(a), Type::Record(b), out]) => {
                self.union_disjoint(c, (a, b), out, givens, chain)
            }
            (Lang::MapFields, [f, Type::Record(row), out]) => match rest(row) {
                Rest::Closed => {
                    let entries = row
                        .entries
                        .iter()
                        .map(|(l, t)| (l.clone(), Type::app(f.clone(), t.clone())))
                        .collect();
                    self.result(c, out, &Type::Record(Row::closed(entries)))
                }
                Rest::Unknown => Outcome::Stuck,
                Rest::Open => {
                    refuse("the record is open, so its other fields are not known".to_owned())
                }
            },
            (Lang::Labels, [Type::Record(row)]) => match rest(row) {
                Rest::Closed => solved(self),
                Rest::Unknown => Outcome::Stuck,
                Rest::Open => {
                    refuse("the record is open, so its other fields are not known".to_owned())
                }
            },
            (Lang::PathParams, [Type::Symbol(path), part]) => self.params_of_path(c, path, part),
            _ => unknown(),
        }
    }

    /// `PathParams path part`: the `:name` segments of `path` are the fields
    /// of the record `part`, or there are none and `part` is no record.
    fn params_of_path(&self, c: &Type, path: &str, part: &Type) -> Outcome {
        let refuse = |reason: String| Outcome::Missing(c.clone(), Some(reason));
        let fields = match self.whnf(part) {
            Type::Record(row) => match rest(&row) {
                Rest::Closed => row.entries.into_iter().map(|(l, _)| l).collect(),
                Rest::Unknown => return Outcome::Stuck,
                Rest::Open => {
                    return refuse(
                        "the record is open, so its other fields are not known".to_owned(),
                    );
                }
            },
            t if has_metas(&t) => return Outcome::Stuck,
            _ => Vec::new(),
        };
        match path_params(path, fields) {
            None => primitive(self.show(c), Vec::new()),
            Some(why) => refuse(why),
        }
    }

    /// `UnionDisjoint a b out`: no label twice, `out` has the fields of both.
    fn union_disjoint(
        &mut self,
        c: &Type,
        (a, b): (&Row, &Row),
        out: &Type,
        givens: &Givens,
        chain: &mut Vec<Type>,
    ) -> Outcome {
        if let Some((label, _)) = a.entries.iter().find(|(l, _)| b.get(l).is_some()) {
            return Outcome::Missing(
                c.clone(),
                Some(format!(
                    "both records have field `{label}`; labels of a record are unique"
                )),
            );
        }
        let entries: Vec<(String, Type)> = a.entries.iter().chain(&b.entries).cloned().collect();
        match (rest(a), rest(b)) {
            (Rest::Closed, Rest::Closed) => {
                self.result(c, out, &Type::Record(Row::closed(entries)))
            }
            (Rest::Unknown, _) | (_, Rest::Unknown) => Outcome::Stuck,
            (Rest::Open, Rest::Closed) => self.disjoint_open(c, a, b, entries, out, givens, chain),
            (Rest::Closed, Rest::Open) => self.disjoint_open(c, b, a, entries, out, givens, chain),
            (Rest::Open, Rest::Open) => Outcome::Missing(
                c.clone(),
                Some("both records are open, so their other fields may share labels".to_owned()),
            ),
        }
    }

    /// An open record and a closed one are disjoint when the open one lacks
    /// every label of the other, as assumed by the signature.
    #[allow(clippy::too_many_arguments)]
    fn disjoint_open(
        &mut self,
        c: &Type,
        open: &Row,
        closed: &Row,
        mut entries: Vec<(String, Type)>,
        out: &Type,
        givens: &Givens,
        chain: &mut Vec<Type>,
    ) -> Outcome {
        for (label, _) in &closed.entries {
            let lacks = Type::apps(
                Type::lang(Lang::Lacks),
                [Type::Symbol(label.clone()), Type::Record(open.clone())],
            );
            match self.entail(&lacks, givens, chain) {
                Outcome::Solved(_) => {}
                other => return other,
            }
        }
        entries.sort_by(|x, y| x.0.cmp(&y.0));
        let union = Type::Record(Row {
            entries,
            tail: open.tail.clone(),
            ..Row::default()
        });
        self.result(c, out, &union)
    }

    /// The determined record of a primitive must be `computed`.
    fn result(&mut self, c: &Type, out: &Type, computed: &Type) -> Outcome {
        if self.unify(out, computed).is_ok() {
            return primitive(self.show(c), Vec::new());
        }
        let shown = self.show(computed);
        Outcome::Missing(c.clone(), Some(format!("the result is `{shown}`")))
    }
}

/// Why the `:name` segments of `path` are not `fields`, if they are not.
fn path_params(path: &str, mut fields: Vec<String>) -> Option<String> {
    let mut params: Vec<&str> = (path.split('/'))
        .filter_map(|s| s.strip_prefix(':'))
        .collect();
    params.sort_unstable();
    fields.sort_unstable();
    if params == fields {
        return None;
    }
    let listed = |names: &[&str]| {
        if names.is_empty() {
            "none".to_owned()
        } else {
            names
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let fields: Vec<&str> = fields.iter().map(String::as_str).collect();
    Some(format!(
        "the path `{path}` has the parameters {}, and its `path` part the fields {}",
        listed(&params),
        listed(&fields)
    ))
}

fn primitive(constraint: String, labels: Vec<String>) -> Outcome {
    Outcome::Solved(Dict::Primitive { constraint, labels })
}
