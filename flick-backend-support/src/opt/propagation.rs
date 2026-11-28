/// For each statement that binds one local with `let`: how often the
/// statements after it and `result` read the local, and how often they
/// surely evaluate that read once. One walk of the block from its end.
fn later_uses(stmts: &[Stmt], result: &Expr) -> Vec<Option<(usize, usize)>> {
    let mut all: HashMap<&str, usize> = HashMap::new();
    let mut strict: HashMap<&str, usize> = HashMap::new();
    each_use(result, &mut |l| *all.entry(l).or_default() += 1);
    each_strict_use(result, &mut |l| *strict.entry(l).or_default() += 1);
    let mut later = vec![None; stmts.len()];
    for (i, stmt) in stmts.iter().enumerate().rev() {
        if let Stmt::Let {
            pat: Pat::Bind(name),
            ..
        } = stmt
        {
            let of = |counts: &HashMap<&str, usize>| counts.get(name.as_str()).copied();
            later[i] = Some((of(&all).unwrap_or(0), of(&strict).unwrap_or(0)));
        }
        match stmt {
            Stmt::Let { value: v, .. }
            | Stmt::Var { value: v, .. }
            | Stmt::Assign(_, v)
            | Stmt::Expr(v) => {
                each_use(v, &mut |l| *all.entry(l).or_default() += 1);
                each_strict_use(v, &mut |l| *strict.entry(l).or_default() += 1);
            }
            Stmt::Fun { value, .. } => each_use(value, &mut |l| *all.entry(l).or_default() += 1),
        }
    }
    later
}

/// Replaces a local bound to an atom with the atom, and a local read once
/// by its pure value when that read surely happens. A function applied to
/// names, but not to all its arguments, moves to its one read even inside a
/// function when the read gives it the rest: `{ let g = f a; fun x -> g x }`
/// calls `f` with all its arguments at once; so does a method of a
/// dictionary already built, which costs a read either way. One walk of the block: a value
/// on its way to its reads waits in `moving`, and each statement takes what
/// it reads when the walk comes to it. A value reads no local bound after
/// it, so the counts taken before the walk still hold then.
fn evidence_operands(e: &Expr, output: &mut HashSet<String>) {
    if let ExprKind::WithOwnerEvidence(_, call) = &e.kind {
        let mut operand = |value: &Expr| {
            if let ExprKind::Local(name) = &value.kind {
                output.insert(name.clone());
            }
        };
        match &call.kind {
            ExprKind::App { func, arg, .. } => {
                operand(func);
                operand(arg);
            }
            ExprKind::Await(task) => operand(task),
            _ => {}
        }
    }
    e.each_child(&mut |child| evidence_operands(child, output));
}

fn dictionary_operands(e: &Expr, output: &mut HashSet<String>) {
    walk::each_dictionary_use(e, &mut |name| {
        output.insert(name.to_owned());
    });
    e.each_child(&mut |child| dictionary_operands(child, output));
}

fn propagate(stmts: &mut Vec<Stmt>, result: &mut Expr, arity: Arity) -> bool {
    // Without an atom or inert value, no binding can enter `moving`.
    if !stmts.iter().any(|stmt| {
        matches!(
            stmt,
            Stmt::Let { pat: Pat::Bind(_), value, .. } if atom(value) || inert(value, arity)
        )
    }) {
        return false;
    }
    let mut evaluated = HashSet::new();
    let mut metadata = HashSet::new();
    evidence_operands(result, &mut evaluated);
    dictionary_operands(result, &mut metadata);
    for stmt in stmts.iter() {
        match stmt {
            Stmt::Let { value, .. }
            | Stmt::Var { value, .. }
            | Stmt::Assign(_, value)
            | Stmt::Expr(value)
            | Stmt::Fun { value, .. } => {
                evidence_operands(value, &mut evaluated);
                dictionary_operands(value, &mut metadata);
            }
        }
    }
    let later = later_uses(stmts, result);
    let completed: Vec<bool> = (stmts.iter().enumerate())
        .map(|(i, stmt)| match (stmt, later[i]) {
            (
                Stmt::Let {
                    pat: Pat::Bind(_),
                    value: v,
                    ..
                },
                Some((1, _)),
            ) if walk::known_method(v) => true,
            (
                Stmt::Let {
                    pat: Pat::Bind(name),
                    value: v,
                    ..
                },
                Some((1, _)),
            ) if applied_to_atoms(v) => {
                partial(v, arity).is_some_and(|rest| completes(&stmts[i + 1..], result, name, rest))
            }
            _ => false,
        })
        .collect();
    let before = stmts.len();
    let mut moving: HashMap<String, Expr> = HashMap::new();
    let mut kept = Vec::with_capacity(before);
    let mut completed = completed.into_iter();
    for (mut stmt, uses) in mem::take(stmts).into_iter().zip(later) {
        let completed = completed.next().unwrap_or(false);
        // what moves in may be dropped, so a value that may be dropped
        // before it still may after: asking first keeps the answer from
        // walking the values that moved into this one
        let once = matches!(uses, Some((1, 1)));
        let pure = completed
            || once && matches!(&stmt, Stmt::Let { value: v, .. } if inert(v, arity));
        match &mut stmt {
            Stmt::Let { value: v, .. }
            | Stmt::Var { value: v, .. }
            | Stmt::Assign(_, v)
            | Stmt::Expr(v)
            | Stmt::Fun { value: v, .. } => place(v, &mut moving),
        }
        match (stmt, uses) {
            (
                Stmt::Let {
                    pat: Pat::Bind(name),
                    binder_types,
                    value,
                },
                Some((all, _)),
            ) if all != 0
                // A polymorphic local `let` (`Forall`) is instantiated at each
                // use site; substituting its uninstantiated initializer directly
                // would leave bound type variables uninstantiated in typed IR.
                && !binder_types
                    .iter()
                    .any(|(_, ty)| matches!(**ty, IrType::Forall { .. }))
                && (!evaluated.contains(&name)
                    || matches!(value.kind, ExprKind::Local(_) | ExprKind::Lit(_)))
                && (!metadata.contains(&name) || matches!(value.kind, ExprKind::Local(_)))
                && (atom(&value) || pure || (once && inert(&value, arity))) =>
            {
                moving.insert(name, value);
            }
            (stmt, _) => kept.push(stmt),
        }
    }
    place(result, &mut moving);
    *stmts = kept;
    stmts.len() != before
}

/// Puts the values of `moving` where `e` reads their locals: an atom is
/// copied to each read; any other value has one read, and moves there.
fn place(e: &mut Expr, moving: &mut HashMap<String, Expr>) {
    if moving.is_empty() {
        return;
    }
    let mut evaluated = HashSet::new();
    let mut metadata = HashSet::new();
    if moving
        .values()
        .any(|value| !matches!(value.kind, ExprKind::Local(_) | ExprKind::Lit(_)))
    {
        evidence_operands(e, &mut evaluated);
    }
    dictionary_operands(e, &mut metadata);
    let aliases = moving
        .iter()
        .filter_map(|(name, value)| match &value.kind {
            ExprKind::Local(alias) => Some((name.clone(), alias.clone())),
            _ => None,
        })
        .collect();
    place_reads(e, moving, &evaluated, &metadata, &aliases);
}

/// Every substitution entry point preserves the already-evaluated operand
/// boundary, not only block propagation. A caller must retain any binding
/// whose reads could not be substituted.
fn place_reads(
    e: &mut Expr,
    moving: &mut HashMap<String, Expr>,
    evaluated: &HashSet<String>,
    metadata: &HashSet<String>,
    aliases: &HashMap<String, String>,
) {
    if let ExprKind::Local(l) = &e.kind
        && let Some(value) = moving.get(l)
    {
        if (evaluated.contains(l) && !matches!(value.kind, ExprKind::Local(_) | ExprKind::Lit(_)))
            || (metadata.contains(l) && !matches!(value.kind, ExprKind::Local(_)))
        {
            return;
        }
        let old_ty = e.ty.clone();
        *e = if atom(value) {
            value.clone()
        } else {
            moving.remove(l).expect("found above")
        };
        if e.ty.is_none() {
            e.ty = old_ty;
        }
        return;
    }
    walk::rename_dictionaries(e, aliases);
    e.each_child_mut(&mut |c| place_reads(c, moving, evaluated, metadata, aliases));
}

/// `f a b` applied to atoms only.
fn applied_to_atoms(e: &Expr) -> bool {
    let mut head = e;
    while let ExprKind::App { func, arg, .. } = &head.kind {
        if !atom(arg) {
            return false;
        }
        head = func;
    }
    !ptr::eq(head, e)
}

/// Whether the statements `stmts` and `result` read `name` as the function
/// of an application of at least `rest` arguments.
fn completes(stmts: &[Stmt], result: &Expr, name: &str, rest: usize) -> bool {
    fn applies(e: &Expr, name: &str, rest: usize) -> bool {
        let (mut head, mut args) = (e, 0);
        while let ExprKind::App { func, .. } = &head.kind {
            (head, args) = (func, args + 1);
        }
        if matches!(&head.kind, ExprKind::Local(l) if l == name) {
            return args >= rest;
        }
        let mut found = false;
        e.each_child(&mut |c| found |= applies(c, name, rest));
        found
    }
    let in_stmt = |s: &Stmt| match s {
        Stmt::Let { value: v, .. }
        | Stmt::Var { value: v, .. }
        | Stmt::Assign(_, v)
        | Stmt::Expr(v)
        | Stmt::Fun { value: v, .. } => applies(v, name, rest),
    };
    stmts.iter().any(in_stmt) || applies(result, name, rest)
}

/// Drops statements whose value is unused and whose evaluation shows
/// nothing; a runtime call always stays. One walk of the block from its
/// end, with the locals the statements kept so far and `result` read.
fn dead(stmts: &mut Vec<Stmt>, result: &Expr, arity: Arity) -> bool {
    let keep: Vec<bool> = {
        let mut read: HashSet<&str> = HashSet::new();
        each_use(result, &mut |l| {
            read.insert(l);
        });
        let mut keep = vec![true; stmts.len()];
        let has_rec = stmts
            .iter()
            .any(|s| matches!(s, Stmt::Fun { rec: true, .. }));
        loop {
            let before_read = read.len();
            for (i, stmt) in stmts.iter().enumerate().rev() {
                let drop = match stmt {
                    Stmt::Expr(v) => inert(v, arity),
                    Stmt::Let {
                        pat: p, value: v, ..
                    } => {
                        let mut bound = Vec::new();
                        p.binders(&mut bound);
                        p.irrefutable()
                            && bound.iter().all(|n| !read.contains(n))
                            && inert(v, arity)
                    }
                    Stmt::Fun { name, .. } => !read.contains(name.as_str()),
                    Stmt::Var { .. } | Stmt::Assign(..) => false,
                };
                keep[i] = !drop;
                if drop {
                    continue;
                }
                match stmt {
                    Stmt::Let { value: v, .. }
                    | Stmt::Var { value: v, .. }
                    | Stmt::Assign(_, v)
                    | Stmt::Expr(v)
                    | Stmt::Fun { value: v, .. } => {
                        each_use(v, &mut |l| {
                            read.insert(l);
                        });
                    }
                }
            }
            if !has_rec || read.len() == before_read {
                break;
            }
        }
        keep
    };
    let before = stmts.len();
    let mut keep = keep.into_iter();
    stmts.retain(|_| keep.next().expect("one for each statement"));
    stmts.len() != before
}
