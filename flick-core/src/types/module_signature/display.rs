impl ModuleSignature {
    pub fn display(&self) -> String {
        let mut signature = self.clone();
        signature.display_names(&mut Vec::new());
        signature.display_body()
    }

    fn display_names(&mut self, used: &mut Vec<String>) {
        if let Self::Structure { requirements } = self {
            let members: Vec<_> = requirements
                .iter()
                .filter_map(|r| match r {
                    R::Module { binder, name, .. } if !binder.is_empty() => {
                        Some((binder.clone(), name.clone()))
                    }
                    _ => None,
                })
                .collect();
            for (binder, name) in members {
                self.rename_parameter(&binder, &format!("$display:{name}"));
            }
        }
        match self {
            Self::Functor {
                binder,
                parameter,
                result,
                ..
            } => {
                parameter.display_names(&mut used.clone());
                if !binder.is_empty() {
                    let base = binder.rsplit('#').next().unwrap_or(binder).to_owned();
                    let mut name = base.clone();
                    let mut suffix = 2;
                    while used.contains(&name) {
                        name = format!("{base}{suffix}");
                        suffix += 1;
                    }
                    result.rename_parameter(binder, &format!("$display:{name}"));
                    binder.clone_from(&name);
                    used.push(name);
                }
                result.display_names(used);
            }
            Self::Structure { requirements } => {
                for requirement in requirements {
                    if let R::Module { signature, .. } = requirement {
                        signature.display_names(&mut used.clone());
                    }
                }
            }
        }
    }

    fn display_body(&self) -> String {
        match self {
            Self::Functor { group_size, .. } => {
                let mut parameters = Vec::new();
                let mut tail = self;
                for _ in 0..(*group_size).max(1) {
                    let Self::Functor {
                        binder,
                        parameter,
                        result,
                        ..
                    } = tail
                    else {
                        break;
                    };
                    let input = parameter.display_body();
                    parameters.push(if binder.is_empty() {
                        if matches!(parameter.as_ref(), Self::Functor { .. }) {
                            format!("({input})")
                        } else {
                            input
                        }
                    } else {
                        format!("{binder} : {input}")
                    });
                    tail = result;
                }
                let input = parameters.join(", ");
                let bound = matches!(self, Self::Functor { binder, .. } if !binder.is_empty());
                if parameters.len() > 1 || bound {
                    format!("({input}) -> {}", tail.display_body())
                } else {
                    format!("{input} -> {}", tail.display_body())
                }
            }
            Self::Structure { requirements } if requirements.is_empty() => "{}".into(),
            Self::Structure { requirements } => requirements
                .iter()
                .map(|r| match r {
                    R::Type { name, manifest, .. } => manifest.as_ref().map_or_else(
                        || format!("type {name}"),
                        |ty| format!("type {name} = {}", display_type(ty)),
                    ),
                    R::Value { name, ty } => format!("{name} : {}", display_type(ty)),
                    R::Module {
                        name, signature, ..
                    } => {
                        format!("module {name} : {}", signature.display_body())
                    }
                    R::Dictionary { head, from } => {
                        format!("instance {} from {from}", display_type(head))
                    }
                    R::Nominal {
                        name,
                        category,
                        identity,
                        ..
                    } => format!(
                        "{} {name}{}",
                        if matches!(category, ModuleNominalKind::Class) {
                            "class"
                        } else {
                            "service"
                        },
                        identity.as_ref().map_or(String::new(), |e| format!(
                            " = {}.{}",
                            e.module.strip_prefix("$display:").unwrap_or(&e.module),
                            e.name
                        ))
                    ),
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}
