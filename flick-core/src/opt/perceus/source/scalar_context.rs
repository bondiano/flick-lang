//! Checked compiler declaration facts for overloaded scalar host bindings.
//!
//! This is not validation of arbitrarily forged IR/interface pairs. Callers
//! supply the checked `Compiled.declarations` views, including private/open
//! representations, together with the matching builtin declarations. Missing,
//! incomplete, unresolved and conflicting facts fail closed before lowering.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::iface::{Export, Interface, Namespace, support};
use crate::ir::memory::{Intrinsic, Name};
use crate::types::lang::Lang;
use crate::types::{Atom, Binder, Elem, Form, Kind, Type, TypeDef};

pub(super) struct ScalarContext<'a> {
    values: BTreeMap<Name, &'a Type>,
    definitions: BTreeMap<Name, &'a TypeDef>,
    recursive: BTreeSet<Name>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScalarDomain {
    Number,
    BigIntConversion,
    FloatSelection,
    IntSelection,
}

#[derive(Debug, PartialEq, Eq)]
enum Domain {
    Int,
    BigInt,
    Float,
    Bool,
    String,
    Unit,
    Other,
    Unknown,
}

impl<'a> ScalarContext<'a> {
    pub(super) fn new(interfaces: impl IntoIterator<Item = &'a Interface>) -> Result<Self, String> {
        let mut context = Self {
            values: BTreeMap::new(),
            definitions: BTreeMap::new(),
            recursive: BTreeSet::new(),
        };
        let roots: Vec<_> = interfaces.into_iter().collect();
        let mut modules = BTreeSet::new();
        let mut authoritative = BTreeSet::new();
        for interface in &roots {
            if !modules.insert(interface.module.as_str()) {
                return Err(format!(
                    "duplicate declaration context {}",
                    interface.module
                ));
            }
            for export in &interface.exports {
                if export.entity.module == interface.module {
                    authoritative.insert(export.entity.clone());
                }
            }
        }
        // Only an explicit checked root owns its canonical declarations. A
        // public/ascribed child projection is not a second representation of
        // that authority (notably an opaque public view versus an opened alias).
        // Facts without a supplied owning root still come from nested/support
        // views and must agree exactly, independently of traversal order.
        for interface in &roots {
            for export in &interface.exports {
                if export.entity.module == interface.module {
                    context.export(export)?;
                }
            }
        }
        let mut pending = roots;
        while let Some(interface) = pending.pop() {
            if interface.incomplete {
                return Err(format!(
                    "incomplete declaration context {}",
                    interface.module
                ));
            }
            support::validate(interface)?;
            pending.extend(interface.modules.values());
            let mut entities = BTreeSet::new();
            for export in &interface.exports {
                if !entities.insert(&export.entity) {
                    return Err(format!("duplicate declaration fact {:?}", export.entity));
                }
                if !authoritative.contains(&export.entity) {
                    context.export(export)?;
                }
            }
            for support in &interface.support_types {
                if !authoritative.contains(&support.entity) {
                    context.definition(
                        &Name::new(&support.entity.module, &support.entity.name),
                        &support.definition,
                    )?;
                }
            }
        }
        context.recursive = recursive_aliases(&context.definitions);
        Ok(context)
    }

    fn export(&mut self, export: &'a Export) -> Result<(), String> {
        let key = Name::new(&export.entity.module, &export.entity.name);
        if export.entity.namespace == Namespace::Value {
            let scheme = export
                .scheme
                .as_ref()
                .ok_or_else(|| format!("missing declaration scheme {key:?}"))?;
            validate_type(scheme).map_err(|message| format!("{key:?} scheme: {message}"))?;
            if let Some(previous) = self.values.insert(key.clone(), scheme)
                && previous != scheme
            {
                return Err(format!("conflicting declaration schemes {key:?}"));
            }
        }
        if export.entity.namespace == Namespace::Type
            && let Some(definition) = &export.definition
        {
            self.definition(&key, definition)?;
        }
        Ok(())
    }

    fn definition(&mut self, key: &Name, definition: &'a TypeDef) -> Result<(), String> {
        validate_kind(&definition.kind)
            .map_err(|message| format!("{key:?} definition kind: {message}"))?;
        if let Form::Alias { params, body, .. } = &definition.form {
            validate_type(body).map_err(|message| format!("{key:?} alias body: {message}"))?;
            if params.iter().collect::<BTreeSet<_>>().len() != params.len() {
                return Err(format!("duplicate alias parameters {key:?}"));
            }
        }
        if let Some(previous) = self.definitions.insert(key.clone(), definition)
            && previous != definition
        {
            return Err(format!("conflicting type declaration facts {key:?}"));
        }
        Ok(())
    }

    /// App.fail's result copy is unreachable; only its consumed input needs a
    /// domain proof. Observe the checked scheme, never the generic Data shape.
    pub(super) fn app_fail(&self, key: &Name) -> Result<(), String> {
        let scheme = *self
            .values
            .get(key)
            .ok_or_else(|| format!("missing checked declaration scheme {key:?}"))?;
        let mut observer = Observer {
            context: self,
            frames: vec![vec![]],
        };
        let head = observer.scheme(scheme)?;
        let Type::Fun {
            param,
            effects: Some(_),
            ..
        } = head.ty
        else {
            return Err("checked App.fail domain is not an effect arrow".into());
        };
        let input = observer.primitive_domain(Descriptor {
            ty: param,
            environment: head.environment,
        })?;
        if input == Domain::String {
            Ok(())
        } else {
            Err(format!(
                "unsupported checked App.fail input domain {input:?}"
            ))
        }
    }

    /// Data shapes do not distinguish Int from Float or a nominal type.
    /// Prove both pure input arrows and the exact canonical List Int output.
    pub(super) fn list_range(&self, key: &Name) -> Result<(), String> {
        let scheme = *self
            .values
            .get(key)
            .ok_or_else(|| format!("missing checked declaration scheme {key:?}"))?;
        let mut observer = Observer {
            context: self,
            frames: vec![vec![]],
        };
        let mut cursor = observer.scheme(scheme)?;
        for input in 0..2 {
            let (head, arguments) = observer.head(cursor)?;
            if !arguments.is_empty() {
                return Err("unknown applied range signature".into());
            }
            let Type::Fun {
                param,
                effects: None,
                result,
            } = head.ty
            else {
                return Err("checked range requires two pure arrows".into());
            };
            let domain = observer.primitive_domain(Descriptor {
                ty: param,
                environment: head.environment,
            })?;
            if domain != Domain::Int {
                return Err(format!(
                    "unsupported checked range input {input} domain {domain:?}"
                ));
            }
            cursor = Descriptor {
                ty: result,
                environment: head.environment,
            };
        }
        let (head, mut arguments) = observer.head(cursor)?;
        let Type::Con { module, name } = head.ty else {
            return Err("checked range output is not List Int".into());
        };
        if !head.ty.is_lang(Lang::List)
            || !self.definitions.contains_key(&Name::new(module, name))
            || arguments.len() != 1
        {
            return Err("checked range output is not List Int".into());
        }
        let item = arguments.pop_front().expect("one checked List argument");
        if observer.primitive_domain(item)? != Domain::Int {
            return Err("checked range output element is not Int".into());
        }
        Ok(())
    }

    pub(super) fn adapter(&self, key: &Name, kind: Intrinsic) -> Result<ScalarDomain, String> {
        let scheme = *self
            .values
            .get(key)
            .ok_or_else(|| format!("missing checked declaration scheme {key:?}"))?;
        let mut observer = Observer {
            context: self,
            frames: vec![vec![]],
        };
        let head = observer.scheme(scheme)?;
        let Type::Fun {
            param,
            effects: None,
            result,
        } = head.ty
        else {
            return Err("checked scalar domain is not a pure arrow".into());
        };
        let input = Descriptor {
            ty: param,
            environment: head.environment,
        };
        let output = Descriptor {
            ty: result,
            environment: head.environment,
        };
        if kind == Intrinsic::ToBigInt {
            return observer.bigint_conversion(input, output);
        }
        let input = observer.domain(input)?;
        match kind {
            Intrinsic::NumberToFloat => {
                let output = observer.domain(output)?;
                if matches!(input, Domain::Int | Domain::BigInt | Domain::Float)
                    && output == Domain::Float
                {
                    Ok(ScalarDomain::Number)
                } else {
                    Err(format!(
                        "unsupported checked Number domain {input:?} -> {output:?}"
                    ))
                }
            }
            Intrinsic::FloatMin | Intrinsic::FloatMax => {
                let (next, arguments) = observer.head(output)?;
                if !arguments.is_empty() {
                    return Err("unknown applied Number selection signature".into());
                }
                let Type::Fun {
                    param,
                    effects: None,
                    result,
                } = next.ty
                else {
                    return Err("checked Number selection requires two pure arrows".into());
                };
                let middle = observer.domain(Descriptor {
                    ty: param,
                    environment: next.environment,
                })?;
                let output = observer.domain(Descriptor {
                    ty: result,
                    environment: next.environment,
                })?;
                if input == Domain::Float && middle == Domain::Float && output == Domain::Float {
                    Ok(ScalarDomain::FloatSelection)
                } else if input == Domain::Int && middle == Domain::Int && output == Domain::Int {
                    Ok(ScalarDomain::IntSelection)
                } else {
                    Err(format!(
                        "unsupported checked Number selection domain {input:?} -> {middle:?} -> {output:?}"
                    ))
                }
            }
            _ => Err("not an evidence-based scalar adapter".into()),
        }
    }
}

include!("scalar_context/observation.rs");
