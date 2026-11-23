impl Checker<'_> {
    /// Reads the metadata lines of `decl` into `info`; the retention, if
    /// written. Core contract words are handled here; backend-declared adapter
    /// words are parsed according to the active backend's descriptor and
    /// carried on `Adapter.words` for backend validation and code generation.
    fn read_meta(&mut self, decl: &Node, scheme: &Type) -> PendingForeign {
        let mut pending = PendingForeign {
            decl: decl.clone(),
            scheme: scheme.clone(),
            info: ForeignInfo::default(),
            retention: None,
            arity: Vec::new(),
            owned: Vec::new(),
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
                    None => self.bad_meta(item, "`optional` takes the position of an `Option` argument, from 0"),
                },
                "owned" => match number(0) {
                    Some(i) if args.len() == 1 => {
                        if info.adapter.owned.contains(&i) {
                            self.bad_meta(item, &format!("duplicate `owned {i}` in foreign metadata"));
                        } else {
                            info.adapter.owned.push(i);
                            pending.owned.push(((*item).clone(), i));
                        }
                    }
                    _ => self.bad_meta(item, "`owned` takes the position of an argument, from 0"),
                },
                "retention" => match word(0).as_deref() {
                    Some("call") => retention = Some(Retention::Call),
                    Some("owner") => retention = Some(Retention::Owner),
                    _ => self.bad_meta(item, "write `retention call` or `retention owner`"),
                },
                "catch" => match args.first() {
                    Some(mapping) if !pure => {
                        let error = self.infer_catch_mapping(mapping);
                        pending.catch.push(((*item).clone(), (*mapping).clone(), error));
                        info.catch = Some(mapping.range());
                    }
                    Some(_) => self.bad_meta(item, "a `foreign pure` call cannot throw: `catch` needs another mode"),
                    None => self.bad_meta(item, "`catch` takes the function that maps a JS exception to an error"),
                },
                other => {
                    let Some(spec) = self.foreign_meta().adapter_word(other).cloned() else {
                        // Unknown metadata words are reported by `syntax::foreign_meta::validate_tree` (`FLICK-S011`).
                        continue;
                    };
                    if !spec.repeatable && info.adapter.has_word(&spec.name) {
                        self.bad_meta(
                            item,
                            &format!("duplicate `{}` in foreign metadata", spec.name),
                        );
                        continue;
                    }
                    match parse_adapter_word_args(self.text, &args, &spec) {
                        Ok(parsed_args) => {
                            info.adapter.words.push(flick_ir::AdapterWord {
                                name: spec.name,
                                args: parsed_args,
                                span: self.span(item),
                            });
                        }
                        Err(message) => self.bad_meta(item, &message),
                    }
                }
            }
        }
        pending.retention = retention;
        pending
    }

    /// Checks the core contract metadata in `info` against the parameters of
    /// the signature. Backend-owned adapter words are validated by the target
    /// backend in `check-target`.
    fn check_meta(
        &mut self,
        info: &ForeignInfo,
        owned: &[(Node, usize)],
        params: &[Type],
        body: &Type,
        span: Span,
    ) {
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
        for (item, i) in owned {
            if *i >= arity {
                self.bad_meta(
                    item,
                    &format!("`owned {i}` is past the {arity} arguments of the call"),
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

fn parse_adapter_word_args(
    text: &str,
    args: &[&Node],
    spec: &AdapterWordSpec,
) -> Result<Vec<flick_ir::AdapterArg>, String> {
    let err = || adapter_usage_error(spec);
    if args.len() != spec.args.len() {
        return Err(err());
    }
    let mut parsed = Vec::with_capacity(spec.args.len());
    for (node, arg_spec) in args.iter().zip(&spec.args) {
        let raw_text = node.text(text);
        let raw = raw_text.trim();
        match arg_spec.kind {
            AdapterArgKind::Index => {
                let value = raw.parse::<usize>().map_err(|_| err())?;
                parsed.push(flick_ir::AdapterArg::Index(value));
            }
            AdapterArgKind::Ident => {
                if !is_ident(raw) {
                    return Err(err());
                }
                if !arg_spec.choices.is_empty() && !arg_spec.choices.iter().any(|c| c == raw) {
                    return Err(err());
                }
                parsed.push(flick_ir::AdapterArg::Ident(raw.to_owned()));
            }
            AdapterArgKind::Text => {
                let Some(raw) = plain_string_literal(text, node) else {
                    return Err(err());
                };
                let decoded = literal::string_value(raw);
                if !arg_spec.choices.is_empty() && !arg_spec.choices.iter().any(|c| c == &decoded) {
                    return Err(err());
                }
                parsed.push(flick_ir::AdapterArg::Text(decoded));
            }
        }
    }
    Ok(parsed)
}

/// Returns the raw token slice when `node` is a single plain string literal
/// (`K::Literal` with one non-trivia `K::String` token and no child nodes),
/// rejecting postfix expressions (`"a".b`, `"a"()`) and string interpolation.
fn plain_string_literal<'a>(text: &'a str, node: &Node) -> Option<&'a str> {
    if node.kind != K::Literal || node.nodes().next().is_some() {
        return None;
    }
    let mut tokens = node.child_tokens();
    let token = tokens.next()?;
    if token.kind != K::String || tokens.next().is_some() {
        return None;
    }
    Some(token.text(text))
}

fn is_ident(raw: &str) -> bool {
    let mut chars = raw.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn adapter_usage_error(spec: &AdapterWordSpec) -> String {
    if spec.args.is_empty() {
        return format!("`{}` takes no arguments", spec.name);
    }
    if let [only] = spec.args.as_slice()
        && only.kind == AdapterArgKind::Ident
        && let [choice] = only.choices.as_slice()
    {
        return format!("write `{} {choice}`", spec.name);
    }
    let parts: Vec<String> = spec
        .args
        .iter()
        .map(|arg| {
            if arg.choices.is_empty() {
                match arg.kind {
                    AdapterArgKind::Index => format!("<{}: index>", arg.name),
                    AdapterArgKind::Ident => format!("<{}>", arg.name),
                    AdapterArgKind::Text => format!("\"<{}>\"", arg.name),
                }
            } else {
                arg.choices.join(" | ")
            }
        })
        .collect();
    format!("`{}` expects `{}`", spec.name, parts.join(" "))
}
