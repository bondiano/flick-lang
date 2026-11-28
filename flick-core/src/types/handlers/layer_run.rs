impl Checker<'_> {
    /// Whether `head` resolves to the built-in value lang item `item`,
    /// whether written qualified, aliased, opened, or parenthesized.
    pub(super) fn is_lang_callee(&self, head: &Node, item: Lang) -> bool {
        let head = super::sugar_lints::unparen(head);
        matches!(
            self.typing.names.get(&head.range()),
            Some(Ref::Value { module, name }) if super::lang::is(item, module, name)
        )
    }

    /// The output row of `layer` in an instantiated `Layer.run` application.
    pub(super) fn layer_run_outputs(&self, head: &Node, ty: &Type) -> Option<Type> {
        if !self.is_lang_callee(head, Lang::LayerRun) {
            return None;
        }
        let Type::Fun { param, .. } = self.whnf(ty) else {
            return None;
        };
        let (_, _, ro) = self.layer_parts(&param)?;
        Some(ro)
    }

    /// Attaches provider-scope metadata (`Effects.provides`) when referencing
    /// `Layer.run` (so unapplied aliases like `let f = Layer.run` retain all
    /// provider checks) or any higher-order wrapper whose runtime arrow
    /// discharges services from its callback parameter's service row.
    pub(super) fn attach_provides(&self, node: &Node, ty: &mut Type) {
        if self.is_lang_callee(node, Lang::LayerRun) {
            let mut cur = ty;
            while let Type::Bounded { ty: inner, .. } | Type::Captured { ty: inner, .. } = cur {
                cur = inner;
            }
            if let Type::Fun { param, result, .. } = cur
                && let Some((_, _, ro)) = self.layer_parts(param)
            {
                let mut next = result.as_mut();
                while let Type::Bounded { ty: inner, .. } | Type::Captured { ty: inner, .. } = next
                {
                    next = inner;
                }
                if let Type::Fun {
                    effects: Some(run_effects),
                    ..
                } = next
                {
                    run_effects.provides = Some(ro);
                }
            }
            return;
        }
        self.attach_discharged_provides(ty);
    }

    /// Attaches provider-scope metadata on any runtime arrow in `ty`'s spine
    /// whose callback parameter discharges services relative to the arrow's
    /// own service row.
    pub(super) fn attach_discharged_provides(&self, ty: &mut Type) {
        let mut cur = ty;
        loop {
            while let Type::Bounded { ty: inner, .. } | Type::Captured { ty: inner, .. } = cur {
                cur = inner;
            }
            let Type::Fun {
                param,
                effects,
                result,
            } = cur
            else {
                break;
            };
            self.attach_discharged_arrow(param, effects, result);
            cur = result;
        }
    }

    /// Attaches provider-scope metadata on a single runtime arrow when its
    /// callback parameter discharges services relative to `effects.services`.
    pub(super) fn attach_discharged_arrow(
        &self,
        param: &mut Type,
        effects: &mut Option<Box<Effects>>,
        result: &Type,
    ) {
        if let Some(outer_eff) = effects.as_deref_mut()
            && outer_eff.provides.is_none()
            && let Some(discharged) = self.discharged_services(param, &outer_eff.services, result)
        {
            outer_eff.provides = Some(discharged.clone());
            self.exclude_provided_on_param(param, &discharged);
        }
    }

    /// Services present in a runtime callback parameter's service row that are
    /// absent from the enclosing runtime arrow's service row (discharged by
    /// the wrapper's provider/handler scope).
    fn discharged_services(
        &self,
        param: &Type,
        outer_services: &Type,
        result: &Type,
    ) -> Option<Type> {
        let mut cur = param;
        while let Type::Bounded { ty: inner, .. } | Type::Captured { ty: inner, .. } = cur {
            cur = inner;
        }
        let Type::Fun {
            effects: Some(cb_eff),
            ..
        } = cur
        else {
            return None;
        };
        let cb = self.store.normalize(&cb_eff.services);
        let outer = self.store.normalize(outer_services);
        let shared_tail = cb.tails.iter().any(|t| {
            let bare = bare_tail(t);
            !matches!(bare, Type::Error) && outer.tails.iter().any(|o| bare_tail(o) == bare)
        });
        let outer_extra_meta = outer.tails.iter().any(|o| {
            let bare = bare_tail(o);
            matches!(bare, Type::Meta(_)) && !cb.tails.iter().any(|t| bare_tail(t) == bare)
        });
        // When `cb` has an open tail not shared with `outer` (e.g.
        // `(Unit ->{Greeter | r} a) ->{Log} (Unit ->{Greeter | r} a)` or
        // `Ref`-stashing), `outer` cannot execute `cb` polymorphically to
        // discharge services into `outer`; obligations stay in `r` until the
        // callback is actually invoked.
        if !shared_tail && (!cb.tails.is_empty() || outer_extra_meta) {
            return None;
        }
        let scope_label = super::regions::scope();
        let async_label = super::lang::label(Lang::Async);
        let entries: Vec<(Elem, Type)> = cb
            .entries
            .into_iter()
            .filter(|(l, _)| {
                !matches!(l, Elem::Live | Elem::Provided(_) | Elem::ProvidedAny)
                    && l.name() != Some(scope_label)
                    && l.name() != async_label
                    && outer.get(l).is_none()
                    && (shared_tail || !self.result_carries_service(result, l))
            })
            .collect();
        let tails: Vec<Type> = if shared_tail {
            cb.tails
                .iter()
                .map(bare_tail)
                .filter(|bare| {
                    !matches!(bare, Type::Error)
                        && !outer.tails.iter().any(|o| bare_tail(o) == *bare)
                })
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        if entries.is_empty() && tails.is_empty() {
            return None;
        }
        Some(Norm { entries, tails }.into_type())
    }

    /// Whether `result` carries `label` in a returned callable or `Eff`
    /// service row (distinguishing forwarding wrappers like
    /// `(Unit ->{S} a) ->{} (Unit ->{S} a)` from discharging providers).
    fn result_carries_service(&self, result: &Type, label: &Elem) -> bool {
        let mut cur = self.whnf(result);
        loop {
            match cur {
                Type::Bounded { ty, .. } | Type::Captured { ty, .. } => cur = self.whnf(&ty),
                Type::Fun {
                    effects: Some(e),
                    result: next,
                    ..
                } => {
                    if self.store.normalize(&e.services).get(label).is_some() {
                        return true;
                    }
                    cur = self.whnf(&next);
                }
                Type::Fun {
                    effects: None,
                    result: next,
                    ..
                } => cur = self.whnf(&next),
                other => {
                    return self
                        .eff_parts(&other)
                        .is_some_and(|(s, _, _)| self.store.normalize(&s).get(label).is_some());
                }
            }
        }
    }

    fn exclude_provided_on_param(&self, body_param: &mut Type, ro: &Type) {
        let outputs = self.store.normalize(ro);
        let excluded = provided_exclusions(&outputs);
        if excluded.is_empty() {
            return;
        }
        let mut body = body_param;
        while let Type::Bounded { ty: inner, .. } | Type::Captured { ty: inner, .. } = body {
            body = inner;
        }
        if let Type::Fun {
            effects: Some(body_effects),
            ..
        } = body
        {
            replace_residual_tail(&mut body_effects.services, |r| without(r, &excluded));
        }
    }

    /// After checking `layer` in `Layer.run layer`, enforces the `provide`
    /// invariants on the remaining `(Unit ->{ro + r; e2} a) ->{ri + r; e + e2} a`
    /// arrow (covering direct calls, pipes, partial application, and
    /// higher-order wrappers):
    /// 1. Parameterized service instances in `ro` exclude those service names
    ///    from the body's residual row `r` (`FLICK-E019`).
    /// 2. Provided services in `ro` (or `ProvidedAny` for an open non-meta tail)
    ///    exclude matching `Provided` obligations from `r`, and `run_effects.provides`
    ///    records `ro` for direct/aliased call sites (`FLICK-G003`).
    /// 3. Region-indexed services in `ro` require their indexing regions live
    ///    where the layer is installed (`FLICK-G002`).
    pub(super) fn refine_layer_run_after_layer(&self, ro: &Type, ty: &mut Type) {
        let outputs = self.store.normalize(ro);
        let mut excluded: Vec<Elem> = (outputs.entries.iter())
            .filter(|(l, t)| l.is_instance(t))
            .map(|(l, _)| l.clone())
            .collect();
        excluded.extend(provided_exclusions(&outputs));
        let mut cur = ty;
        while let Type::Bounded { ty: inner, .. } | Type::Captured { ty: inner, .. } = cur {
            cur = inner;
        }
        let Type::Fun {
            param: body_param,
            effects: Some(run_effects),
            ..
        } = cur
        else {
            return;
        };
        run_effects.provides = Some(ro.clone());
        if !excluded.is_empty() {
            let mut body = body_param.as_mut();
            while let Type::Bounded { ty: inner, .. } | Type::Captured { ty: inner, .. } = body {
                body = inner;
            }
            if let Type::Fun {
                effects: Some(body_effects),
                ..
            } = body
            {
                replace_residual_tail(&mut body_effects.services, |r| without(r, &excluded));
            }
        }
        run_effects.services = self.with_live_install(
            &run_effects.services,
            outputs.entries.iter().map(|(_, t)| t),
        );
    }

    /// Re-runs refinement when an arrow carrying `effects.provides` has its
    /// provided row instantiated (e.g. `let f = Layer.run; f greeter`,
    /// `let mk l = Layer.run l; mk greeter`, or `run_with greeter`).
    pub(super) fn refine_provides_arrow(&self, ty: &mut Type, after_layer: bool) {
        let mut cur = &*ty;
        while let Type::Bounded { ty: inner, .. } | Type::Captured { ty: inner, .. } = cur {
            cur = inner;
        }
        let Type::Fun {
            effects: Some(run_effects),
            ..
        } = cur
        else {
            return;
        };
        let Some(ro) = run_effects.provides.clone() else {
            return;
        };
        if after_layer {
            self.refine_layer_run_after_layer(&ro, ty);
        } else {
            let mut cur = ty;
            while let Type::Bounded { ty: inner, .. } | Type::Captured { ty: inner, .. } = cur {
                cur = inner;
            }
            if let Type::Fun {
                param: body_param, ..
            } = cur
            {
                self.exclude_provided_on_param(body_param, &ro);
            }
        }
    }

    /// Enters the `Provide` scope for a `Layer.run` call building `ro`.
    pub(super) fn enter_layer_run_provide(&mut self, head: &Node, span: Span, ro: &Type) {
        let outputs = self.store.normalize(ro);
        let labels = outputs.entries.iter().map(|(l, _)| l.clone()).collect();
        let open = !outputs.tails.is_empty();
        self.enter_provide(head, span, labels, open);
    }

    pub(super) fn begin_layer_run_arg(
        &mut self,
        head: &Node,
        arg: &Node,
        provides_ro: Option<&Type>,
        span: Span,
    ) -> bool {
        let Some(ro) = provides_ro.filter(|_| is_immediate_thunk(arg)) else {
            return false;
        };
        self.enter_layer_run_provide(head, span, ro);
        self.cx.regions.immediate_provide = Some(self.cx.regions.provides.len() - 1);
        true
    }

    pub(super) fn refine_after_arg(
        &self,
        first_arg: bool,
        is_layer_arg: bool,
        layer_run_ro: Option<&Type>,
        ty: &mut Type,
    ) {
        if first_arg && let Some(ro) = layer_run_ro {
            self.refine_layer_run_after_layer(ro, ty);
        } else {
            self.refine_provides_arrow(ty, is_layer_arg);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn finish_layer_run_arg(
        &mut self,
        head: &Node,
        callee: &Type,
        param: &Type,
        effects: Option<&Effects>,
        ro: &Type,
        span: Span,
        entered_layer_run: bool,
        marks: (usize, usize, usize),
        defaults: &mut Vec<(Type, usize)>,
    ) {
        let (body_provided, body_provided_late, subrow_mark) = marks;
        if !defaults.is_empty() {
            self.grow();
            let pending = mem::take(defaults);
            self.settle_defaults(pending);
        }
        self.settle_provided_since(body_provided, body_provided_late);
        if let Some(e) = effects {
            let live = self.live_call(callee, e);
            let arg_subrows = (!entered_layer_run).then_some((subrow_mark, param));
            self.layer_run_call_effects(head, ro, &live, span, entered_layer_run, arg_subrows);
        } else if entered_layer_run {
            self.leave_provide();
        }
    }

    /// Settles the residual `Provided` obligations of a `Layer.run` body under
    /// its `Provide` scope (while leaving the layer's own inputs `ri` in the
    /// enclosing scope), checks region-indexed installations, and performs the
    /// call effects.
    pub(super) fn layer_run_call_effects(
        &mut self,
        head: &Node,
        ro: &Type,
        effects: &Effects,
        span: Span,
        already_entered: bool,
        arg_subrows: Option<(usize, &Type)>,
    ) {
        if !already_entered {
            self.enter_layer_run_provide(head, span, ro);
        }
        if let Some((subrow_mark, param)) = arg_subrows {
            self.settle_layer_run_subrows(subrow_mark, param, span);
        }
        let mut settled = effects.clone();
        if !replace_union_rhs(&mut settled.services, |r| self.settle_performed(&r, span)) {
            settled.services = self.settle_performed(&settled.services, span);
        }
        self.leave_provide();
        if self.cx.stage.pure.is_none() {
            let outputs = self.store.normalize(ro);
            self.live_install(outputs.entries.iter().map(|(_, t)| t), span);
        }
        self.call_effects(&settled, span);
    }

    /// Settles `Provided` obligations already present in the argument-to-body
    /// `SubRow` constraints under the active `Provide` scope so diagnostics
    /// point at the `Layer.run` / alias call span without duplicating reports.
    fn settle_layer_run_subrows(&mut self, subrow_mark: usize, param: &Type, span: Span) {
        let Type::Fun {
            effects: Some(param_effects),
            ..
        } = self.whnf(param)
        else {
            return;
        };
        let param_services = param_effects.services;
        for i in subrow_mark..self.cx.subrows.len() {
            if !self.cx.subrows[i].errors && self.cx.subrows[i].declared == param_services {
                let actual = self.cx.subrows[i].actual.clone();
                self.cx.subrows[i].actual = self.settle_performed(&actual, span);
            }
        }
    }
}

/// Whether `arg` is an immediate 1-parameter `fun _ -> body` thunk, whose body
/// executes directly inside `Layer.run`'s provider scope.
pub(super) fn is_immediate_thunk(arg: &Node) -> bool {
    let arg = super::sugar_lints::unparen(arg);
    arg.kind == K::FunExpr
        && arg
            .nodes()
            .filter(|n| is_pattern(n.kind))
            .count()
            == 1
}

/// Builds `Provided` exclusion labels for the provided output row `outputs`.
fn provided_exclusions(outputs: &Norm) -> Vec<Elem> {
    let mut excluded = Vec::new();
    let scope_label = super::regions::scope();
    let async_label = super::lang::label(Lang::Async);
    for (l, _) in &outputs.entries {
        if matches!(l, Elem::Live | Elem::Provided(_) | Elem::ProvidedAny)
            || l.name() == Some(scope_label)
            || l.name() == async_label
        {
            continue;
        }
        excluded.push(Elem::Provided(Box::new(l.clone())));
    }
    if outputs
        .tails
        .iter()
        .any(|t| !matches!(t, Type::Meta(_) | Type::Error))
    {
        excluded.push(Elem::ProvidedAny);
    }
    excluded
}

/// Strips outer `Type::Without` wrappers from a row tail.
fn bare_tail(mut ty: &Type) -> &Type {
    while let Type::Without { row, .. } = ty {
        ty = row;
    }
    ty
}

/// Rewrites the residual tail of a callback service row: the RHS of a
/// `Union(left, right)` if present, otherwise the tail of a `Row`.
fn replace_residual_tail(ty: &mut Type, f: impl FnOnce(Type) -> Type) -> bool {
    match ty {
        Type::Union(_, rhs) => {
            let old = mem::replace(rhs.as_mut(), Type::Error);
            **rhs = f(old);
            true
        }
        Type::Row(row) => {
            let Some(tail) = row.tail.as_deref_mut() else {
                return false;
            };
            if matches!(tail, Type::Union(..) | Type::Row(_)) {
                replace_residual_tail(tail, f)
            } else {
                let old = mem::replace(tail, Type::Error);
                *tail = f(old);
                true
            }
        }
        _ => false,
    }
}

/// Rewrites the right-hand operand of `Union(left, right)` in `ty`, looking
/// through an outer `Row` wrapper added for `Live`. Returns whether a `Union`
/// was rewritten.
fn replace_union_rhs(ty: &mut Type, f: impl FnOnce(Type) -> Type) -> bool {
    match ty {
        Type::Union(_, rhs) => {
            let old = mem::replace(rhs.as_mut(), Type::Error);
            **rhs = f(old);
            true
        }
        Type::Row(row) => row
            .tail
            .as_deref_mut()
            .is_some_and(|tail| replace_union_rhs(tail, f)),
        _ => false,
    }
}
