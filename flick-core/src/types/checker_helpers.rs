impl Checker<'_> {
    fn span(&self, node: &Node) -> Span {
        let (start, end) = node.range();
        Span::new(self.file, start, end)
    }

    fn token_span(&self, token: Token) -> Span {
        Span::new(self.file, token.start, token.end)
    }

    /// The span of a node's own (possibly qualified) name tokens, without
    /// the arguments a pattern or type application carries as child nodes.
    fn name_span(&self, node: &Node) -> Span {
        let mut names = node
            .child_tokens()
            .filter(|t| matches!(t.kind, K::Con | K::Ident | K::Dot));
        let Some(first) = names.next() else {
            return self.span(node);
        };
        let end = names.last().map_or(first.end, |t| t.end);
        Span::new(self.file, first.start, end)
    }

    /// Source text of a node with its line breaks and runs of spaces collapsed.
    fn text_of(&self, node: &Node) -> String {
        let (start, end) = node.range();
        self.text[start as usize..end as usize]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn error(&mut self, code: Code, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error(code, message, span));
    }

    /// An error that offers `guess` as the replacement of `span`'s text.
    fn error_guess(
        &mut self,
        code: Code,
        span: Span,
        message: impl Into<String>,
        guess: impl FnOnce(&Self) -> Option<String>,
    ) {
        let diagnostic = Diagnostic::error(code, message, span).did_you_mean(span, || guess(self));
        self.diagnostics.push(diagnostic);
    }

    fn warning(&mut self, code: Code, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::new(code, Severity::Warning, message).with_span(span));
    }

    fn duplicate_label(&mut self, label: &str, span: Span) {
        self.error(
            codes::T014,
            span,
            format!("field `{label}` is given more than once; labels of a record are unique"),
        );
    }
}
