//! Lowering checker [`Type`]s into interned [`flick_ir::IrType`]s.

use std::collections::HashMap;

use flick_ir::{Constraint, IrType, Stage, TypeInterner, TypeRef};

use super::Checker;
use super::env::Form;
use super::gadt::scheme::is_hidden_captures;
use super::kinds::Kind;
use super::lang::{self, Lang};
use super::options::ModuleLookup;
use super::ty::{Binder, Type};
use super::typing::passes_dictionary;
use crate::iface::Namespace;
use crate::resolve::Interface;

/// Whether a type parameter or argument of kind `kind` affects value layout.
///
/// Effect, service, error, capture, region, symbol, and constraint parameters
/// carry no layout information and are omitted from IR types and declarations.
pub(crate) fn is_layout_kind(kind: &Kind) -> bool {
    match kind {
        Kind::Type | Kind::Module | Kind::Named { .. } | Kind::Meta(_) => true,
        Kind::Row(inner) => is_layout_kind(inner),
        Kind::Arrow(_, result) => is_layout_kind(result),
        Kind::ServiceRow
        | Kind::ErrorRow
        | Kind::CaptureRow
        | Kind::Symbol
        | Kind::Region
        | Kind::Constraint
        | Kind::Service => false,
    }
}

/// Whether `ty` is a non-layout row/symbol argument (effect, service, error,
/// capture, or region) that should be omitted when lowering higher-kinded
/// applications, as opposed to an unresolved `Meta` or `Error` which must erase
/// the enclosing IR type.
fn is_non_layout_type(ty: &Type) -> bool {
    match ty {
        Type::Symbol(_) | Type::Union(..) | Type::Without { .. } => true,
        Type::Captured { ty, .. } | Type::Bounded { ty, .. } => is_non_layout_type(ty),
        _ => false,
    }
}

/// Parameter kinds of a type constructor's kind `k1 -> k2 -> ... -> res`.
pub(super) fn kind_params(mut kind: &Kind) -> Vec<&Kind> {
    let mut out = Vec::new();
    while let Kind::Arrow(param, result) = kind {
        out.push(param.as_ref());
        kind = result;
    }
    out
}

/// Collects all outer `Forall` binders (skipping `Captured` / `Bounded`) and
/// dictionary-passing constraints, returning `(binders, constraints, inner)`.
pub(super) fn peel_scheme(ty: &Type) -> (Vec<Binder>, Vec<Type>, &Type) {
    let mut vars = Vec::new();
    let mut constraints = Vec::new();
    let mut cur = ty;
    loop {
        match cur {
            Type::Captured { ty, .. } | Type::Bounded { ty, .. } => cur = ty,
            Type::Forall {
                vars: vs,
                constraints: cs,
                body,
            } => {
                vars.extend(vs.iter().cloned());
                constraints.extend(cs.iter().cloned());
                cur = body;
            }
            _ => break,
        }
    }
    (vars, constraints, cur)
}

/// Lowers checker types into interned [`TypeRef`]s using the module's solved
/// environment and unification store.
pub(super) struct IrTypeLower<'a, 'c> {
    checker: &'a Checker<'c>,
    ir_lookup: Option<ModuleLookup<'a>>,
    unsealed_cache: HashMap<String, Option<Interface>>,
    pub(super) interner: TypeInterner,
}

impl<'a, 'c> IrTypeLower<'a, 'c> {
    #[cfg(test)]
    pub(super) fn new(checker: &'a Checker<'c>) -> Self {
        Self::with_lookup(checker, None)
    }

    pub(super) fn with_lookup(
        checker: &'a Checker<'c>,
        ir_lookup: Option<ModuleLookup<'a>>,
    ) -> Self {
        IrTypeLower {
            checker,
            ir_lookup,
            unsealed_cache: HashMap::new(),
            interner: TypeInterner::new(),
        }
    }

    /// Lowers `ty` after zonking and resolving aliases; returns `None` when
    /// `ty` contains an unresolved `Meta` or `Error` (erased path).
    pub(super) fn lower(&mut self, ty: &Type) -> Option<TypeRef> {
        let zonked = self.checker.zonk(ty);
        self.convert(&zonked, 0)
    }

    /// Lowers a scheme `(type_params, dict_constraints, body_ty)` by peeling
    /// outer `Forall`s into layout-relevant `type_params` and dictionary
    /// `constraints`.
    pub(super) fn lower_scheme(
        &mut self,
        scheme: &Type,
    ) -> Option<(Vec<String>, Vec<Constraint>, TypeRef)> {
        let zonked = self.checker.zonk(scheme);
        let (vars, constraints, body) = peel_scheme(&zonked);
        let type_params: Vec<String> = vars
            .iter()
            .filter(|v| !is_hidden_captures(v) && is_layout_kind(&v.kind))
            .map(|v| v.name.clone())
            .collect();
        let dict_constraints = self.lower_constraints(&constraints);
        let body_ty = self.convert(body, 0)?;
        Some((type_params, dict_constraints, body_ty))
    }

    pub(super) fn lower_constraints(&mut self, constraints: &[Type]) -> Vec<Constraint> {
        constraints
            .iter()
            .filter(|c| passes_dictionary(c))
            .filter_map(|c| self.lower_constraint(c))
            .collect()
    }

    fn lower_constraint(&mut self, constraint: &Type) -> Option<Constraint> {
        let (head, args) = constraint.spine();
        let Type::Con { module, name } = head else {
            return None;
        };
        let ir_args = self.filter_con_args(module, name, &args, 0)?;
        Some(Constraint {
            module: module.clone(),
            class: name.clone(),
            args: ir_args,
        })
    }

    fn filter_con_args(
        &mut self,
        module: &str,
        name: &str,
        args: &[&Type],
        depth: u8,
    ) -> Option<Vec<TypeRef>> {
        let kinds = self
            .checker
            .env
            .type_def(module, name)
            .map(|d| kind_params(&d.kind));
        let mut out = Vec::with_capacity(args.len());
        for (i, arg) in args.iter().enumerate() {
            let keep = kinds
                .as_ref()
                .and_then(|ks| ks.get(i))
                .is_none_or(|k| is_layout_kind(k));
            if keep {
                out.push(self.convert(arg, depth + 1)?);
            }
        }
        Some(out)
    }

    fn convert(&mut self, ty: &Type, depth: u8) -> Option<TypeRef> {
        if depth > 64 {
            return None;
        }
        match ty {
            Type::Captured { ty, .. } | Type::Bounded { ty, .. } => self.convert(ty, depth),
            Type::Error
            | Type::Meta(_)
            | Type::Symbol(_)
            | Type::Union(..)
            | Type::Without { .. } => None,
            Type::Var(name) => Some(self.interner.var(name)),
            Type::Skolem(id) => {
                let name = self.checker.store.skolem_name(*id).to_owned();
                Some(self.interner.var(name))
            }
            Type::Con { module, name } => self.convert_named(module, name, &[], depth),
            Type::App { .. } => {
                let (head, args) = ty.spine();
                match head {
                    Type::Con { module, name } => self.convert_named(module, name, &args, depth),
                    other => {
                        let ir_head = self.convert(other, depth + 1)?;
                        let mut ir_args = Vec::with_capacity(args.len());
                        for a in args {
                            if is_non_layout_type(a) {
                                continue;
                            }
                            ir_args.push(self.convert(a, depth + 1)?);
                        }
                        if ir_args.is_empty() {
                            Some(ir_head)
                        } else {
                            Some(self.interner.intern(IrType::App {
                                head: ir_head,
                                args: ir_args,
                            }))
                        }
                    }
                }
            }
            Type::Fun {
                param,
                effects,
                result,
            } => {
                let ir_param = self.convert(param, depth + 1)?;
                let ir_result = self.convert(result, depth + 1)?;
                let stage = match effects {
                    None => Stage::Pure,
                    Some(eff) if eff.sync => Stage::Sync,
                    Some(_) => Stage::Runtime,
                };
                Some(self.interner.fun(ir_param, stage, ir_result))
            }
            Type::Tuple(items) => {
                let ir_items: Option<Vec<TypeRef>> =
                    items.iter().map(|t| self.convert(t, depth + 1)).collect();
                Some(self.interner.tuple(ir_items?))
            }
            Type::Record(row) | Type::Row(row) => self.convert_record(row, depth),
            Type::Forall {
                vars,
                constraints,
                body,
            } => {
                let ir_vars: Vec<String> = vars
                    .iter()
                    .filter(|v| !is_hidden_captures(v) && is_layout_kind(&v.kind))
                    .map(|v| v.name.clone())
                    .collect();
                let ir_constraints = self.lower_constraints(constraints);
                let ir_body = self.convert(body, depth + 1)?;
                if ir_vars.is_empty() && ir_constraints.is_empty() {
                    Some(ir_body)
                } else {
                    Some(self.interner.intern(IrType::Forall {
                        vars: ir_vars,
                        constraints: ir_constraints,
                        body: ir_body,
                    }))
                }
            }
            Type::Exists {
                binder,
                contract,
                body,
                ..
            } => {
                let ir_contract = self.convert(contract, depth + 1)?;
                let ir_body = self.convert(body, depth + 1)?;
                Some(self.interner.intern(IrType::Exists {
                    binder: binder.clone(),
                    contract: ir_contract,
                    body: ir_body,
                }))
            }
        }
    }

    fn convert_record(&mut self, row: &super::ty::Row, depth: u8) -> Option<TypeRef> {
        let norm = self.checker.store.normalize(&Type::Row(row.clone()));
        let mut fields = Vec::with_capacity(norm.entries.len());
        for (elem, fty) in &norm.entries {
            let Some(label) = elem.name() else {
                continue;
            };
            fields.push((label.to_owned(), self.convert(fty, depth + 1)?));
        }
        let mut tail = None;
        for t in norm.tails {
            match t {
                Type::Var(v) if tail.is_none() => tail = Some(v),
                Type::Skolem(id) if tail.is_none() => {
                    tail = Some(self.checker.store.skolem_name(id).to_owned());
                }
                Type::Meta(_) | Type::Error => return None,
                _ => {}
            }
        }
        Some(self.interner.record(fields, tail))
    }

    fn convert_named(
        &mut self,
        module: &str,
        name: &str,
        args: &[&Type],
        depth: u8,
    ) -> Option<TypeRef> {
        if lang::is(Lang::Errors, module, name)
            && let [row] = args
        {
            let norm = self.checker.store.normalize(row);
            if norm.tails.is_empty() {
                if norm.entries.is_empty() {
                    let never = lang::item(Lang::Never);
                    return Some(self.interner.named(&never.module, &never.name, Vec::new()));
                }
                if norm.entries.len() == 1 {
                    return self.convert(&norm.entries[0].1, depth + 1);
                }
            }
        }
        let alias = self
            .checker
            .env
            .type_def(module, name)
            .and_then(|def| match &def.form {
                Form::Alias {
                    params,
                    body,
                    opaque: false,
                } if args.len() >= params.len() => Some((params.clone(), body.clone())),
                _ => None,
            })
            .or_else(|| self.unsealed_transparent_alias(module, name, args.len()));
        if let Some((params, body)) = alias {
            let subst: Vec<(String, Type)> = params
                .iter()
                .cloned()
                .zip(args.iter().map(|a| (*a).clone()))
                .collect();
            let expanded = body.substitute(&subst);
            let expanded = Type::apps(expanded, args[params.len()..].iter().map(|a| (*a).clone()));
            return self.convert(&expanded, depth + 1);
        }
        let ir_args = self.filter_con_args(module, name, args, depth)?;
        Some(self.interner.named(module, name, ir_args))
    }

    fn unsealed_transparent_alias(
        &mut self,
        module: &str,
        name: &str,
        arity: usize,
    ) -> Option<(Vec<String>, Type)> {
        let lookup = self.ir_lookup?;
        let iface = self
            .unsealed_cache
            .entry(module.to_owned())
            .or_insert_with(|| lookup(module))
            .as_ref()?;
        let def = iface
            .exports
            .iter()
            .find(|e| {
                e.entity.namespace == Namespace::Type
                    && e.entity.module == module
                    && e.entity.name == name
            })
            .and_then(|e| e.definition.as_ref())
            .or_else(|| {
                iface
                    .support_types
                    .iter()
                    .find(|s| {
                        s.entity.namespace == Namespace::Type
                            && s.entity.module == module
                            && s.entity.name == name
                    })
                    .map(|s| &s.definition)
            })?;
        match &def.form {
            Form::Alias {
                params,
                body,
                opaque: false,
            } if arity >= params.len() => Some((params.clone(), body.clone())),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use flick_ir::IrType;

    use super::{IrTypeLower, Type};
    use crate::resolve::{self, Module};
    use crate::source::SourceMap;
    use crate::syntax;
    use crate::types::{
        ForeignBackendSpec, Limits, Options, StaticContext, Thresholds, Watched, builtin_env,
        checker_in,
    };

    #[test]
    fn higher_kinded_app_erases_unresolved_meta_and_error_but_omits_non_layout_args() {
        let text = "module Test exposing (..)\n";
        let mut sources = SourceMap::default();
        let file = sources.add("Test.fl", text);
        let parse = syntax::parse(text, file);
        let syntax = resolve::module_syntax(&parse.tree, text, file);
        let module = Module {
            name: "Test",
            file,
            text,
            tree: &parse.tree,
            syntax: &syntax,
            generated: resolve::NO_GENERATED,
        };
        let local = |n: &str| n == "Test";
        let options = Options {
            local: &local,
            limits: Limits::default(),
            no_let_assert: false,
            thresholds: Thresholds::default(),
            watched: Watched::none(),
            stages_by_class: false,
            foreign_backend: ForeignBackendSpec::default_js(),
            validate_foreign_metadata: true,
            native_overrides: None,
        };
        let mut checker = checker_in(
            &module,
            &|_| None,
            builtin_env(false),
            options,
            StaticContext::default(),
        );
        let meta = checker.store.fresh();
        let int_ty = Type::Con {
            module: "Prelude".into(),
            name: "Int".into(),
        };
        let f_var = Type::Var("f".into());

        let mut lower = IrTypeLower::new(&checker);
        // Unresolved meta or error argument must erase the entire application (`None`).
        assert_eq!(lower.lower(&Type::app(f_var.clone(), meta.clone())), None);
        assert_eq!(
            lower.lower(&Type::apps(f_var.clone(), [meta, int_ty.clone()])),
            None
        );
        assert_eq!(lower.lower(&Type::app(f_var.clone(), Type::Error)), None);

        // Non-layout arguments (e.g. `Symbol`) are omitted without erasing the head.
        let sym = Type::Symbol("lbl".into());
        assert_eq!(
            lower
                .lower(&Type::app(f_var.clone(), sym.clone()))
                .as_deref(),
            Some(&IrType::Var("f".into()))
        );
        assert_eq!(
            lower
                .lower(&Type::apps(f_var, [sym, int_ty]))
                .map(|t| t.to_string())
                .as_deref(),
            Some("f Prelude.Int")
        );
    }
}
