//! Checking declared instances against their superclasses and recording their
//! codegen metadata.

use super::declare::Declared;
use super::{Dict, Outcome, split};
use crate::diagnostics::{Diagnostic, codes};
use crate::types::Checker;
use crate::types::env::Form;
use crate::types::ty::Type;
use crate::types::typing::{Constructor, InstanceInfo, passes_dictionary, range};

impl Checker<'_> {
    /// After every instance is known: superclasses hold, derived fields have
    /// instances, and methods match the class.
    pub fn check_instances(&mut self, declared: &[Declared]) {
        for d in declared {
            self.classes.steps = 0;
            let name = d.instance.name.clone();
            let mut supers = Vec::new();
            let declared = self.supers_of(&d.instance.head);
            for (i, s) in declared.iter().enumerate() {
                let shown = self.show(s);
                match self.entails(s, &d.instance.context) {
                    Outcome::Solved(dict) => supers.push((super::super_slot(&declared, i), dict)),
                    Outcome::Budget(_) => self.error(
                        codes::C009,
                        d.span,
                        format!("resolving superclass `{shown}` of `{name}` exceeds the budget"),
                    ),
                    _ => self.diagnostics.push(
                        Diagnostic::error(
                            codes::C001,
                            format!("`{name}` needs an instance `{shown}` of its superclass"),
                            d.span,
                        )
                        .with_fix(|| (format!("declare `instance {shown}`"), Vec::new())),
                    ),
                }
            }
            let (derived, defaults) = if let Some(fields) = &d.fields {
                (Some(self.derived_fields(d, fields)), Vec::new())
            } else if d.generic.is_some() || d.enumerates.is_some() {
                (None, Vec::new())
            } else {
                (None, self.instance_methods(d.decl, &d.instance))
            };
            self.record_instance(d, supers, derived, defaults);
        }
    }

    /// What code generation needs to build the dictionary of an instance.
    fn record_instance(
        &mut self,
        d: &Declared,
        supers: Vec<(String, Dict)>,
        fields: Option<Vec<Dict>>,
        defaults: Vec<(String, usize, Vec<Dict>)>,
    ) {
        let Some(((cm, cn), _)) = split(&d.instance.head) else {
            return;
        };
        let class = (cm.to_owned(), cn.to_owned());
        let context = d
            .instance
            .context
            .iter()
            .filter(|c| passes_dictionary(c))
            .map(|c| self.show_exact(&self.canon(c)))
            .collect();
        let derived = fields.map(|fields| {
            let target = d.instance.head.spine().1.first().map(|t| (*t).clone());
            let constructors = target.and_then(|t| self.derived_constructors(&t));
            (fields, constructors)
        });
        let info = InstanceInfo {
            module: d.instance.module.clone(),
            name: d.instance.name.clone(),
            class,
            span: range(d.span),
            context,
            supers,
            derived,
            defaults,
            generic: d.generic.clone(),
            enumerates: d.enumerates.clone(),
        };
        self.typing.instances.push(info);
    }

    /// The constructors of the type a derived instance is for; `None` when
    /// it has no variants.
    fn derived_constructors(&self, target: &Type) -> Option<Vec<Constructor>> {
        let Type::Con { module, name } = target.spine().0 else {
            return None;
        };
        let names = match self.env.type_def(module, name)?.typing() {
            Form::Adt { constructors, .. } => constructors.clone(),
            Form::Error => vec![name.clone()],
            _ => return None,
        };
        Some(
            names
                .into_iter()
                .map(|c| Constructor {
                    module: module.clone(),
                    ty: name.clone(),
                    arity: self.constructor_fields(&c).len(),
                    rep: self.env.constructor_rep(module, name, &c),
                    name: c,
                    dicts: 0,
                    fields: Vec::new(),
                    type_args: Vec::new(),
                })
                .collect(),
        )
    }
}
