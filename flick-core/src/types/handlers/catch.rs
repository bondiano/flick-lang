impl Checker<'_> {
    /// `body catch | E p [when g] -> recovery …`: typed-error-only sugar over
    /// `try body with | E p [when g] -> recovery …`. Rejects operation arms,
    /// `return` arms, `Fail.fail p` arms, and `resume`.
    pub fn catch_expr(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let ty = expected.cloned().unwrap_or_else(|| self.fresh());
        let arms: Vec<&Node> = node.nodes().filter(|n| n.kind == K::MatchArm).collect();
        let span = self.keyword_span(node, K::CatchKw);
        let raised = self.store.fresh_error_row();
        let mut stage = self.cx.stage.clone();
        (
            stage.errors,
            stage.error_site,
            stage.in_result,
            stage.returns,
        ) = (raised.clone(), None, false, None);
        let outer = mem::replace(&mut self.cx.stage, stage);
        if let Some(body) = node.nodes().next() {
            self.check(body, &ty);
        }
        let inner = mem::replace(&mut self.cx.stage, outer);
        self.cx.stage.runtime |= inner.runtime;
        self.cx.stage.calls = inner.calls;
        let mut errors = Caught::default();
        let returns = self.cx.stage.returns.take();
        self.cx.catch_arm += 1;
        for arm in &arms {
            if !self.reject_catch_arm(node, arm, &raised, &ty, &mut errors) {
                self.error_arm(arm, &raised, &ty, &mut errors);
            }
        }
        self.cx.catch_arm -= 1;
        self.cx.stage.returns = returns;
        if !errors.all && self.cx.stage.pure.is_none() {
            self.unhandled_errors(&raised, errors.handled, span);
        }
        ty
    }

    /// Diagnoses arms rejected in `catch` (`return`, `S.op`, `Fail.fail p`),
    /// returning `true` when the arm was handled here.
    fn reject_catch_arm(
        &mut self,
        node: &Node,
        arm: &Node,
        raised: &Type,
        ty: &Type,
        errors: &mut Caught,
    ) -> bool {
        if arm.has_token(K::ReturnKw) {
            let err_span = return_arm_span(arm, self.file);
            self.push_catch_try_diagnostic(
                node,
                err_span,
                "`catch` handles typed errors only; use `try … with` for a `return` arm",
            );
            let _ = self.arm(arm, ty, ty);
            return true;
        }
        let Some(pattern) = arm_pattern(arm) else {
            return false;
        };
        if let Some((qualifier, name)) = operation_path(pattern, self.text) {
            let err_span = self.span(pattern);
            self.push_catch_try_diagnostic(
                node,
                err_span,
                format!(
                    "`catch` handles typed errors only; for `{qualifier}.{name}`, use `try … with`"
                ),
            );
            self.unknown_operation_arm(arm, ty);
            return true;
        }
        if let Some(inner_pat) = failed_pattern(pattern, self.text) {
            let err_span = self.span(pattern);
            let direct = unwrap_paren_pat(inner_pat).text(self.text);
            let diagnostic = Diagnostic::error(
                codes::E007,
                "`catch` takes a direct error pattern, not `Fail.fail …`: write the error pattern directly, or use `try … with`",
                err_span,
            )
            .with_fix(|| {
                (
                    format!("replace with `{direct}`"),
                    vec![Edit {
                        span: err_span,
                        replacement: direct,
                    }],
                )
            });
            self.diagnostics.push(diagnostic);
            self.error_arm(arm, raised, ty, errors);
            return true;
        }
        false
    }

    fn push_catch_try_diagnostic(
        &mut self,
        node: &Node,
        err_span: Span,
        message: impl Into<String>,
    ) {
        let mut diagnostic = Diagnostic::error(codes::E007, message, err_span);
        if let Some(rewrite) = catch_to_try_text(node, self.text) {
            let full = self.span(node);
            diagnostic = diagnostic.with_fix(|| {
                (
                    "rewrite as `try … with`",
                    vec![Edit {
                        span: full,
                        replacement: rewrite,
                    }],
                )
            });
        }
        self.diagnostics.push(diagnostic);
    }
}

fn unwrap_paren_pat(mut pattern: &Node) -> &Node {
    while pattern.kind == K::ParenPat {
        let Some(inner) = pattern.nodes().find(|n| is_pattern(n.kind)) else {
            break;
        };
        pattern = inner;
    }
    pattern
}

fn return_arm_span(arm: &Node, file: FileId) -> Span {
    let start = arm
        .child_tokens()
        .find(|t| t.kind == K::ReturnKw)
        .map_or(arm.range().0, |t| t.start);
    let end = arm_pattern(arm).map_or_else(
        || {
            arm.child_tokens()
                .find(|t| t.kind == K::ReturnKw)
                .map_or(arm.range().1, |t| t.end)
        },
        |p| p.range().1,
    );
    Span::new(file, start, end)
}

/// Rewrites a `CatchExpr` into an equivalent `try … with` preserving comments,
/// guards, arm order, bindings, and layout boundaries.
pub(crate) fn catch_to_try_text(node: &Node, text: &str) -> Option<String> {
    if node.kind != K::CatchExpr {
        return None;
    }
    let lhs = node.nodes().next()?;
    let catch_kw = node.child_tokens().find(|t| t.kind == K::CatchKw)?;
    if !node.nodes().any(|n| n.kind == K::MatchArm) {
        return None;
    }
    let lhs_range = lhs.range();
    let node_range = node.range();
    let lhs_text = &text[lhs_range.0 as usize..lhs_range.1 as usize];
    let between = &text[lhs_range.1 as usize..catch_kw.start as usize];
    let after = &text[catch_kw.end as usize..node_range.1 as usize];
    if !lhs_text.contains('\n') {
        return Some(format!("try {lhs_text}{between}with{after}"));
    }
    let line_start = text[..node_range.0 as usize]
        .rfind('\n')
        .map_or(0, |i| i + 1);
    let line = &text[line_start..];
    let base_indent = &line[..line.len() - line.trim_start_matches(' ').len()];
    let mut indented = String::new();
    for (i, part) in lhs_text.split('\n').enumerate() {
        if i > 0 {
            indented.push('\n');
        }
        if i == 0 {
            indented.push_str(base_indent);
            indented.push_str("  ");
            indented.push_str(part);
        } else if part.trim().is_empty() {
            // keep blank lines free of trailing spaces
        } else {
            indented.push_str("  ");
            indented.push_str(part);
        }
    }
    if between.contains('\n') {
        Some(format!("try\n{indented}{between}with{after}"))
    } else {
        let trimmed = between.trim_end();
        Some(format!(
            "try\n{indented}{trimmed}\n{base_indent}with{after}"
        ))
    }
}
