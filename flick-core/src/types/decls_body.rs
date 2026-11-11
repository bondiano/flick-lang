impl Checker<'_> {
    /// Converts what a declaration defines with its parameters in scope.
    fn body<'n>(&mut self, p: &Pending<'n>, refined: &mut Vec<(&'n Node, Type)>) -> Body {
        let decl = p.node;
        match decl.kind {
            K::ModuleTypeDecl => {
                if let Some(signature) = decl.nodes().find(|n| n.kind != K::Members) {
                    match self.static_module_signature(signature) {
                        super::ModuleSignature::Structure { requirements } => {
                            Body::ModuleContract(requirements)
                        }
                        signature @ super::ModuleSignature::Functor { .. } => {
                            Body::ModuleSignature(signature)
                        }
                    }
                } else {
                    Body::ModuleContract(self.module_contract(decl))
                }
            }
            K::TypeDecl => {
                let variants: Vec<&Node> = decl.nodes().filter(|n| n.kind == K::Variant).collect();
                if !variants.is_empty() {
                    return Body::Variants(
                        variants.iter().filter_map(|v| self.variant(v)).collect(),
                    );
                }
                let Some(body) = decl.nodes().find(|n| is_type(n.kind)) else {
                    return Body::None;
                };
                if decl.has_token(K::RefinedKw) {
                    let base = self.ty(body, &Kind::Type, Free::Reject);
                    if let Some(predicate) = decl.nodes().find(|n| n.kind == K::Refinement) {
                        refined.push((predicate, base));
                    }
                    return Body::None;
                }
                let Some(TypeDef { kind, .. }) = self.env.type_def(&self.env.module, &p.name)
                else {
                    return Body::None;
                };
                let mut result = kind.clone();
                for _ in &p.params {
                    result = match self.kinds.resolve(&result) {
                        Kind::Arrow(_, r) => *r,
                        other => other,
                    };
                }
                let converted = self.ty(body, &result, Free::Reject);
                if decl.has_token(K::OpaqueKw) && self.kinds.resolve(&result).is_row() {
                    self.error(
                        codes::T002,
                        self.span(decl),
                        format!(
                            "`{}` cannot be opaque: a set is its row, which a signature that names it must show",
                            p.name
                        ),
                    );
                }
                if let Some(TypeDef {
                    form: Form::Alias { body, .. },
                    ..
                }) = self.env.type_def_mut(&p.name)
                {
                    *body = converted;
                }
                Body::None
            }
            K::ErrorDecl => {
                let payload = decl
                    .nodes()
                    .find(|n| n.kind == K::Variant)
                    .map(|v| self.payload(v))
                    .unwrap_or_default();
                Body::Variants(vec![Variant {
                    name: p.name.clone(),
                    payload,
                    constraints: Vec::new(),
                    indexed: None,
                }])
            }
            K::ServiceDecl => Body::Members(self.operations(decl)),
            K::ClassDecl => {
                let mut supers = Vec::new();
                for context in decl.nodes().filter(|n| n.kind == K::Context) {
                    for c in context.nodes() {
                        supers.extend(self.constraints(c, Free::Reject));
                    }
                }
                let fundeps = decl
                    .nodes()
                    .filter(|n| n.kind == K::Fundep)
                    .filter_map(|f| self.fundep(f, &p.params))
                    .collect();
                let head = Type::apps(
                    Type::con(&self.env.module, &p.name),
                    p.params.iter().map(|b| Type::Var(b.name.clone())),
                );
                let (members, defaults) = self.class_members(decl, head);
                Body::Class(supers, fundeps, members, defaults)
            }
            _ => Body::None,
        }
    }
}
