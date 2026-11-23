impl Checker<'_> {
    /// Checks metadata word names on a `foreign` declaration that had no
    /// signature, so unknown foreign metadata words (`FLICK-S011`) are still
    /// reported at the exact word span.
    pub(super) fn check_foreign_meta_words(&mut self, decl: &Node) {
        let meta: Vec<&Node> = decl
            .nodes()
            .filter(|n| n.kind == K::ForeignMeta)
            .flat_map(Node::nodes)
            .collect();
        for item in meta {
            let Some(name) = item.child_tokens().find(|t| t.kind == K::Ident) else {
                continue;
            };
            let word = name.text(self.text);
            if core_foreign_meta(word).is_none()
                && self.options.foreign_backend.adapter(word).is_none()
                && self.options.validate_foreign_metadata
            {
                let expected = self.options.foreign_backend.all_foreign_words();
                self.error(
                    codes::S011,
                    self.token_span(name),
                    format!(
                        "unknown foreign metadata `{word}`; expected one of: {}",
                        expected.join(", ")
                    ),
                );
            }
        }
    }

    /// Reads the metadata lines of `decl` into `info`; core contract words are
    /// validated directly, while backend-declared adapter words are parsed
    /// against `options.foreign_backend` and stored in `info.adapter.words`
    /// with their exact source spans.
    fn read_meta(&mut self, decl: &Node, scheme: &Type) -> PendingForeign {
        let mut pending = PendingForeign {
            decl: decl.clone(),
            scheme: scheme.clone(),
            info: ForeignInfo::default(),
            retention: None,
            arity: Vec::new(),
            catch: Vec::new(),
            span: self.span(decl),
            name: super::toplevel::foreign_name(decl, self.text)
                .unwrap_or_default()
                .into(),
            unknown: None,
        };
        let pure = foreign_words(decl)
            .0
            .is_some_and(|word| word.text(self.text) == "pure");
        let info = &mut pending.info;
        let mut retention = None;
        let mut seen_backend: BTreeSet<String> = BTreeSet::new();
        let meta: Vec<&Node> = decl
            .nodes()
            .filter(|n| n.kind == K::ForeignMeta)
            .flat_map(Node::nodes)
            .collect();
        for item in &meta {
            let Some(name) = item.child_tokens().find(|t| t.kind == K::Ident) else {
                continue;
            };
            let args: Vec<&Node> = item.nodes().collect();
            let word = |i: usize| args.get(i).map(|n| n.text(self.text).trim().to_owned());
            let number = |i: usize| word(i).and_then(|w| w.parse::<usize>().ok());
            match name.text(self.text) {
                "arity" => {
                    info.arity = number(0);
                    pending.arity.push(((*item).clone(), info.arity));
                }
                "reviewed" if !word(0).is_some_and(|w| w.starts_with('"')) => self.bad_meta(
                    item,
                    "`reviewed` takes the package and fingerprint the binding was checked against, as a string",
                ),
                "reviewed" => {}
                "nullable" => info.adapter.result.nullable = true,
                "optional" => match number(0) {
                    Some(i) => info.adapter.optional.push(i),
                    None => self.bad_meta(
                        item,
                        "`optional` takes the position of an `Option` argument, from 0",
                    ),
                },
                "owned" => match number(0) {
                    Some(i) => info.adapter.owned.push(i),
                    None => self.bad_meta(item, "`owned` takes the position of an argument, from 0"),
                },
                "retention" => match word(0).as_deref() {
                    Some("call") => retention = Some(Retention::Call),
                    Some("owner") => retention = Some(Retention::Owner),
                    _ => self.bad_meta(item, "write `retention call` or `retention owner`"),
                },
                "catch" => match args.first() {
                    Some(mapping) if !pure => {
                        let error = self.infer_catch_mapping(mapping);
                        pending
                            .catch
                            .push(((*item).clone(), (*mapping).clone(), error));
                        info.catch = Some(mapping.range());
                    }
                    Some(_) => self.bad_meta(
                        item,
                        "a `foreign pure` call cannot throw: `catch` needs another mode",
                    ),
                    None => self.bad_meta(
                        item,
                        "`catch` takes the function that maps a JS exception to an error",
                    ),
                },
                other => {
                    self.read_backend_meta_item(
                        item,
                        name,
                        other,
                        &args,
                        &mut seen_backend,
                        &mut info.adapter.words,
                    );
                }
            }
        }
        pending.retention = retention;
        pending
    }

    fn read_backend_meta_item(
        &mut self,
        item: &Node,
        name: Token,
        word: &str,
        args: &[&Node],
        seen_backend: &mut BTreeSet<String>,
        words: &mut Vec<BackendAdapter>,
    ) {
        if let Some(spec) = self.options.foreign_backend.adapter(word).cloned() {
            if !spec.repeatable && !seen_backend.insert(spec.name.clone()) {
                self.bad_meta(item, &format!("duplicate foreign metadata `{word}`"));
                return;
            }
            if let Some(parsed) = self.parse_backend_adapter(item, &spec, args) {
                words.push(parsed);
            }
        } else if self.options.validate_foreign_metadata {
            let expected = self.options.foreign_backend.all_foreign_words();
            self.error(
                codes::S011,
                self.token_span(name),
                format!(
                    "unknown foreign metadata `{word}`; expected one of: {}",
                    expected.join(", ")
                ),
            );
        }
    }

    fn parse_backend_adapter(
        &mut self,
        item: &Node,
        spec: &AdapterWordSpec,
        args: &[&Node],
    ) -> Option<BackendAdapter> {
        let usage = || -> String {
            if spec.args.is_empty() {
                return format!("`{}` takes no arguments", spec.name);
            }
            if let [arg] = spec.args.as_slice()
                && let AdapterArgKind::Keyword(choices) = &arg.kind
            {
                let forms: Vec<String> = choices
                    .iter()
                    .map(|choice| format!("`{} {choice}`", spec.name))
                    .collect();
                return format!("write {}", forms.join(" or "));
            }
            format!("write `{}`", spec.synopsis())
        };
        if args.len() != spec.args.len() {
            self.bad_meta(item, &usage());
            return None;
        }
        let mut parsed = Vec::with_capacity(spec.args.len());
        for (node, arg_spec) in args.iter().zip(&spec.args) {
            let raw_text = node.text(self.text);
            let raw = raw_text.trim();
            let value = match &arg_spec.kind {
                AdapterArgKind::Keyword(choices) => {
                    if choices.iter().any(|choice| choice == raw) {
                        AdapterArg::Word(raw.to_owned())
                    } else {
                        self.bad_meta(item, &usage());
                        return None;
                    }
                }
                AdapterArgKind::ParamIndex | AdapterArgKind::Uint => {
                    let Ok(n) = raw.parse::<u64>() else {
                        self.bad_meta(item, &usage());
                        return None;
                    };
                    AdapterArg::Int(n)
                }
                AdapterArgKind::Text => {
                    let Some(raw) = plain_string_literal(self.text, node) else {
                        self.bad_meta(item, &usage());
                        return None;
                    };
                    AdapterArg::String(literal::string_value(raw))
                }
                AdapterArgKind::Word => {
                    if raw
                        .bytes()
                        .next()
                        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                        && raw
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    {
                        AdapterArg::Word(raw.to_owned())
                    } else {
                        self.bad_meta(item, &usage());
                        return None;
                    }
                }
            };
            parsed.push(value);
        }
        Some(BackendAdapter {
            name: spec.name.clone(),
            args: parsed,
            span: self.span(item),
        })
    }

    /// Checks core contract metadata in `info` against the parameters of the signature.
    fn check_meta(&mut self, info: &ForeignInfo, params: &[Type], body: &Type, span: Span) {
        let arity = info.js_arity();
        let stages = &info.stages;
        if arity == 0
            && !stages.is_empty()
            && !params.first().is_some_and(|p| p.is_lang(Lang::Unit))
        {
            self.error(
                codes::F001,
                span,
                "`arity 0` drops the first argument, which must be `()`".to_owned(),
            );
        }
        for &i in &info.adapter.optional {
            if !params
                .get(i)
                .is_some_and(|p| self.whnf(p).spine().0.is_lang(Lang::Option))
                || i >= arity
            {
                self.error(
                    codes::F001,
                    span,
                    format!("`optional {i}` names an argument of type `Option a`"),
                );
            }
        }
        for &i in &info.adapter.owned {
            if i >= arity {
                self.error(
                    codes::F001,
                    span,
                    format!("`owned {i}` names an argument past the {arity} parameters of the call"),
                );
            }
        }
        if info.adapter.result.nullable
            && !self
                .call_result(body, arity)
                .is_some_and(|r| self.whnf(&r).spine().0.is_lang(Lang::Option))
        {
            self.error(
                codes::F001,
                span,
                "`nullable` makes the result an `Option a`".to_owned(),
            );
        }
    }
}
