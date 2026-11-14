#[derive(Clone, Copy)]
struct Descriptor<'a> {
    ty: &'a Type,
    environment: usize,
}
struct Observer<'a, 'ctx> {
    context: &'ctx ScalarContext<'a>,
    // Flat arena: argument descriptors always retain their PRIOR environment.
    frames: Vec<Vec<(&'a str, Descriptor<'a>)>>,
}
impl<'a> Observer<'a, '_> {
    fn bigint_conversion(
        &mut self,
        input: Descriptor<'a>,
        output: Descriptor<'a>,
    ) -> Result<ScalarDomain, String> {
        let input = self.primitive_domain(input)?;
        let output = self.domain(output)?;
        if matches!(
            input,
            Domain::Int
                | Domain::BigInt
                | Domain::Float
                | Domain::Bool
                | Domain::String
                | Domain::Unit
        ) && output == Domain::BigInt
        {
            Ok(ScalarDomain::BigIntConversion)
        } else {
            Err(format!(
                "unsupported checked BigInt domain {input:?} -> {output:?}"
            ))
        }
    }

    fn scheme(&mut self, scheme: &'a Type) -> Result<Descriptor<'a>, String> {
        let mut descriptor = Descriptor {
            ty: scheme,
            environment: 0,
        };
        loop {
            let (head, arguments) = self.head(descriptor)?;
            if !arguments.is_empty() {
                return Err("unknown applied scalar signature".into());
            }
            if let Type::Forall { vars, body, .. } = head.ty {
                descriptor = Descriptor {
                    ty: body,
                    environment: self.mask(head.environment, vars),
                };
            } else {
                return Ok(head);
            }
        }
    }

    fn mask(&mut self, environment: usize, binders: &[Binder]) -> usize {
        let frame = self.frames[environment]
            .iter()
            .copied()
            .filter(|(name, _)| !binders.iter().any(|binder| binder.name == *name))
            .collect();
        let index = self.frames.len();
        self.frames.push(frame);
        index
    }

    fn head(
        &mut self,
        mut descriptor: Descriptor<'a>,
    ) -> Result<(Descriptor<'a>, VecDeque<Descriptor<'a>>), String> {
        let mut arguments = VecDeque::new();
        loop {
            match descriptor.ty {
                Type::Var(name) => {
                    if let Some((_, replacement)) = self.frames[descriptor.environment]
                        .iter()
                        .find(|(bound, _)| *bound == name)
                    {
                        descriptor = *replacement;
                    } else {
                        return Ok((descriptor, arguments));
                    }
                }
                Type::Captured { ty, .. } | Type::Bounded { ty, .. } => descriptor.ty = ty,
                Type::App { head, arg } => {
                    arguments.push_front(Descriptor {
                        ty: arg,
                        environment: descriptor.environment,
                    });
                    descriptor.ty = head;
                }
                Type::Con { module, name } => {
                    let key = Name::new(module, name);
                    let Some(definition) = self.context.definitions.get(&key) else {
                        return Ok((descriptor, arguments));
                    };
                    let Form::Alias { params, body, .. } = &definition.form else {
                        return Ok((descriptor, arguments));
                    };
                    if arguments.len() < params.len() {
                        return Ok((descriptor, arguments));
                    }
                    // Unknown is not nonnumeric: positive finite cyclic proofs
                    // remain a separate obligation. Never guess a nominal head.
                    if self.context.recursive.contains(&key) {
                        return Err(format!(
                            "unknown scalar domain through recursive alias {key:?}"
                        ));
                    }
                    let frame = params
                        .iter()
                        .map(|param| {
                            (
                                param.as_str(),
                                arguments.pop_front().expect("complete alias spine"),
                            )
                        })
                        .collect();
                    let environment = self.frames.len();
                    self.frames.push(frame);
                    descriptor = Descriptor {
                        ty: body,
                        environment,
                    };
                }
                _ => return Ok((descriptor, arguments)),
            }
        }
    }

    fn domain(&mut self, descriptor: Descriptor<'a>) -> Result<Domain, String> {
        // Keep the existing Number proof and its diagnostics unchanged.
        Ok(match self.primitive_domain(descriptor)? {
            Domain::Bool | Domain::String | Domain::Unit => Domain::Other,
            domain => domain,
        })
    }

    fn primitive_domain(&mut self, descriptor: Descriptor<'a>) -> Result<Domain, String> {
        let (head, arguments) = self.head(descriptor)?;
        if !arguments.is_empty() {
            return Ok(Domain::Unknown);
        }
        let Type::Con { module, name } = head.ty else {
            return Ok(Domain::Unknown);
        };
        if !self
            .context
            .definitions
            .contains_key(&Name::new(module, name))
        {
            return Ok(Domain::Unknown);
        }
        Ok(if head.ty.is_lang(Lang::Int) {
            Domain::Int
        } else if head.ty.is_lang(Lang::BigInt) {
            Domain::BigInt
        } else if head.ty.is_lang(Lang::Float) {
            Domain::Float
        } else if head.ty.is_lang(Lang::Bool) {
            Domain::Bool
        } else if head.ty.is_lang(Lang::String) {
            Domain::String
        } else if head.ty.is_lang(Lang::Unit) {
            Domain::Unit
        } else {
            Domain::Unknown
        })
    }
}

fn children<'a>(ty: &'a Type, pending: &mut Vec<&'a Type>) {
    match ty {
        Type::App { head, arg } => pending.extend([head.as_ref(), arg.as_ref()]),
        Type::Fun {
            param,
            effects,
            result,
        } => {
            pending.extend([param.as_ref(), result.as_ref()]);
            if let Some(effects) = effects {
                pending.extend([&effects.services, &effects.errors]);
            }
        }
        Type::Tuple(items) => pending.extend(items),
        Type::Row(row) | Type::Record(row) => {
            pending.extend(row.entries.iter().map(|(_, ty)| ty));
            pending.extend(row.tail.as_deref());
        }
        Type::Union(left, right) => pending.extend([left.as_ref(), right.as_ref()]),
        Type::Without { row, .. } => pending.push(row),
        Type::Captured { ty, captures, .. } | Type::Bounded { ty, captures } => {
            pending.extend([ty.as_ref(), captures.as_ref()]);
        }
        Type::Forall {
            constraints, body, ..
        } => {
            pending.extend(constraints);
            pending.push(body);
        }
        Type::Exists { contract, body, .. } => pending.extend([contract.as_ref(), body.as_ref()]),
        _ => {}
    }
}
fn validate_type(ty: &Type) -> Result<(), String> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match ty {
            Type::Meta(_) | Type::MetaView(_) | Type::Error => {
                return Err("unresolved/error type in checked declaration context".into());
            }
            Type::Forall { vars, .. } => {
                for binder in vars {
                    validate_kind(&binder.kind)
                        .map_err(|message| format!("forall {}: {message}", binder.name))?;
                }
            }
            Type::Row(row) | Type::Record(row) => {
                for (label, _) in &row.entries {
                    validate_label(label)?;
                }
            }
            Type::Without { gone, .. } => {
                for label in gone {
                    validate_label(label)?;
                }
            }
            _ => {}
        }
        children(ty, &mut pending);
    }
    Ok(())
}
fn validate_label(mut label: &Elem) -> Result<(), String> {
    while let Elem::Provided(inner) = label {
        label = inner;
    }
    if matches!(
        label,
        Elem::Capture(Atom::Meta(_) | Atom::MetaView(_))
            | Elem::Error(Atom::Meta(_) | Atom::MetaView(_))
            | Elem::Service(Atom::Meta(_) | Atom::MetaView(_))
    ) {
        return Err("unresolved row label in checked declaration context".into());
    }
    Ok(())
}
fn validate_kind(kind: &Kind) -> Result<(), String> {
    let mut pending = vec![kind];
    while let Some(kind) = pending.pop() {
        match kind {
            Kind::Meta(_) | Kind::MetaView(_) => {
                return Err("unresolved kind in checked declaration context".into());
            }
            Kind::Row(inner) => pending.push(inner),
            Kind::Arrow(input, output) => pending.extend([input.as_ref(), output.as_ref()]),
            _ => {}
        }
    }
    Ok(())
}

// Iterative Kosaraju over original alias definitions, never instantiated Types.
fn recursive_aliases(definitions: &BTreeMap<Name, &TypeDef>) -> BTreeSet<Name> {
    let aliases: Vec<_> = definitions
        .iter()
        .filter_map(|(name, definition)| {
            matches!(definition.form, Form::Alias { .. }).then_some(name)
        })
        .collect();
    let index: BTreeMap<_, _> = aliases
        .iter()
        .enumerate()
        .map(|(i, name)| (*name, i))
        .collect();
    let mut edges = vec![vec![]; aliases.len()];
    let mut reverse = vec![vec![]; aliases.len()];
    for (at, name) in aliases.iter().enumerate() {
        let Form::Alias { body, .. } = &definitions[*name].form else {
            unreachable!("alias inventory")
        };
        let mut pending = vec![body];
        let mut targets = BTreeSet::new();
        while let Some(ty) = pending.pop() {
            if let Type::Con { module, name } = ty
                && let Some(&target) = index.get(&Name::new(module, name))
            {
                targets.insert(target);
            }
            children(ty, &mut pending);
        }
        edges[at] = targets.into_iter().collect();
        for &target in &edges[at] {
            reverse[target].push(at);
        }
    }
    let mut seen = vec![false; aliases.len()];
    let mut order = Vec::new();
    for root in 0..aliases.len() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some((at, next)) = stack.last_mut() {
            if let Some(&target) = edges[*at].get(*next) {
                *next += 1;
                if !seen[target] {
                    seen[target] = true;
                    stack.push((target, 0));
                }
            } else {
                order.push(*at);
                stack.pop();
            }
        }
    }
    seen.fill(false);
    let mut recursive = BTreeSet::new();
    for root in order.into_iter().rev() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut pending = vec![root];
        let mut component = Vec::new();
        while let Some(at) = pending.pop() {
            component.push(at);
            for &target in &reverse[at] {
                if !seen[target] {
                    seen[target] = true;
                    pending.push(target);
                }
            }
        }
        if component.len() > 1 || edges[root].contains(&root) {
            recursive.extend(component.into_iter().map(|at| aliases[at].clone()));
        }
    }
    recursive
}
