// Bind production inference roots through their complete lexical occurrence.

impl Checker<'_> {
    pub(in crate::types) fn bind_occurrence(
        &mut self,
        occurrence: &MetaOccurrence,
        t: &Type,
    ) -> Unified {
        self.inference_trial(|this| this.bind_occurrence_inner(occurrence, t))
    }

    fn bind_occurrence_inner(&mut self, occurrence: &MetaOccurrence, t: &Type) -> Unified {
        let variable = Type::Meta(occurrence.clone());
        let m = occurrence
            .inference_type_root()
            .ok_or_else(|| Mismatch::Types(variable.clone(), t.clone()))?;
        let mappings = self
            .store
            .lexical_mappings(occurrence)
            .map_err(|()| Mismatch::Types(variable.clone(), t.clone()))?;
        let t = self.zonk(t);
        if let Type::Meta(other) = &t
            && other.inference_type_root() == Some(m)
        {
            let other_view = self
                .store
                .lexical_mappings(other)
                .map_err(|()| Mismatch::Types(variable.clone(), t.clone()))?;
            if occurrence == other {
                return Ok(());
            }
            let mut context = self.store.meta_contexts[m as usize].clone();
            context.lexical.permitted.retain(|id| {
                let left = mappings
                    .iter()
                    .find(|(from, _)| from.id == *id)
                    .map(|(_, target)| target);
                let right = other_view
                    .iter()
                    .find(|(from, _)| from.id == *id)
                    .map(|(_, target)| target);
                left.is_some() && left == right
            });
            if let Some(kind) = &context.kind {
                let projected = module_contract::lexical::transport_kind(kind, &mappings);
                let allowed = mappings
                    .iter()
                    .filter(|(source, _)| context.lexical.permitted.contains(&source.id))
                    .map(|(_, target)| target.clone())
                    .collect::<Vec<_>>();
                self.kinds
                    .restrict_lexical(&projected, &allowed, &[])
                    .map_err(|()| Mismatch::Types(variable.clone(), t.clone()))?;
            }
            self.store.write_context(m, context);
            return Ok(());
        }
        let mut occurs = false;
        module_contract::lexical::visit_type_metas(&t, &mut |meta, kind| {
            occurs |= !kind && meta.inference_type_root() == Some(m);
        });
        if occurs {
            return Err(Mismatch::Occurs(t));
        }
        if t.contains(&mut |x| matches!(x, Type::Forall { .. })) {
            return Err(Mismatch::Polymorphic(t));
        }
        if self.store.is_numeric(m) && !self.store.numeric_fits(m, &t) {
            return Err(Mismatch::Types(variable, t));
        }
        if self.store.untouchable(m) {
            return Err(Mismatch::Untouchable(t));
        }
        let allowed = mappings
            .iter()
            .map(|(_, target)| target.clone())
            .collect::<Vec<_>>();
        let solution = self
            .lexical_solution(&t, &allowed, &[], false, &mut Vec::new())
            .map_err(|()| Mismatch::Types(Type::Meta(occurrence.clone()), t.clone()))?;
        let inverse = mappings
            .iter()
            .map(|(source, target)| (target.clone(), source.clone()))
            .collect::<Vec<_>>();
        let solution = module_contract::lexical::transport_type(&solution, &inverse);
        let level = self
            .store
            .meta_level(m)
            .ok_or_else(|| Mismatch::Types(Type::Meta(occurrence.clone()), t.clone()))?;
        self.store
            .adjust(&solution, level)
            .map_err(Mismatch::Escape)?;
        self.store.set(m, solution);
        Ok(())
    }
}
