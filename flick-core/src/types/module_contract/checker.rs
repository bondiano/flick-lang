impl super::Checker<'_> {
    pub(super) fn unpack_module(&mut self, node: &Node) -> Type {
        let mut children = node.nodes();
        let Some(source) = children.next() else {
            return Type::Error;
        };
        let Some(body) = children.find(|n| n.kind == K::Block) else {
            return Type::Error;
        };
        let Some(alias) = node.child_tokens().find(|t| t.kind == K::Con) else {
            return Type::Error;
        };
        let alias = alias.text(self.text).to_owned();
        let mut tokens = node.child_tokens();
        let payload = tokens
            .find(|t| t.kind == K::Comma)
            .and_then(|_| tokens.find(|t| t.kind == K::Ident));
        let witness = format!("{}.$unpack{}", self.env.module, node.range().0);
        let Some((signature, opened_value)) =
            self.unpacked_signature(node, source, payload.is_some(), &witness)
        else {
            return Type::Error;
        };
        let Type::Con { module, name } = &signature else {
            self.error(
                codes::T002,
                self.span(source),
                "`unpack` requires a named `type module` contract",
            );
            return Type::Error;
        };
        let Some(def) = self.env.type_def(module, name) else {
            self.error(
                codes::T001,
                self.span(source),
                format!("module contract `{module}.{name}` is not available here"),
            );
            return Type::Error;
        };
        let Form::ModuleContract { requirements } = &def.form else {
            self.error(
                codes::T002,
                self.span(source),
                format!("`{module}.{name}` is not a `type module` contract"),
            );
            return Type::Error;
        };
        let requirements = requirements.clone();
        let (subst, types, values) = self.opened_requirements(&requirements, &witness);
        let local_mark = self.classes.local.len();
        for requirement in &requirements {
            if let ModuleRequirement::Dictionary { head, from } = requirement {
                self.classes.local.push((
                    head.substitute(&subst),
                    super::Dict::Opened {
                        binder: alias.clone(),
                        field: from.clone(),
                    },
                ));
            }
        }
        self.opened_modules.push(Opened {
            alias: alias.clone(),
            types,
            values,
        });
        let value_mark = self.locals.len();
        if let (Some(payload), Some(value)) = (payload, opened_value) {
            self.push_local(payload.text(self.text), payload.start, value);
        }
        let result = self.block(body, None);
        self.locals.truncate(value_mark);
        self.opened_modules.pop();
        self.classes.local.truncate(local_mark);
        if result.contains(&mut |ty| matches!(ty, Type::Con { module, .. } if module == &witness)) {
            self.typing.existential_unpacks.insert(node.range());
            Type::Exists {
                binder: alias.clone(),
                contract: Box::new(Type::con(module, name)),
                body: Box::new(close_existential_type(&result, &witness, &alias)),
            }
        } else {
            result
        }
    }

    fn unpacked_signature(
        &mut self,
        node: &Node,
        source: &Node,
        payload: bool,
        witness: &str,
    ) -> Option<(Type, Option<Type>)> {
        let source_ty = self.infer(source);
        let (unwrapped, _) = self.whnf_captured(&source_ty);
        let (signature, opened_value) = if let Type::Exists {
            binder,
            contract,
            body,
        } = &unwrapped
        {
            self.typing.existential_opens.insert(node.range());
            if !payload {
                self.error(
                    codes::T002,
                    self.span(node),
                    "opening an existential value requires `unpack value as K, item in ...`",
                );
                return None;
            }
            (
                (**contract).clone(),
                Some(open_existential_type(body, binder, witness)),
            )
        } else {
            if payload {
                self.error(
                    codes::T002,
                    self.span(node),
                    "`Module Sig` has no carried value; write `unpack packed as K in ...`",
                );
                return None;
            }
            let (head, args) = unwrapped.spine();
            let [signature] = &args[..] else {
                self.error(
                    codes::T002,
                    self.span(source),
                    "`unpack` requires `Module Sig` or `exists K : Sig. T`",
                );
                return None;
            };
            if !matches!(head, Type::Con { module, name } if module == "Prelude" && name == "Module")
            {
                self.error(
                    codes::T002,
                    self.span(source),
                    "`unpack` requires `Module Sig` or `exists K : Sig. T`",
                );
                return None;
            }
            ((*signature).clone(), None)
        };
        Some((signature, opened_value))
    }

    fn opened_requirements(
        &self,
        requirements: &[ModuleRequirement],
        witness: &str,
    ) -> OpenedRequirements {
        let mut subst = Vec::new();
        let mut types = Vec::new();
        for requirement in requirements {
            match requirement {
                ModuleRequirement::Type {
                    name,
                    params,
                    manifest,
                } => {
                    let ty = manifest
                        .as_ref()
                        .map_or_else(|| Type::con(witness, name), |ty| ty.substitute(&subst));
                    let kind = super::Kind::arrows(
                        params.iter().map(|p| p.kind.clone()),
                        super::Kind::Type,
                    );
                    subst.push((name.clone(), ty.clone()));
                    types.push((name.clone(), ty, kind));
                }
                ModuleRequirement::Nominal {
                    name,
                    identity: Some(identity),
                    ..
                } => {
                    let ty = Type::con(&identity.module, &identity.name);
                    let kind = self
                        .env
                        .type_def(&identity.module, &identity.name)
                        .map_or(super::Kind::Type, |def| def.kind.clone());
                    subst.push((name.clone(), ty.clone()));
                    types.push((name.clone(), ty, kind));
                }
                _ => {}
            }
        }
        let values = requirements
            .iter()
            .filter_map(|requirement| match requirement {
                ModuleRequirement::Value { name, ty } => {
                    Some((name.clone(), ty.substitute(&subst)))
                }
                _ => None,
            })
            .collect();
        (subst, types, values)
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn pack_module(&mut self, node: &Node) -> Type {
        let Some(path) = node.nodes().find(|n| n.kind == K::ModulePath) else {
            return Type::Error;
        };
        let Some(contract_node) = node.nodes().find(|n| super::is_type(n.kind)) else {
            return Type::Error;
        };
        let signature = self.ty(
            contract_node,
            &super::Kind::Module,
            super::convert::Free::Reject,
        );
        let Type::Con { module, name } = &signature else {
            return Type::Error;
        };
        let Some(contract) = self.env.type_def(module, name).cloned() else {
            return Type::Error;
        };
        let Form::ModuleContract { requirements } = &contract.form else {
            self.error(
                codes::T068,
                self.span(contract_node),
                "a static parameterized module cannot be packed as a runtime module",
            );
            return Type::Error;
        };
        if requirements
            .iter()
            .any(|r| matches!(r, ModuleRequirement::Module { .. }))
        {
            self.error(
                codes::T068,
                self.span(contract_node),
                "nested static modules cannot be packed as a runtime module; flatten the runtime contract",
            );
            return Type::Error;
        }
        let module_name: String = path.text(self.text).split_whitespace().collect();
        let Some((_, Some(actual))) = self.env.qualified_module(&module_name) else {
            self.error(
                codes::T001,
                self.span(path),
                format!("unknown named module `{module_name}` in `pack`"),
            );
            return Type::Error;
        };
        let actual = actual.clone();
        let mut failed = false;
        for requirement in requirements {
            if let ModuleRequirement::Nominal {
                name,
                identity: None,
                ..
            } = requirement
            {
                self.error(
                    codes::T063,
                    self.span(contract_node),
                    format!("`Module {}` cannot carry abstract class or service `{name}`; fix its identity with `class {name} = Path.{name}` or `service {name} = Path.{name}`", self.text_of(contract_node)),
                );
                failed = true;
            }
        }
        for mismatch in check(&contract, &actual) {
            let (code, message) = match mismatch {
                Mismatch::Missing(member) => (
                    codes::T060,
                    format!(
                        "module `{module_name}` does not export required member `{member}` of `{}`",
                        self.text_of(contract_node)
                    ),
                ),
                Mismatch::NominalIdentity(member) => (
                    codes::T062,
                    format!(
                        "module `{module_name}` exports `{member}` with a different nominal identity than required by `{}`",
                        self.text_of(contract_node)
                    ),
                ),
                Mismatch::Kind(member)
                | Mismatch::TypeEquality(member)
                | Mismatch::ValueType(member)
                | Mismatch::Shape(member) => (
                    codes::T061,
                    format!(
                        "module `{module_name}` exports `{member}`, but its kind, type or shape does not satisfy `{}`",
                        self.text_of(contract_node)
                    ),
                ),
            };
            self.error(code, self.span(path), message);
            failed = true;
        }
        if failed {
            Type::Error
        } else {
            let fields = requirements
                .iter()
                .filter_map(|requirement| match requirement {
                    ModuleRequirement::Value { name, .. } => {
                        let export = actual.find(Namespace::Value, name)?;
                        Some((
                            name.clone(),
                            export.entity.module.clone(),
                            export.entity.name.clone(),
                        ))
                    }
                    _ => None,
                })
                .collect();
            self.typing.packs.insert(node.range(), fields);
            Type::app(Type::con("Prelude", "Module"), signature)
        }
    }
}
