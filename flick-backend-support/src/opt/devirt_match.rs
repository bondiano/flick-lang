/// A `match` of data, its arms tried in order: an arm its pattern or a
/// guard known false rules out is dropped, a guard known true is dropped,
/// and the first arm sure to be taken is taken, its names bound to the
/// parts. When what is left tests one local, the `match` tests only that.
/// A guard is known only when it simplifies to a literal, so nothing it
/// does runs early. Whether anything changed.
pub fn known_match(e: &mut Expr, lang: &crate::Known<'_>, preserve_types: bool) -> bool {
    let ExprKind::Match(scrutinee, arms) = &e.kind else {
        return false;
    };
    let Some(taken) = taken(e.span, e.ty.as_ref(), scrutinee, arms, lang, preserve_types) else {
        return false;
    };
    *e = taken;
    true
}

/// What [`known_match`] makes of `match scrutinee with arms`, of type
/// `ty`, when it changes it.
fn taken(
    span: Span,
    ty: Option<&TypeRef>,
    scrutinee: &Expr,
    arms: &[Arm],
    lang: &crate::Known<'_>,
    preserve_types: bool,
) -> Option<Expr> {
    if !data(scrutinee) {
        return None;
    }
    let mut tried = Vec::new();
    let mut changed = false;
    for (i, arm) in arms.iter().enumerate() {
        let (mut binds, mut open) = (Vec::new(), Vec::new());
        if decide(&arm.pat, scrutinee, &mut binds, &mut open) == Some(false) {
            changed = true;
            continue;
        }
        let guard = match &arm.guard {
            Some(g) => match guard_value(g, &binds, &arm.binder_types, lang) {
                Some(false) => {
                    changed = true;
                    continue;
                }
                Some(true) => {
                    changed = true;
                    None
                }
                None => Some(g),
            },
            None => None,
        };
        let sure = open.is_empty() && guard.is_none();
        tried.push((arm, binds, open, guard));
        if sure {
            changed |= i + 1 < arms.len();
            break;
        }
    }
    let (arm, binds, open, guard) = tried.first()?;
    let block_ty = |body: &Expr| preserve_types.then(|| body.ty.clone()).flatten();
    let bound = |binds: &[(String, Expr)], arm: &Arm, ty| {
        let stmts = lets(binds.to_vec(), &arm.binder_types);
        Expr::with_ty(ExprKind::Block(stmts, Box::new(arm.body.clone())), span, ty)
    };
    if open.is_empty() && guard.is_none() {
        let ty = preserve_types.then(|| ty.cloned()).flatten();
        return Some(bound(binds, arm, ty.or_else(|| block_ty(&arm.body))));
    }
    // one local is left to test: the data around it need not be built
    let leaf = tried.iter().find_map(|(_, _, open, _)| open.first().map(|(_, v)| *v));
    let on_leaf = |o: &Open| {
        o.len() <= 1 && o.iter().all(|(_, v)| Some(&v.kind) == leaf.map(|l| &l.kind))
    };
    let unguarded = (tried.iter()).all(|(_, _, open, guard)| guard.is_none() && on_leaf(open));
    let (scrutinee, arms) = if let Some(leaf) = leaf
        && matches!(leaf.kind, ExprKind::Local(_))
        && !matches!(scrutinee.kind, ExprKind::Local(_))
        && unguarded
    {
        let arms = (tried.iter()).map(|(arm, binds, open, _)| {
            let pat = open.first().map_or(Pat::Wild, |(p, _)| (*p).clone());
            Arm {
                binder_types: filter_binder_types(&pat, &arm.binder_types),
                pat,
                guard: None,
                body: bound(binds, arm, block_ty(&arm.body)),
            }
        });
        (leaf, arms.collect())
    } else if changed {
        let arms = (tried.iter()).map(|(arm, _, _, guard)| Arm {
            guard: guard.cloned(),
            ..(*arm).clone()
        });
        (scrutinee, arms.collect())
    } else {
        return None;
    };
    let kind = ExprKind::Match(Box::new(scrutinee.clone()), arms);
    Some(Expr::with_ty(kind, span, ty.cloned()))
}

/// Whether the guard `g` of an arm whose pattern binds `binds` is known:
/// only when, with them, it simplifies to a literal.
fn guard_value(
    g: &Expr,
    binds: &[(String, Expr)],
    binder_types: &[(String, TypeRef)],
    lang: &crate::Known<'_>,
) -> Option<bool> {
    if let ExprKind::Lit(Lit::Bool(b)) = g.kind {
        return Some(b);
    }
    // names of what is not known tell the guard nothing
    if binds.iter().all(|(_, v)| matches!(v.kind, ExprKind::Local(_))) {
        return None;
    }
    let stmts = lets(binds.to_vec(), binder_types);
    let mut copy = Expr::with_ty(ExprKind::Block(stmts, Box::new(g.clone())), g.span, g.ty.clone());
    super::simplify(&mut copy, lang);
    match copy.kind {
        ExprKind::Lit(Lit::Bool(b)) => Some(b),
        _ => None,
    }
}

/// `match` of a construction some parts of which are not data, or of a
/// local bound to one right before: each such part is bound to a local
/// first, in the order it is evaluated, where [`known_match`] then
/// simplifies the `match`. Whether one was.
pub fn named_scrutinees(
    e: &mut Expr,
    fresh: &mut usize,
    lang: &crate::Known<'_>,
    preserve_types: bool,
) -> bool {
    let mut changed = false;
    e.each_child_mut(&mut |c| changed |= named_scrutinees(c, fresh, lang, preserve_types));
    let (span, ty) = (e.span, e.ty.clone());
    let taken = match &mut e.kind {
        ExprKind::Match(scrutinee, arms) => {
            named(scrutinee, arms, span, ty.as_ref(), fresh, lang, preserve_types)
        }
        ExprKind::Block(stmts, result) => {
            return changed | named_lets(stmts, result, fresh, lang, preserve_types);
        }
        _ => None,
    };
    let Some((parts, _, taken)) = taken else {
        return changed;
    };
    *e = Expr::with_ty(ExprKind::Block(parts, Box::new(taken)), span, ty);
    true
}

/// `let x = construction` right before `match x with …`, which the parts
/// of the construction named as [`named_scrutinees`] does lets
/// [`known_match`] simplify once the construction is in its place.
fn named_lets(
    stmts: &mut Vec<Stmt>,
    result: &Expr,
    fresh: &mut usize,
    lang: &crate::Known<'_>,
    preserve_types: bool,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        let next = match stmts.get(i + 1) {
            Some(Stmt::Let { value: v, .. } | Stmt::Expr(v)) => Some(v),
            Some(_) => None,
            None => Some(result),
        };
        let found = if let (Stmt::Let { pat, value, .. }, Some(next)) = (&stmts[i], next)
            && let (Pat::Bind(x), ExprKind::Match(s, arms)) = (pat, &next.kind)
            && matches!(&s.kind, ExprKind::Local(l) if l == x)
        {
            named(value, arms, next.span, next.ty.as_ref(), fresh, lang, preserve_types)
        } else {
            None
        };
        if let Some((parts, shape, _)) = found {
            if let Stmt::Let { value, .. } = &mut stmts[i] {
                *value = shape;
            }
            let n = parts.len();
            stmts.splice(i..i, parts);
            i += n;
            changed = true;
        }
        i += 1;
    }
    changed
}

/// The construction `shape` with its parts that are not data named, the
/// `let`s that name them and `match shape with arms` simplified, when
/// [`known_match`] simplifies it.
fn named(
    shape: &Expr,
    arms: &[Arm],
    span: Span,
    ty: Option<&TypeRef>,
    fresh: &mut usize,
    lang: &crate::Known<'_>,
    preserve_types: bool,
) -> Option<(Vec<Stmt>, Expr, Expr)> {
    let built = matches!(
        shape.kind,
        ExprKind::Tuple(_) | ExprKind::List(_) | ExprKind::Record(_)
    ) || applied(shape).is_some();
    if !built || data(shape) {
        return None;
    }
    let (mut named, mut parts) = (shape.clone(), Vec::new());
    name_parts(&mut named, fresh, &mut parts);
    let taken = taken(span, ty, &named, arms, lang, preserve_types)?;
    Some((parts, named, taken))
}

/// The `let`s of the names `binds` binds, typed as `binder_types` or their
/// value tells.
fn lets(binds: Vec<(String, Expr)>, binder_types: &[(String, TypeRef)]) -> Vec<Stmt> {
    (binds.into_iter())
        .map(|(x, v)| {
            let bt = (binder_types.iter())
                .find(|(n, _)| n == &x)
                .map(|(_, t)| t.clone())
                .or_else(|| v.ty.clone())
                .map(|t| vec![(x.clone(), t)])
                .unwrap_or_default();
            Stmt::let_typed(Pat::Bind(x), bt, v)
        })
        .collect()
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
                out.extend(lets(binds, binder_types));
                changed = true;
                continue;
            }
        }
        out.push(stmt);
    }
    *stmts = out;
    changed
}

/// The parts of data a pattern may or may not match, with their patterns.
type Open<'a> = Vec<(&'a Pat, &'a Expr)>;

/// Whether `pat` matches the data `e`, binding its names; `None` when that
/// is not known here.
pub(super) fn matches(pat: &Pat, e: &Expr, binds: &mut Vec<(String, Expr)>) -> Option<bool> {
    decide(pat, e, binds, &mut Vec::new())
}

/// [`matches`], with the parts that leave it unknown in `open`: a part of
/// data that cannot match makes the whole a mismatch, wherever it is.
fn decide<'a>(
    pat: &'a Pat,
    e: &'a Expr,
    binds: &mut Vec<(String, Expr)>,
    open: &mut Open<'a>,
) -> Option<bool> {
    let unknown = |open: &mut Open<'a>| -> Option<bool> {
        open.push((pat, e));
        None
    };
    match (pat, &e.kind) {
        (Pat::Wild, _) => Some(true),
        (Pat::Bind(x), _) => {
            binds.push((x.clone(), e.clone()));
            Some(true)
        }
        (Pat::Tuple(pats), ExprKind::Tuple(items)) if pats.len() == items.len() => {
            every(pats.iter().zip(items), binds, open)
        }
        (Pat::Record(fields), ExprKind::Record(values)) => {
            let pairs: Option<Vec<(&Pat, &Expr)>> = (fields.iter())
                .map(|(l, p)| values.iter().find(|(m, _)| m == l).map(|(_, v)| (p, v)))
                .collect();
            match pairs {
                Some(pairs) => every(pairs.into_iter(), binds, open),
                None => unknown(open),
            }
        }
        (Pat::List(pats, rest), ExprKind::List(items))
            if rest.as_deref().is_none_or(|r| matches!(r, Pat::Wild | Pat::Bind(_))) =>
        {
            let short = items.len() < pats.len();
            if short || (rest.is_none() && items.len() != pats.len()) {
                return Some(false);
            }
            let found = every(pats.iter().zip(items), binds, open);
            if let Some(Pat::Bind(x)) = rest.as_deref() {
                let tail = ExprKind::List(items[pats.len()..].to_vec());
                binds.push((x.clone(), Expr::with_ty(tail, e.span, e.ty.clone())));
            }
            found
        }
        (Pat::Con(c, pats), _) => match applied(e) {
            Some((d, _)) if (&d.module, &d.ty) != (&c.module, &c.ty) => unknown(open),
            Some((d, _)) if d.name != c.name => Some(false),
            Some((_, args)) if args.len() == pats.len() => {
                every(pats.iter().zip(args), binds, open)
            }
            _ => unknown(open),
        },
        (Pat::Lit(p @ (Lit::Bool(_) | Lit::Int(_) | Lit::String(_) | Lit::Unit)), ExprKind::Lit(v))
            if mem::discriminant(p) == mem::discriminant(v) =>
        {
            Some(p == v)
        }
        // the first alternative that matches binds the names
        (Pat::Or(alts), _) => {
            let (b, o) = (binds.len(), open.len());
            for alt in alts {
                match decide(alt, e, binds, open) {
                    Some(true) => return Some(true),
                    found => {
                        binds.truncate(b);
                        open.truncate(o);
                        if found.is_none() {
                            return unknown(open);
                        }
                    }
                }
            }
            Some(false)
        }
        _ => unknown(open),
    }
}

fn every<'a>(
    pairs: impl Iterator<Item = (&'a Pat, &'a Expr)>,
    binds: &mut Vec<(String, Expr)>,
    open: &mut Open<'a>,
) -> Option<bool> {
    let mut found = Some(true);
    for (p, v) in pairs {
        match decide(p, v, binds, open) {
            Some(false) => return Some(false),
            None => found = None,
            Some(true) => {}
        }
    }
    found
}
