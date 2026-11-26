impl Checker<'_> {
    /// `provide layer { body }`: the layer is built before the body runs;
    /// the body may need the services it builds, and building it may need
    /// other services and fail with its own errors.
    pub fn provide(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let children: Vec<&Node> = node.nodes().collect();
        let Some(&layer) = children.first() else {
            return Type::Error;
        };
        if let Some(service) = self.record_handler(layer) {
            return self.provide_impl(node, layer, service, children.get(1).copied(), expected);
        }
        let actual = self.infer(layer);
        let (inputs, errors, outputs) = (self.fresh(), self.store.fresh_error_row(), self.fresh());
        let shape = Type::apps(
            Type::lang(Lang::Layer),
            [inputs.clone(), errors.clone(), outputs.clone()],
        );
        if self.unify(&self.whnf(&actual), &shape).is_err() {
            let shown = self.show(&actual);
            self.error(
                codes::T003,
                self.span(layer),
                format!("`provide` takes a `Layer`, but this has type `{shown}`"),
            );
        }
        let span = self.keyword_span(node, K::ProvideKw);
        if self.runtime_allowed(span) {
            self.perform(&inputs, &errors, span);
        }
        let outputs = self.store.normalize(&outputs);
        let labels = outputs.entries.iter().map(|(l, _)| l.clone()).collect();
        // an unknown layer may build any service
        let unknown = !outputs.tails.is_empty();
        let provided = outputs.entries;
        // outputs ending in a variable of the signature (`Layer none none o`)
        // provide all of it; an unknown tail does not grow by the body's needs
        let open: Vec<Type> = outputs
            .tails
            .into_iter()
            .filter(|t| matches!(t, Type::Skolem(_) | Type::Var(_)))
            .collect();
        let needed = self.fresh();
        let mut stage = self.cx.stage.clone();
        stage.services = needed.clone();
        stage.in_result = false;
        stage.returns = None;
        let outer = mem::replace(&mut self.cx.stage, stage);
        self.enter_provide(node, span, labels, unknown);
        let ty = match (children.get(1), expected) {
            (Some(body), Some(expected)) => {
                self.check(body, expected);
                expected.clone()
            }
            (Some(body), None) => self.infer(body),
            (None, _) => Type::Error,
        };
        self.leave_provide();
        let inner = mem::replace(&mut self.cx.stage, outer);
        self.cx.stage.calls = inner.calls;
        if self.cx.stage.pure.is_none() {
            self.live_install(provided.iter().map(|(_, t)| t), span);
            let tail = open
                .into_iter()
                .fold(self.cx.stage.services.clone(), |rest, t| {
                    Type::Union(Box::new(t), Box::new(rest))
                });
            // a service with arguments is answered by its name: the body may
            // need no other instance of it, which only the outside could give
            let instances: Vec<Elem> = (provided.iter())
                .filter(|(l, t)| l.is_instance(t))
                .map(|(l, _)| l.clone())
                .collect();
            let tail = if instances.is_empty() {
                tail
            } else {
                without(tail, &instances)
            };
            let rest = Type::Row(Row {
                entries: provided,
                tail: Some(Box::new(tail)),
                ..Row::default()
            });
            let site = self.cx.stage.site;
            self.sub_row(&needed, &rest, site, span, false);
        }
        ty
    }

    /// `provide S impl { body }`: the record `impl : S.Impl` handles the
    /// service in the body, each operation by its field, as `tail` arms do.
    fn provide_impl(
        &mut self,
        node: &Node,
        layer: &Node,
        (entity, params, effect): (Entity, usize, bool),
        body: Option<&Node>,
        expected: Option<&Type>,
    ) -> Type {
        let span = self.keyword_span(node, K::ProvideKw);
        let label = format!("{}.{}", entity.module, entity.name);
        let mut parts = layer.nodes();
        if let Some(head) = parts.next() {
            self.record_name(self.span(head), Ref::ServiceTag(label.clone()));
        }
        let operations = match self
            .env
            .type_def(&entity.module, &entity.name)
            .map(|d| &d.form)
        {
            Some(Form::Service { operations, .. }) => operations.clone(),
            _ => Vec::new(),
        };
        let ops = operations
            .iter()
            .map(|o| {
                let dicts = match &o.ty {
                    Type::Forall { constraints, .. } => constraints.len(),
                    _ => 0,
                };
                (o.name.clone(), dicts, self.arrow_stages(&o.ty))
            })
            .collect();
        self.typing.provided_ops.insert(node.range(), ops);
        if effect {
            self.error(
                codes::E016,
                span,
                format!(
                    "`provide {0} impl` takes a service, and `{0}` is an effect: handle it with the arms of a `try`",
                    entity.name
                ),
            );
        }
        if self.runtime_allowed(span) {
            let empty = Type::empty_row();
            self.perform(&empty, &empty, span);
        }
        let args: Vec<Type> = (0..params).map(|_| self.fresh()).collect();
        let entry = Type::apps(Type::con(&entity.module, &entity.name), args.clone());
        let implementation = Type::apps(
            Type::con(&entity.module, &format!("{}.Impl", entity.name)),
            args,
        );
        if let Some(record) = parts.next() {
            self.check(record, &implementation);
        }
        let needed = self.fresh();
        let mut stage = self.cx.stage.clone();
        stage.services = needed.clone();
        stage.in_result = false;
        stage.returns = None;
        let outer = mem::replace(&mut self.cx.stage, stage);
        let label = Elem::Named(label);
        self.enter_provide(node, span, vec![label.clone()], false);
        let ty = match (body, expected) {
            (Some(body), Some(expected)) => {
                self.check(body, expected);
                expected.clone()
            }
            (Some(body), None) => self.infer(body),
            (None, _) => Type::Error,
        };
        self.leave_provide();
        let inner = mem::replace(&mut self.cx.stage, outer);
        self.cx.stage.calls = inner.calls;
        if self.cx.stage.pure.is_none() {
            self.live_install([&entry], span);
            // as for a layer, an instance leaves no other in the tail
            let tail = self.cx.stage.services.clone();
            let tail = if label.is_instance(&entry) {
                without(tail, slice::from_ref(&label))
            } else {
                tail
            };
            let rest = Type::Row(Row {
                entries: vec![(label, entry)],
                tail: Some(Box::new(tail)),
                ..Row::default()
            });
            let site = self.cx.stage.site;
            self.sub_row(&needed, &rest, site, span, false);
        }
        ty
    }

    /// The service `S` of `provide S impl { … }`: the head of the
    /// application names a service, its module and name, the number of its
    /// parameters, and whether it is an effect.
    fn record_handler(&self, layer: &Node) -> Option<(Entity, usize, bool)> {
        let mut parts = layer.nodes();
        let (head, _, None) = (parts.next()?, parts.next()?, parts.next()) else {
            return None;
        };
        if layer.kind != K::AppExpr || head.kind != K::NameExpr {
            return None;
        }
        let tokens: Vec<_> = head.child_tokens().filter(|t| t.kind != K::Dot).collect();
        if tokens.last()?.kind != K::Con {
            return None;
        }
        let path: Vec<&str> = tokens.iter().map(|t| t.text(self.text)).collect();
        let Found::Entity(entity) = self.env.find_type(&path.join(".")) else {
            return None;
        };
        match &self.env.type_def(&entity.module, &entity.name)?.form {
            Form::Service { params, effect, .. } => Some((entity.clone(), params.len(), *effect)),
            _ => None,
        }
    }

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

    /// After checking `layer` in `Layer.run layer`, enforces the two type-level
    /// `provide` invariants on the remaining `(Unit ->{ro + r; e2} a) ->{ri + r; e + e2} a`
    /// arrow (covering direct calls, pipes, and partial application):
    /// 1. Parameterized service instances in `ro` exclude those service names
    ///    from the body's residual row `r` (`FLICK-E019`).
    /// 2. Region-indexed services in `ro` require their indexing regions live
    ///    where the layer is installed (`FLICK-G002`).
    pub(super) fn refine_layer_run_after_layer(&self, ro: &Type, ty: &mut Type) {
        let outputs = self.store.normalize(ro);
        let instances: Vec<Elem> = (outputs.entries.iter())
            .filter(|(l, t)| l.is_instance(t))
            .map(|(l, _)| l.clone())
            .collect();
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
        if !instances.is_empty() {
            let mut body = body_param.as_mut();
            while let Type::Bounded { ty: inner, .. } | Type::Captured { ty: inner, .. } = body {
                body = inner;
            }
            if let Type::Fun {
                effects: Some(body_effects),
                ..
            } = body
            {
                replace_union_rhs(&mut body_effects.services, |r| without(r, &instances));
            }
        }
        run_effects.services = self.with_live_install(
            &run_effects.services,
            outputs.entries.iter().map(|(_, t)| t),
        );
    }

    /// Enters the `Provide` scope for a `Layer.run` call building `ro`.
    pub(super) fn enter_layer_run_provide(&mut self, head: &Node, span: Span, ro: &Type) {
        let outputs = self.store.normalize(ro);
        let labels = outputs.entries.iter().map(|(l, _)| l.clone()).collect();
        let open = !outputs.tails.is_empty();
        self.enter_provide(head, span, labels, open);
    }

    /// Settles the residual `Provided` obligations of a `Layer.run` body under
    /// its `Provide` scope (while leaving the layer's own inputs `ri` in the
    /// enclosing scope), checks region-indexed installations, and performs the
    /// call effects.
    pub(super) fn layer_run_call_effects(
        &mut self,
        head: &Node,
        ro: &Type,
        effects: &super::ty::Effects,
        span: Span,
        already_entered: bool,
    ) {
        if !already_entered {
            self.enter_layer_run_provide(head, span, ro);
        }
        let mut settled = effects.clone();
        replace_union_rhs(&mut settled.services, |r| self.settle_performed(&r, span));
        self.leave_provide();
        if self.cx.stage.pure.is_none() {
            let outputs = self.store.normalize(ro);
            self.live_install(outputs.entries.iter().map(|(_, t)| t), span);
        }
        self.call_effects(&settled, span);
    }

    /// If `rhs` of `body |> rhs` is a 1-argument `Layer.run layer` application,
    /// returns `(callee, ro)` so `pipe_from` settles residual `Provided`
    /// obligations under the layer's `Provide` scope.
    pub(super) fn layer_run_pipe_outputs<'n>(
        &self,
        rhs: &'n Node,
        param: &Type,
    ) -> Option<(&'n Node, Type)> {
        let app = super::sugar_lints::unparen(rhs);
        if app.kind != K::AppExpr {
            return None;
        }
        let mut parts = app.nodes();
        let (callee, _, None) = (parts.next()?, parts.next()?, parts.next()) else {
            return None;
        };
        if !self.is_lang_callee(callee, Lang::LayerRun) {
            return None;
        }
        let Type::Fun {
            effects: Some(body_effects),
            ..
        } = self.whnf(param)
        else {
            return None;
        };
        let ro = extract_union_lhs(&body_effects.services)?;
        Some((callee, ro))
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

/// Rewrites the right-hand operand of `Union(left, right)` in `ty`, looking
/// through an outer `Row` wrapper added for `Live`.
fn replace_union_rhs(ty: &mut Type, f: impl FnOnce(Type) -> Type) {
    match ty {
        Type::Union(_, rhs) => {
            let old = mem::replace(rhs.as_mut(), Type::Error);
            **rhs = f(old);
        }
        Type::Row(row) => {
            if let Some(tail) = row.tail.as_deref_mut() {
                replace_union_rhs(tail, f);
            }
        }
        _ => {}
    }
}

/// Extracts the left-hand operand of `Union(left, right)` in `ty`, looking
/// through an outer `Row` wrapper.
fn extract_union_lhs(ty: &Type) -> Option<Type> {
    match ty {
        Type::Union(lhs, _) => Some((**lhs).clone()),
        Type::Row(row) => extract_union_lhs(row.tail.as_deref()?),
        _ => None,
    }
}
