#[cfg_attr(target_arch = "wasm32", allow(clippy::unused_self))]
impl Relocation<'_> {
    fn ir_types(&self, types: &mut [flick_ir::TypeRef]) {
        #[cfg(not(target_arch = "wasm32"))]
        for ty in types {
            self.ir_type(ty);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = types;
    }

    fn opt_ir_type(&self, ty: &mut Option<flick_ir::TypeRef>) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(ty) = ty {
            self.ir_type(ty);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = ty;
    }

    fn binder_types(&self, binders: &mut [(String, flick_ir::TypeRef)]) {
        #[cfg(not(target_arch = "wasm32"))]
        for (_, ty) in binders {
            self.ir_type(ty);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = binders;
    }

    fn constraints(&self, constraints: &mut [flick_ir::Constraint]) {
        #[cfg(not(target_arch = "wasm32"))]
        for c in constraints {
            if c.module == self.child {
                self.parent.clone_into(&mut c.module);
                c.class = format!("{}${}", self.local, c.class);
            }
            self.ir_types(&mut c.args);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = constraints;
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn ir_type(&self, ty: &mut flick_ir::TypeRef) {
        if let Some(updated) = self.rewrite_ir_type(ty) {
            *ty = updated;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn rewrite_ir_types(&self, list: &[flick_ir::TypeRef]) -> Option<Vec<flick_ir::TypeRef>> {
        let mut changed = false;
        let out: Vec<_> = list
            .iter()
            .map(|t| match self.rewrite_ir_type(t) {
                Some(u) => {
                    changed = true;
                    u
                }
                None => t.clone(),
            })
            .collect();
        changed.then_some(out)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn rewrite_ir_type(&self, ty: &flick_ir::IrType) -> Option<flick_ir::TypeRef> {
        use flick_ir::{IrType as T, TypeRef};
        match ty {
            T::Named { module, name, args } => {
                let mut m = module.clone();
                let mut n = name.clone();
                let moved = self.name(&mut m, &mut n);
                let new_args = self.rewrite_ir_types(args);
                (moved || new_args.is_some()).then(|| {
                    TypeRef::new(T::Named {
                        module: m,
                        name: n,
                        args: new_args.unwrap_or_else(|| args.clone()),
                    })
                })
            }
            T::Var(_) => None,
            T::App { head, args } => {
                let new_head = self.rewrite_ir_type(head);
                let new_args = self.rewrite_ir_types(args);
                (new_head.is_some() || new_args.is_some()).then(|| {
                    TypeRef::new(T::App {
                        head: new_head.unwrap_or_else(|| head.clone()),
                        args: new_args.unwrap_or_else(|| args.clone()),
                    })
                })
            }
            T::Fun {
                param,
                stage,
                result,
            } => {
                let new_p = self.rewrite_ir_type(param);
                let new_r = self.rewrite_ir_type(result);
                (new_p.is_some() || new_r.is_some()).then(|| {
                    TypeRef::new(T::Fun {
                        param: new_p.unwrap_or_else(|| param.clone()),
                        stage: *stage,
                        result: new_r.unwrap_or_else(|| result.clone()),
                    })
                })
            }
            T::Tuple(items) => self
                .rewrite_ir_types(items)
                .map(|xs| TypeRef::new(T::Tuple(xs))),
            T::Record { fields, tail } => {
                let mut changed = false;
                let new_fields: Vec<_> = fields
                    .iter()
                    .map(|(l, t)| match self.rewrite_ir_type(t) {
                        Some(u) => {
                            changed = true;
                            (l.clone(), u)
                        }
                        None => (l.clone(), t.clone()),
                    })
                    .collect();
                changed.then(|| {
                    TypeRef::new(T::Record {
                        fields: new_fields,
                        tail: tail.clone(),
                    })
                })
            }
            T::Forall {
                vars,
                constraints,
                body,
            } => {
                let mut new_cs = constraints.clone();
                self.constraints(&mut new_cs);
                let cs_changed = new_cs != *constraints;
                let new_body = self.rewrite_ir_type(body);
                (cs_changed || new_body.is_some()).then(|| {
                    TypeRef::new(T::Forall {
                        vars: vars.clone(),
                        constraints: new_cs,
                        body: new_body.unwrap_or_else(|| body.clone()),
                    })
                })
            }
            T::Exists {
                binder,
                contract,
                body,
            } => {
                let new_c = self.rewrite_ir_type(contract);
                let new_b = self.rewrite_ir_type(body);
                (new_c.is_some() || new_b.is_some()).then(|| {
                    TypeRef::new(T::Exists {
                        binder: binder.clone(),
                        contract: new_c.unwrap_or_else(|| contract.clone()),
                        body: new_b.unwrap_or_else(|| body.clone()),
                    })
                })
            }
        }
    }
}

/// Relocates any remaining references to inline child modules in importers' IR
/// (types, constraints, constructors) so every module in a compilation agrees
/// with the merged parent module's `Child$Name` IR identities.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn relocate_inline_importers(compiled: &mut [super::Compiled]) {
    let module_names: HashSet<String> = compiled.iter().map(|m| m.name.clone()).collect();
    let mut relocations: Vec<(String, String, String)> = Vec::new();
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    for module in compiled.iter() {
        let mut record = |name: &str| {
            let stripped = name.strip_prefix("$default_").unwrap_or(name);
            let Some((prefix, _)) = stripped.rsplit_once('$') else {
                return;
            };
            if prefix.is_empty()
                || !prefix
                    .split('$')
                    .all(|seg| seg.starts_with(|c: char| c.is_ascii_uppercase()))
            {
                return;
            }
            let child = format!("{}.{}", module.name, prefix.replace('$', "."));
            if module_names.contains(&child) {
                return;
            }
            let key = (child, module.name.clone(), prefix.to_owned());
            if seen.insert(key.clone()) {
                relocations.push(key);
            }
        };
        for decl in &module.ir.types {
            record(&decl.name);
        }
        for export in &module.declarations.exports {
            if export.entity.module == module.name {
                record(&export.entity.name);
            }
        }
    }
    if relocations.is_empty() {
        return;
    }
    relocations.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.cmp(b)));
    for (child, parent, local) in &relocations {
        for module in compiled.iter_mut() {
            if &module.name != parent {
                relocate_ir(&mut module.ir, child, parent, local);
            }
        }
    }
}
