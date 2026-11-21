impl Checker<'_> {
    /// `result { body }`: eager runtime typed-error boundary that runs `body`
    /// in the current fiber and reifies its value as `Ok v` and its typed
    /// failures as `Err e`, returning `Result (Errors e) a`.
    pub fn result_expr(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let span = self.keyword_span(node, K::ResultKw);
        let body = node.nodes().next();
        if let Some(body) = body
            && self.result_record_call(body, span)
        {
            return Type::Error;
        }
        if let Some(reason) = self
            .cx
            .stage
            .pure
            .clone()
            .or_else(|| self.cx.stage.only_sync.map(str::to_owned))
        {
            self.error(
                codes::E001,
                span,
                format!("`result {{ … }}` is a runtime typed-error boundary, but {reason}"),
            );
        }
        if self.runtime_allowed(span) {
            let empty = Type::empty_row();
            self.perform(&empty, &empty, span);
        }
        if body.is_some_and(|b| b.nodes().next().is_none()) {
            self.error(
                codes::T019,
                self.span(node),
                "a `result { … }` block must end with an expression; write `result { () }` for `Unit`",
            );
            return Type::Error;
        }
        let every = self.store.fresh_error_row();
        self.cx.widened_rows.push(every.clone());
        let errors_ty = Type::app(Type::lang(Lang::Errors), every.clone());
        let shape = self.result_expected(expected, &errors_ty);
        let mut stage = self.cx.stage.clone();
        stage.errors = if shape.suppress_errors {
            Type::Error
        } else {
            every
        };
        stage.error_site = shape.error_site;
        stage.in_result = true;
        stage.returns = None;
        #[cfg(feature = "ide")]
        let (subrows_before, outer_services) =
            (self.cx.subrows.len(), self.cx.stage.services.clone());
        let outer = mem::replace(&mut self.cx.stage, stage);
        let actual_value = match (body, shape.value.as_ref()) {
            (Some(body), Some(val_exp)) => self.check_value(body, val_exp),
            (Some(body), None) => self.infer(body),
            (None, _) => Type::Error,
        };
        let inner = mem::replace(&mut self.cx.stage, outer);
        self.cx.stage.runtime |= inner.runtime;
        self.cx.stage.calls = inner.calls;
        #[cfg(feature = "ide")]
        self.note_result_services(span, subrows_before, &outer_services);
        if let Some(bound) = &shape.captures {
            self.hold(super::regions::Hold {
                held: vec![(None, actual_value.clone())],
                visible: super::regions::Visible::Now(Vec::new()),
                bound: bound.clone(),
                expected: expected.cloned(),
                written: false,
                what: "value",
                span: self.span(node),
            });
        }
        let result_ty = Type::apps(Type::lang(Lang::Result), [errors_ty, actual_value.clone()]);
        let actual = if shape.captures.is_some() {
            expected.cloned().unwrap_or(result_ty)
        } else {
            let captures = self.captures_of(&actual_value);
            if captures.entries.is_empty() && captures.tails.is_empty() {
                result_ty
            } else {
                self.data_value(&result_ty, captures)
            }
        };
        if let Some(expected) = shape.mismatch {
            self.expect(&actual, &expected, self.span(node));
            return actual;
        }
        expected.cloned().unwrap_or(actual)
    }

    /// Decomposes an expected `Result` type for `result { … }`, unifying its
    /// error argument with `errors_ty` when compatible.
    fn result_expected(&mut self, expected: Option<&Type>, errors_ty: &Type) -> ResultExpected {
        let Some(expected) = expected else {
            return ResultExpected::default();
        };
        let (target, caps) = self.whnf_captured(expected);
        let (head, args) = target.spine();
        let head = self.whnf(head);
        let [exp_err, exp_ok] = args[..] else {
            return ResultExpected {
                mismatch: Some(expected.clone()),
                ..ResultExpected::default()
            };
        };
        if !head.is_lang(Lang::Result) {
            return ResultExpected {
                mismatch: Some(expected.clone()),
                ..ResultExpected::default()
            };
        }
        let (exp_err, exp_ok) = (exp_err.clone(), exp_ok.clone());
        let mut out = ResultExpected {
            value: Some(
                caps.as_ref()
                    .map_or_else(|| exp_ok.clone(), |c| self.data_field(&exp_ok, c)),
            ),
            captures: caps,
            ..ResultExpected::default()
        };
        if self.whnf(&exp_err) == Type::Error {
            out.suppress_errors = true;
        } else if self
            .inference_trial(|this| this.unify(errors_ty, &exp_err))
            .is_ok()
        {
            out.error_site = Some(self.expected_site(expected, Written::default()));
        } else {
            out.mismatch = Some(expected.clone());
        }
        out
    }

    /// Migration diagnostic: `result { field = value }` when a callable
    /// `result` is in scope suggests parenthesizing `(result) { … }`.
    fn result_record_call(&mut self, body: &Node, kw_span: Span) -> bool {
        if !self.assigns_fields(body, true) || !self.result_callable_in_scope() {
            return false;
        }
        let diagnostic = Diagnostic::error(
            codes::T074,
            "`result { … }` starts a result block, not a call to `result` with a record",
            kw_span,
        );
        let edits = vec![Edit {
            span: kw_span,
            replacement: "(result)".to_owned(),
        }];
        self.diagnostics.push(
            diagnostic.with_fix(|| ("parenthesize `(result)` to call it with a record", edits)),
        );
        true
    }

    /// Whether a value named `result` is in scope and may be a function.
    fn result_callable_in_scope(&self) -> bool {
        let is_callable = |ty: &Type| {
            let mut head = self.whnf(ty);
            while let Type::Forall { body, .. } = head {
                head = self.whnf(&body);
            }
            matches!(head, Type::Fun { .. } | Type::Meta(_))
        };
        if let Some(local) = self.cx.locals.iter().rev().find(|l| l.name == "result") {
            return is_callable(&local.ty);
        }
        if let Found::Entity(entity) = self.env.find(Namespace::Value, "result") {
            return self.env.value(&entity).is_none_or(is_callable);
        }
        false
    }

    #[cfg(feature = "ide")]
    fn note_result_services(
        &mut self,
        kw_span: Span,
        subrows_before: usize,
        outer_services: &Type,
    ) {
        let mut entries = Vec::new();
        for c in &self.cx.subrows[subrows_before..] {
            if c.errors {
                continue;
            }
            if c.declared == *outer_services {
                entries.push((c.actual.clone(), Vec::new(), c.provides.clone()));
            } else if let Type::Row(r) = &c.declared
                && r.tail
                    .as_deref()
                    .is_some_and(|t| tail_contains(t, outer_services))
            {
                let excluded = r.entries.iter().map(|(l, _)| l.clone()).collect();
                entries.push((c.actual.clone(), excluded, c.provides.clone()));
            }
        }
        self.typing
            .pending_result_services
            .push((super::typing::range(kw_span), entries));
    }
}

/// Decomposed expected `Result` type for `result { … }`.
#[derive(Default)]
struct ResultExpected {
    value: Option<Type>,
    captures: Option<Type>,
    error_site: Option<usize>,
    suppress_errors: bool,
    mismatch: Option<Type>,
}

#[cfg(feature = "ide")]
fn tail_contains(tail: &Type, outer: &Type) -> bool {
    if tail == outer {
        return true;
    }
    match tail {
        Type::Union(a, b) => tail_contains(a, outer) || tail_contains(b, outer),
        Type::Without { row, .. } => tail_contains(row, outer),
        _ => false,
    }
}
