/// A `match` of data: the arm it takes, its names bound to the parts;
/// whether it was one.
pub fn known_match(e: &mut Expr, preserve_types: bool) -> bool {
    let ExprKind::Match(scrutinee, arms) = &e.kind else {
        return false;
    };
    if !data(scrutinee) {
        return false;
    }
    for Arm {
        pat,
        binder_types,
        guard,
        body,
    } in arms
    {
        if guard.is_some() {
            return false;
        }
        let mut binds = Vec::new();
        match matches(pat, scrutinee, &mut binds) {
            Some(true) => {
                let stmts = binds
                    .into_iter()
                    .map(|(x, v)| {
                        let bt = binder_types
                            .iter()
                            .find(|(n, _)| n == &x)
                            .map(|(_, t)| t.clone())
                            .or_else(|| v.ty.clone())
                            .map(|t| vec![(x.clone(), t)])
                            .unwrap_or_default();
                        Stmt::let_typed(Pat::Bind(x), bt, v)
                    })
                    .collect();
                let ty = preserve_types
                    .then(|| e.ty.clone().or_else(|| body.ty.clone()))
                    .flatten();
                *e = Expr::with_ty(ExprKind::Block(stmts, Box::new(body.clone())), e.span, ty);
                return true;
            }
            Some(false) => {}
            None => return false,
        }
    }
    false
}

/// The `let`s of `stmts` whose pattern takes data apart, as `let`s of the
/// names it binds to the parts; whether there was one.
pub fn known_lets(stmts: &mut Vec<Stmt>) -> bool {
    let taken_apart = |s: &Stmt| {
        matches!(
            s,
            Stmt::Let { pat, value, .. } if !matches!(pat, Pat::Bind(_)) && data(value)
        )
    };
    if !stmts.iter().any(taken_apart) {
        return false;
    }
    let mut changed = false;
    let mut out = Vec::with_capacity(stmts.len());
    for stmt in mem::take(stmts) {
        if let Stmt::Let {
            pat,
            binder_types,
            value,
        } = &stmt
            && taken_apart(&stmt)
        {
            let mut binds = Vec::new();
            // a pattern that does not match keeps its failure
            if matches(pat, value, &mut binds) == Some(true) {
                out.extend(binds.into_iter().map(|(x, v)| {
                    let bt = binder_types
                        .iter()
                        .find(|(n, _)| n == &x)
                        .map(|(_, t)| t.clone())
                        .or_else(|| v.ty.clone())
                        .map(|t| vec![(x.clone(), t)])
                        .unwrap_or_default();
                    Stmt::let_typed(Pat::Bind(x), bt, v)
                }));
                changed = true;
                continue;
            }
        }
        out.push(stmt);
    }
    *stmts = out;
    changed
}

/// Whether `pat` matches the data `e`, binding its names; `None` when that
/// is not known here.
pub(super) fn matches(pat: &Pat, e: &Expr, binds: &mut Vec<(String, Expr)>) -> Option<bool> {
    match (pat, &e.kind) {
        (Pat::Wild, _) => Some(true),
        (Pat::Bind(x), _) => {
            binds.push((x.clone(), e.clone()));
            Some(true)
        }
        (Pat::Tuple(pats), ExprKind::Tuple(items)) if pats.len() == items.len() => {
            all(pats.iter().zip(items), binds)
        }
        (Pat::Record(fields), ExprKind::Record(values)) => {
            let pairs: Option<Vec<(&Pat, &Expr)>> = (fields.iter())
                .map(|(l, p)| values.iter().find(|(m, _)| m == l).map(|(_, v)| (p, v)))
                .collect();
            all(pairs?.into_iter(), binds)
        }
        (Pat::Con(c, pats), _) => {
            let (d, args) = applied(e)?;
            if (d.module.as_str(), d.ty.as_str()) != (c.module.as_str(), c.ty.as_str()) {
                return None;
            }
            if d.name != c.name {
                return Some(false);
            }
            (pats.len() == args.len()).then_some(())?;
            all(pats.iter().zip(args), binds)
        }
        (Pat::Lit(Lit::Bool(b)), ExprKind::Lit(Lit::Bool(v))) => Some(b == v),
        _ => None,
    }
}

fn all<'p>(
    pairs: impl Iterator<Item = (&'p Pat, &'p Expr)>,
    binds: &mut Vec<(String, Expr)>,
) -> Option<bool> {
    for (p, v) in pairs {
        if !matches(p, v, binds)? {
            return Some(false);
        }
    }
    Some(true)
}
