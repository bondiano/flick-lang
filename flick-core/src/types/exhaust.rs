//! Exhaustiveness and redundancy of patterns, by the usefulness algorithm of
//! Maranget ("Warnings for pattern matching", 2007): a pattern is useful after
//! some rows when a value matches it but none of the rows. A `match` must
//! leave no value unmatched (the diagnostic shows one); an arm that is not
//! useful can never run. Guarded arms cover nothing, since the guard may fail.

use super::Checker;
use super::env::Form;
use super::patterns::{Ctor, Pat};
use super::ty::Type;
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::syntax::cst::Node;

impl Checker<'_> {
    pub fn exhaustive(&mut self, node: &Node, arms: &[(Pat, bool, Span)]) {
        let mut rows: Vec<Vec<Pat>> = Vec::new();
        for (pat, guarded, span) in arms {
            if self.useful(&rows, std::slice::from_ref(pat)).is_none() {
                self.warning(
                    codes::T010,
                    *span,
                    "this arm is never reached: the arms above match every value it matches",
                );
            }
            if !guarded {
                rows.push(vec![pat.clone()]);
            }
        }
        if let Some(witness) = self.useful(&rows, &[Pat::Wild]) {
            let shown = self.show_pat(&witness[0], false);
            let start = node.range().0;
            let end = node.nodes().next().map_or(start, |s| s.range().1);
            let span = Span::new(self.file, start, end);
            self.diagnostics.push(
                crate::diagnostics::Diagnostic::error(
                    codes::T009,
                    format!("`match` does not cover every value: `{shown}` is not matched"),
                    span,
                )
                .with_fix(format!("add an arm `| {shown} -> …`"), Vec::new()),
            );
        }
    }

    /// A pattern of `let` or of a parameter must match every value.
    pub fn irrefutable(&mut self, pat: &Pat, node: &Node) {
        if *pat == Pat::Wild {
            return;
        }
        if let Some(witness) = self.useful(&[vec![pat.clone()]], &[Pat::Wild]) {
            let shown = self.show_pat(&witness[0], false);
            self.error(
                codes::T009,
                self.span(node),
                format!("this pattern does not match `{shown}`; use `match` to handle every case"),
            );
        }
    }

    /// Whether `pat` matches every value of its type.
    pub fn covers_all(&self, pat: &Pat) -> bool {
        self.useful(&[vec![pat.clone()]], &[Pat::Wild]).is_none()
    }

    /// A value vector that matches `v` but no row; `None` if there is none.
    fn useful(&self, rows: &[Vec<Pat>], v: &[Pat]) -> Option<Vec<Pat>> {
        let Some((first, rest)) = v.split_first() else {
            return rows.is_empty().then(Vec::new);
        };
        let rows: Vec<Vec<Pat>> = rows.iter().flat_map(|r| expand(r)).collect();
        match first {
            Pat::Or(alternatives) => alternatives.iter().find_map(|alt| {
                let v: Vec<Pat> = std::iter::once(alt.clone())
                    .chain(rest.iter().cloned())
                    .collect();
                self.useful(&rows, &v)
            }),
            Pat::Ctor(ctor, args) => {
                let specialized = specialize(&rows, ctor, args.len());
                let v: Vec<Pat> = args.iter().chain(rest).cloned().collect();
                let witness = self.useful(&specialized, &v)?;
                Some(rebuild(ctor, witness, args.len()))
            }
            Pat::Record(fields) => {
                let labels = labels(&rows, Some(fields));
                let specialized = specialize_record(&rows, &labels);
                let v: Vec<Pat> = record_args(first, &labels)
                    .into_iter()
                    .chain(rest.iter().cloned())
                    .collect();
                let witness = self.useful(&specialized, &v)?;
                Some(rebuild_record(&labels, witness))
            }
            Pat::Wild => self.useful_wild(&rows, rest),
        }
    }

    fn useful_wild(&self, rows: &[Vec<Pat>], rest: &[Pat]) -> Option<Vec<Pat>> {
        let heads: Vec<&Pat> = rows.iter().map(|r| &r[0]).collect();
        if heads.iter().any(|h| matches!(h, Pat::Record(_))) {
            let labels = labels(rows, None);
            let specialized = specialize_record(rows, &labels);
            let v: Vec<Pat> = labels
                .iter()
                .map(|_| Pat::Wild)
                .chain(rest.iter().cloned())
                .collect();
            let witness = self.useful(&specialized, &v)?;
            return Some(rebuild_record(&labels, witness));
        }
        let mut seen: Vec<&Ctor> = Vec::new();
        for head in &heads {
            if let Pat::Ctor(c, _) = head
                && !seen.contains(&c)
            {
                seen.push(c);
            }
        }
        let all = seen.first().and_then(|c| self.constructors(c));
        if let Some(all) = &all
            && all.iter().all(|(c, _)| seen.contains(&c))
        {
            return all.iter().find_map(|(ctor, arity)| {
                let specialized = specialize(rows, ctor, *arity);
                let v: Vec<Pat> = std::iter::repeat_n(Pat::Wild, *arity)
                    .chain(rest.iter().cloned())
                    .collect();
                let witness = self.useful(&specialized, &v)?;
                Some(rebuild(ctor, witness, *arity))
            });
        }
        let defaults: Vec<Vec<Pat>> = rows
            .iter()
            .filter(|r| r[0] == Pat::Wild)
            .map(|r| r[1..].to_vec())
            .collect();
        let witness = self.useful(&defaults, rest)?;
        let missing = match (&all, seen.first()) {
            (Some(all), _) => all
                .iter()
                .find(|(c, _)| !seen.contains(&c))
                .map_or(Pat::Wild, |(c, arity)| {
                    Pat::Ctor(c.clone(), vec![Pat::Wild; *arity])
                }),
            (None, Some(Ctor::List(_))) => {
                let n = (0..=seen.len())
                    .find(|n| !seen.contains(&&Ctor::List(*n)))
                    .expect("one of n + 1 lengths is free");
                Pat::Ctor(Ctor::List(n), vec![Pat::Wild; n])
            }
            _ => Pat::Wild,
        };
        Some(std::iter::once(missing).chain(witness).collect())
    }

    /// Every constructor of the type `ctor` belongs to, with arities; `None`
    /// for infinitely many (lists, literals) or an unknown set.
    fn constructors(&self, ctor: &Ctor) -> Option<Vec<(Ctor, usize)>> {
        match ctor {
            Ctor::Bool(_) => Some(vec![(Ctor::Bool(false), 0), (Ctor::Bool(true), 0)]),
            Ctor::Unit => Some(vec![(Ctor::Unit, 0)]),
            Ctor::Tuple(n) => Some(vec![(Ctor::Tuple(*n), *n)]),
            Ctor::List(_) | Ctor::Literal(_) => None,
            Ctor::Variant { module, ty, .. } => {
                let names = match &self.env.type_def(module, ty)?.form {
                    Form::Adt { constructors } => constructors.clone(),
                    Form::Error => vec![ty.clone()],
                    _ => return None,
                };
                Some(
                    names
                        .into_iter()
                        .map(|name| {
                            let entity = Entity {
                                module: module.clone(),
                                namespace: Namespace::Constructor,
                                name: name.clone(),
                            };
                            let arity = self.env.value(&entity).map_or(0, arity);
                            let ctor = Ctor::Variant {
                                module: module.clone(),
                                ty: ty.clone(),
                                name,
                            };
                            (ctor, arity)
                        })
                        .collect(),
                )
            }
        }
    }

    fn show_pat(&self, pat: &Pat, nested: bool) -> String {
        let wrap = |s: String, args: bool| if nested && args { format!("({s})") } else { s };
        match pat {
            Pat::Wild | Pat::Or(_) => "_".to_owned(),
            Pat::Ctor(Ctor::Variant { name, .. }, args) => {
                let mut s = name.clone();
                for arg in args {
                    s.push(' ');
                    s.push_str(&self.show_pat(arg, true));
                }
                wrap(s, !args.is_empty())
            }
            Pat::Ctor(Ctor::Bool(b), _) => b.to_string(),
            Pat::Ctor(Ctor::Unit, _) => "()".to_owned(),
            Pat::Ctor(Ctor::Literal(text), _) => text.clone(),
            Pat::Ctor(Ctor::Tuple(_), args) => format!("({})", self.show_pats(args)),
            Pat::Ctor(Ctor::List(_), args) => format!("[{}]", self.show_pats(args)),
            Pat::Record(fields) => {
                let shown: Vec<String> = fields
                    .iter()
                    .filter(|(_, p)| *p != Pat::Wild)
                    .map(|(l, p)| format!("{l} = {}", self.show_pat(p, false)))
                    .collect();
                if shown.is_empty() {
                    "_".to_owned()
                } else {
                    format!("{{ {} }}", shown.join("; "))
                }
            }
        }
    }

    fn show_pats(&self, pats: &[Pat]) -> String {
        pats.iter()
            .map(|p| self.show_pat(p, false))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn arity(scheme: &Type) -> usize {
    let mut t = scheme;
    while let Type::Forall { body, .. } = t {
        t = body;
    }
    let mut n = 0;
    while let Type::Fun { result, .. } = t {
        n += 1;
        t = result;
    }
    n
}

/// Rows with an or-pattern in front split into one row per alternative.
fn expand(row: &[Pat]) -> Vec<Vec<Pat>> {
    match &row[0] {
        Pat::Or(alternatives) => alternatives
            .iter()
            .flat_map(|alt| {
                let r: Vec<Pat> = std::iter::once(alt.clone())
                    .chain(row[1..].iter().cloned())
                    .collect();
                expand(&r)
            })
            .collect(),
        _ => vec![row.to_vec()],
    }
}

fn specialize(rows: &[Vec<Pat>], ctor: &Ctor, arity: usize) -> Vec<Vec<Pat>> {
    rows.iter()
        .filter_map(|row| {
            let args = match &row[0] {
                Pat::Ctor(c, args) if c == ctor => args.clone(),
                Pat::Wild => vec![Pat::Wild; arity],
                _ => return None,
            };
            Some(args.into_iter().chain(row[1..].iter().cloned()).collect())
        })
        .collect()
}

/// Labels the record patterns of the first column mention, sorted.
fn labels(rows: &[Vec<Pat>], extra: Option<&Vec<(String, Pat)>>) -> Vec<String> {
    let mut labels: Vec<String> = rows
        .iter()
        .filter_map(|r| match &r[0] {
            Pat::Record(fields) => Some(fields),
            _ => None,
        })
        .chain(extra)
        .flat_map(|fields| fields.iter().map(|(l, _)| l.clone()))
        .collect();
    labels.sort();
    labels.dedup();
    labels
}

fn record_args(pat: &Pat, labels: &[String]) -> Vec<Pat> {
    labels
        .iter()
        .map(|label| match pat {
            Pat::Record(fields) => fields
                .iter()
                .find(|(l, _)| l == label)
                .map_or(Pat::Wild, |(_, p)| p.clone()),
            _ => Pat::Wild,
        })
        .collect()
}

fn specialize_record(rows: &[Vec<Pat>], labels: &[String]) -> Vec<Vec<Pat>> {
    rows.iter()
        .map(|row| {
            record_args(&row[0], labels)
                .into_iter()
                .chain(row[1..].iter().cloned())
                .collect()
        })
        .collect()
}

fn rebuild(ctor: &Ctor, mut witness: Vec<Pat>, arity: usize) -> Vec<Pat> {
    let rest = witness.split_off(arity);
    std::iter::once(Pat::Ctor(ctor.clone(), witness))
        .chain(rest)
        .collect()
}

fn rebuild_record(labels: &[String], mut witness: Vec<Pat>) -> Vec<Pat> {
    let rest = witness.split_off(labels.len());
    let fields = labels.iter().cloned().zip(witness).collect();
    std::iter::once(Pat::Record(fields)).chain(rest).collect()
}
