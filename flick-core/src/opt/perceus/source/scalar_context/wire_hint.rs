use flick_ir::memory::WireHint;

use crate::types::Abi;

impl ScalarContext<'_> {
    /// Derives structural [`WireHint`]s for a foreign declaration's `arity`
    /// arguments and its return value from the checked compiler signature.
    pub(super) fn foreign_wire_hints(
        &self,
        key: &Name,
        arity: usize,
        taken: usize,
    ) -> (Vec<WireHint>, WireHint) {
        let Some(&scheme) = self.values.get(key) else {
            return (vec![WireHint::Any; arity], WireHint::Any);
        };
        let mut observer = Observer {
            context: self,
            frames: vec![vec![]],
        };
        let Ok(mut cursor) = observer.scheme(scheme) else {
            return (vec![WireHint::Any; arity], WireHint::Any);
        };
        let mut arg_hints = Vec::with_capacity(arity);
        let mut consumed = 0;
        while consumed < taken {
            let Ok((head, args)) = observer.head_for_hint(cursor) else {
                break;
            };
            if !args.is_empty() {
                break;
            }
            let Type::Fun { param, result, .. } = head.ty else {
                break;
            };
            if consumed < arity {
                let param_desc = Descriptor {
                    ty: param,
                    environment: head.environment,
                };
                arg_hints.push(observer.wire_hint(param_desc, 0));
            }
            consumed += 1;
            cursor = Descriptor {
                ty: result,
                environment: head.environment,
            };
        }
        while arg_hints.len() < arity {
            arg_hints.push(WireHint::Any);
        }
        let result_hint = observer.wire_hint(cursor, 0);
        (arg_hints, result_hint)
    }
}

impl<'a> Observer<'a, '_> {
    fn head_for_hint(
        &mut self,
        mut descriptor: Descriptor<'a>,
    ) -> Result<(Descriptor<'a>, VecDeque<Descriptor<'a>>), ()> {
        let mut arguments = VecDeque::new();
        for _ in 0..64 {
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
                    let Form::Alias {
                        params,
                        body,
                        opaque,
                    } = &definition.form
                    else {
                        return Ok((descriptor, arguments));
                    };
                    if *opaque || arguments.len() < params.len() || self.context.recursive.contains(&key) {
                        return Ok((descriptor, arguments));
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
        Err(())
    }

    fn wire_hint(&mut self, descriptor: Descriptor<'a>, depth: usize) -> WireHint {
        if depth >= 16 {
            return WireHint::Any;
        }
        let Ok((head, mut args)) = self.head_for_hint(descriptor) else {
            return WireHint::Any;
        };
        match head.ty {
            Type::Forall { vars, body, .. } if args.is_empty() => {
                let env = self.mask(head.environment, vars);
                self.wire_hint(
                    Descriptor {
                        ty: body,
                        environment: env,
                    },
                    depth + 1,
                )
            }
            Type::Tuple(items) if args.is_empty() => WireHint::Tuple(
                items
                    .iter()
                    .map(|item| {
                        self.wire_hint(
                            Descriptor {
                                ty: item,
                                environment: head.environment,
                            },
                            depth + 1,
                        )
                    })
                    .collect(),
            ),
            Type::Record(row) if args.is_empty() => {
                let fields = row
                    .entries
                    .iter()
                    .filter_map(|(elem, ty)| match elem {
                        Elem::Named(label) => Some((
                            label.clone(),
                            self.wire_hint(
                                Descriptor {
                                    ty,
                                    environment: head.environment,
                                },
                                depth + 1,
                            ),
                        )),
                        _ => None,
                    })
                    .collect();
                WireHint::Record(fields)
            }
            Type::Fun { .. } if args.is_empty() => self.fun_wire_hint(head, depth),
            Type::Con { module, name } => self.con_wire_hint(head, module, name, &mut args, depth),
            _ => WireHint::Any,
        }
    }

    fn fun_wire_hint(&mut self, start: Descriptor<'a>, depth: usize) -> WireHint {
        let mut fun_args = Vec::new();
        let mut cursor = start;
        for _ in 0..16 {
            let Ok((head, args)) = self.head_for_hint(cursor) else {
                break;
            };
            if !args.is_empty() {
                return WireHint::Fun {
                    args: fun_args,
                    result: Box::new(self.wire_hint(cursor, depth + 1)),
                };
            }
            match head.ty {
                Type::Forall { vars, body, .. } => {
                    let env = self.mask(head.environment, vars);
                    cursor = Descriptor {
                        ty: body,
                        environment: env,
                    };
                }
                Type::Fun {
                    param,
                    effects,
                    result,
                } => {
                    fun_args.push(self.wire_hint(
                        Descriptor {
                            ty: param,
                            environment: head.environment,
                        },
                        depth + 1,
                    ));
                    cursor = Descriptor {
                        ty: result,
                        environment: head.environment,
                    };
                    if effects.is_some() {
                        break;
                    }
                }
                _ => break,
            }
        }
        WireHint::Fun {
            args: fun_args,
            result: Box::new(self.wire_hint(cursor, depth + 1)),
        }
    }

    fn con_wire_hint(
        &mut self,
        head: Descriptor<'a>,
        module: &str,
        name: &str,
        args: &mut VecDeque<Descriptor<'a>>,
        depth: usize,
    ) -> WireHint {
        if head.ty.is_lang(Lang::Int) {
            return WireHint::Int;
        }
        if head.ty.is_lang(Lang::Float) {
            return WireHint::Float;
        }
        if head.ty.is_lang(Lang::Unit) {
            return WireHint::Unit;
        }
        if head.ty.is_lang(Lang::JsValue) {
            return WireHint::Opaque;
        }
        if head.ty.is_lang(Lang::List) || (module == "NonEmpty" && name == "NonEmpty") {
            let item = args
                .pop_front()
                .map_or(WireHint::Any, |d| self.wire_hint(d, depth + 1));
            return WireHint::List(Box::new(item));
        }
        if head.ty.is_lang(Lang::Option) {
            let item = args
                .pop_front()
                .map_or(WireHint::Any, |d| self.wire_hint(d, depth + 1));
            return WireHint::Option(Box::new(item));
        }
        if head.ty.is_lang(Lang::Result) {
            let err = args
                .pop_front()
                .map_or(WireHint::Any, |d| self.wire_hint(d, depth + 1));
            let ok = args
                .pop_front()
                .map_or(WireHint::Any, |d| self.wire_hint(d, depth + 1));
            return WireHint::Result(Box::new(err), Box::new(ok));
        }
        if head.ty.is_lang(Lang::Eff) {
            // Eff r e a -> 3rd type argument is the result value type.
            let val = args
                .get(2)
                .copied()
                .map_or(WireHint::Any, |d| self.wire_hint(d, depth + 1));
            return WireHint::Eff(Box::new(val));
        }
        if is_known_data_lang(head.ty) || is_known_stdlib_data(module, name) {
            return WireHint::Any;
        }
        let key = Name::new(module, name);
        let Some(def) = self.context.definitions.get(&key) else {
            return WireHint::Any;
        };
        match &def.form {
            Form::Nominal { literals, .. } if literals.is_empty() => WireHint::Opaque,
            Form::Alias { opaque: true, .. } => WireHint::Opaque,
            Form::Adt {
                constructors,
                abi: Abi::Unboxed,
            } => {
                if let Some(ctor) = constructors.first()
                    && let Some(&ctor_scheme) = self.context.values.get(&Name::new(module, ctor))
                    && let Ok(ctor_head) = self.scheme(ctor_scheme)
                    && let Type::Fun { param, .. } = ctor_head.ty
                {
                    return self.wire_hint(
                        Descriptor {
                            ty: param,
                            environment: ctor_head.environment,
                        },
                        depth + 1,
                    );
                }
                WireHint::Any
            }
            _ => WireHint::Any,
        }
    }
}

fn is_known_data_lang(ty: &Type) -> bool {
    ty.is_lang(Lang::String)
        || ty.is_lang(Lang::Bool)
        || ty.is_lang(Lang::BigInt)
        || ty.is_lang(Lang::Unit)
        || ty.is_lang(Lang::Unknown)
        || ty.is_lang(Lang::Json)
        || ty.is_lang(Lang::Never)
        || ty.is_lang(Lang::Exit)
        || ty.is_lang(Lang::Failure)
        || ty.is_lang(Lang::Layer)
        || ty.is_lang(Lang::Schema)
        || ty.is_lang(Lang::Codec)
        || ty.is_lang(Lang::PureCodec)
        || ty.is_lang(Lang::Owner)
        || ty.is_lang(Lang::ServiceTag)
        || ty.is_lang(Lang::SchemaError)
        || ty.is_lang(Lang::AppError)
        || ty.is_lang(Lang::Errors)
        || ty.is_lang(Lang::Redacted)
        || ty.is_lang(Lang::Ref)
        || ty.is_lang(Lang::Seq)
        || ty.is_lang(Lang::Lazy)
}

fn is_known_stdlib_data(module: &str, name: &str) -> bool {
    matches!(
        (module, name),
        ("Bytes", "Bytes")
            | ("BigDecimal", "BigDecimal")
            | ("RegExp", "RegExp")
            | ("Duration", "Duration")
            | ("DateTime", "Utc" | "Zoned" | "TimeZone")
            | ("Cron", "Cron")
            | ("Graph", "Graph" | "Walker")
            | ("Chunk", "Chunk")
            | ("HashMap", "HashMap")
            | ("HashSet", "HashSet")
            | ("Context", "Context" | "Reference")
            | ("Fiber", "Fiber")
            | ("Deferred", "Deferred")
            | ("Layer", "MemoMap")
            | ("Schedule", "Schedule")
            | ("Cause" | "RawCause", "Cause" | "RawCause")
            | ("File", "Path")
    )
}
