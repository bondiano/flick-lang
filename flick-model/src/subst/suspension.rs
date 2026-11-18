/// Source values whose construction performs no evaluation or store lookup.
/// Aggregates qualify only recursively: a record with a call, projection,
/// global, or cell read in any evaluated field must retain its original let.
fn inert_value(value: &Term) -> bool {
    match value {
        Term::Lam(..) | Term::Task(_) | Term::Lit(_) | Term::Tag(_) => true,
        Term::Tuple(values) | Term::List(values) => values.iter().all(inert_value),
        Term::Record(fields) => fields.iter().all(|(_, value)| inert_value(value)),
        Term::Apply(Head::Con(constructor), values) => {
            // Apply is a closed runtime value, unlike source constructor App
            // spines. Do not admit open children that closed substitution
            // deliberately does not traverse.
            values.len() <= constructor.arity
                && values.iter().all(inert_value)
                && free_vars(value).is_empty()
        }
        _ => false,
    }
}

/// Remove an administrative, single-use suspension binding. Constructing a
/// task or lambda, or an aggregate of inert values, performs no computation:
/// suspension bodies still run only at await/application.
/// Unlike evaluator substitution, the value may be open. Freshening conflicting
/// binders first preserves lexical captures and region names in signatures.
/// Multiple uses retain their shared binding; computations are never moved.
pub(crate) fn inline_suspension(
    name: &Name,
    value: &Term,
    body: &Term,
    fresh: &mut impl FnMut() -> Name,
) -> Option<Term> {
    if !inert_value(value) {
        return None;
    }
    let marker = unused_name(body, value, fresh);
    let counted = subst(body, &[(name.clone(), Term::Var(marker.clone()))]);
    let count = Cell::new(0);
    any(&counted, &|term| {
        if matches!(term, Term::Var(found) if *found == marker) {
            count.set(count.get() + 1);
        }
        false
    });
    if count.get() != 1 {
        return None;
    }
    let avoid = free_vars(value);
    let safe = freshen(body, value, &avoid, fresh);
    Some(subst(&safe, &[(name.clone(), value.clone())]))
}

fn unused_name(a: &Term, b: &Term, fresh: &mut impl FnMut() -> Name) -> Name {
    loop {
        let candidate = fresh();
        if !mentions(a, &candidate) && !mentions(b, &candidate) {
            return candidate;
        }
    }
}

fn mentions(term: &Term, name: &Name) -> bool {
    any(term, &|term| {
        let signature = match term {
            Term::Var(x) | Term::Lam(x, _) | Term::Scope(x, _) if x == name => return true,
            Term::Let(x, sig, ..) | Term::LetRec(x, sig, ..) | Term::Fix(x, sig, _) => {
                if x == name {
                    return true;
                }
                sig
            }
            Term::Match(_, arms) | Term::Try(_, arms) | Term::Handle(_, arms) => {
                return arms.iter().any(|arm| {
                    let mut names = Vec::new();
                    binders(&arm.pat, &mut names);
                    names.contains(name)
                });
            }
            _ => return false,
        };
        signature.as_ref().is_some_and(|sig| {
            **sig != in_regions(sig, &[(name.clone(), format!("{name}$probe").into())])
        })
    })
}

fn fresh_binding(
    name: &Name,
    scope: &Term,
    value: &Term,
    avoid: &BTreeSet<Name>,
    fresh: &mut impl FnMut() -> Name,
) -> Name {
    if avoid.contains(name) || mentions(value, name) {
        unused_name(scope, value, fresh)
    } else {
        name.clone()
    }
}

fn fresh_arms(
    arms: &[Arm],
    value: &Term,
    avoid: &BTreeSet<Name>,
    fresh: &mut impl FnMut() -> Name,
) -> Rc<[Arm]> {
    arms.iter()
        .map(|arm| {
            let mut bound = Vec::new();
            binders(&arm.pat, &mut bound);
            let scope = Term::Match(Rc::new(Term::unit()), Rc::from([arm.clone()]));
            let replacements: Vec<_> = bound
                .iter()
                .filter(|name| avoid.contains(*name) || mentions(value, name))
                .map(|name| (name.clone(), Term::Var(unused_name(&scope, value, fresh))))
                .collect();
            let rename = |pat: &Pat| rename_pattern(pat, &replacements);
            Arm {
                pat: rename(&arm.pat),
                guard: arm
                    .guard
                    .as_ref()
                    .map(|guard| freshen(&subst(guard, &replacements), value, avoid, fresh)),
                body: freshen(&subst(&arm.body, &replacements), value, avoid, fresh),
            }
        })
        .collect()
}

fn rename_pattern(pattern: &Pat, replacements: &Subst) -> Pat {
    match pattern {
        Pat::Var(name) => match replacements.iter().find(|(old, _)| old == name) {
            Some((_, Term::Var(new))) => Pat::Var(new.clone()),
            _ => pattern.clone(),
        },
        Pat::Con(con, args) => Pat::Con(
            con.clone(),
            args.iter()
                .map(|p| rename_pattern(p, replacements))
                .collect(),
        ),
        Pat::Tuple(args) => Pat::Tuple(
            args.iter()
                .map(|p| rename_pattern(p, replacements))
                .collect(),
        ),
        Pat::List(args, rest) => Pat::List(
            args.iter()
                .map(|p| rename_pattern(p, replacements))
                .collect(),
            rest.as_ref()
                .map(|p| Rc::new(rename_pattern(p, replacements))),
        ),
        Pat::Record(fields) => Pat::Record(
            fields
                .iter()
                .map(|(label, p)| (label.clone(), rename_pattern(p, replacements)))
                .collect(),
        ),
        Pat::Or(args) => Pat::Or(
            args.iter()
                .map(|p| rename_pattern(p, replacements))
                .collect(),
        ),
        Pat::Wild | Pat::Lit(_) => pattern.clone(),
    }
}

fn freshen(
    e: &Term,
    value: &Term,
    avoid: &BTreeSet<Name>,
    fresh: &mut impl FnMut() -> Name,
) -> Term {
    let rc = Rc::new;
    match e {
        Term::Lam(x, body) | Term::Scope(x, body) | Term::Fix(x, _, body) => {
            let next = fresh_binding(x, e, value, avoid, fresh);
            let substitution = [(x.clone(), Term::Var(next.clone()))];
            let body = if matches!(e, Term::Scope(..)) {
                subst_in_region(body, &substitution, x, &next)
            } else {
                subst(body, &substitution)
            };
            let body = rc(freshen(&body, value, avoid, fresh));
            match e {
                Term::Lam(..) => Term::Lam(next, body),
                Term::Scope(..) => Term::Scope(next, body),
                Term::Fix(_, sig, _) => Term::Fix(next, sig.clone(), body),
                _ => unreachable!(),
            }
        }
        Term::Let(x, sig, a, b) | Term::LetRec(x, sig, a, b) => {
            let next = fresh_binding(x, e, value, avoid, fresh);
            let substitution = [(x.clone(), Term::Var(next.clone()))];
            let a = if matches!(e, Term::LetRec(..)) {
                subst(a, &substitution)
            } else {
                (**a).clone()
            };
            let b = subst(b, &substitution);
            let a = rc(freshen(&a, value, avoid, fresh));
            let b = rc(freshen(&b, value, avoid, fresh));
            if matches!(e, Term::LetRec(..)) {
                Term::LetRec(next, sig.clone(), a, b)
            } else {
                Term::Let(next, sig.clone(), a, b)
            }
        }
        Term::Match(x, arms) | Term::Try(x, arms) | Term::Handle(x, arms) => {
            let x = rc(freshen(x, value, avoid, fresh));
            let arms = fresh_arms(arms, value, avoid, fresh);
            match e {
                Term::Match(..) => Term::Match(x, arms),
                Term::Try(..) => Term::Try(x, arms),
                Term::Handle(..) => Term::Handle(x, arms),
                _ => unreachable!(),
            }
        }
        _ => fresh_children(e, value, avoid, fresh),
    }
}

fn fresh_children(
    e: &Term,
    value: &Term,
    avoid: &BTreeSet<Name>,
    fresh: &mut impl FnMut() -> Name,
) -> Term {
    let mut go = |term: &Term| Rc::new(freshen(term, value, avoid, fresh));
    match e {
        Term::App(a, b) => Term::App(go(a), go(b)),
        Term::WithOwnerEvidence(a, b) => Term::WithOwnerEvidence(go(a), go(b)),
        Term::ActivateOwner(a, b) => Term::ActivateOwner(go(a), go(b)),
        Term::Update(a, l, b) => Term::Update(go(a), l.clone(), go(b)),
        Term::Provide(a, b) => Term::Provide(go(a), go(b)),
        Term::Join(a, b) => Term::Join(go(a), go(b)),
        Term::Field(a, l) => Term::Field(go(a), l.clone()),
        Term::Task(a) => Term::Task(go(a)),
        Term::Await(a) => Term::Await(go(a)),
        Term::Raise(a) => Term::Raise(go(a)),
        Term::Bind(p, a) => Term::Bind(p.clone(), go(a)),
        Term::With(env, a) => Term::With(env.clone(), go(a)),
        Term::Inside(p, n, a) => Term::Inside(*p, n.clone(), go(a)),
        Term::Settle(l, a) => Term::Settle(*l, go(a)),
        Term::Tuple(xs) => {
            Term::Tuple(xs.iter().map(|x| freshen(x, value, avoid, fresh)).collect())
        }
        Term::List(xs) => Term::List(xs.iter().map(|x| freshen(x, value, avoid, fresh)).collect()),
        Term::Structural(c, xs) => Term::Structural(
            c.clone(),
            xs.iter().map(|x| freshen(x, value, avoid, fresh)).collect(),
        ),
        Term::Record(xs) => Term::Record(
            xs.iter()
                .map(|(n, x)| (n.clone(), freshen(x, value, avoid, fresh)))
                .collect(),
        ),
        Term::Handler(h) => {
            let mapped = {
                let fresh = RefCell::new(&mut *fresh);
                h.map(
                    |t| freshen(t, value, avoid, &mut **fresh.borrow_mut()),
                    Clone::clone,
                )
            };
            let errors = fresh_arms(&mapped.errors, value, avoid, fresh);
            Term::Handler(Rc::new(Handler { errors, ..mapped }))
        }
        Term::Var(_)
        | Term::Global(_)
        | Term::Lit(_)
        | Term::Apply(..)
        | Term::Tag(_)
        | Term::Stuck(_)
        | Term::Loc(_)
        | Term::Env(_)
        | Term::Die(_)
        | Term::OwnerCandidate(_)
        | Term::ReadOwnerEvidence
        | Term::DefaultOwner
        | Term::Own(_)
        | Term::Cont(_)
        | Term::Signal(_)
        | Term::Cap(..) => e.clone(),
        Term::Lam(..)
        | Term::Scope(..)
        | Term::Fix(..)
        | Term::Let(..)
        | Term::LetRec(..)
        | Term::Match(..)
        | Term::Try(..)
        | Term::Handle(..) => unreachable!("binders handled by freshen"),
    }
}
