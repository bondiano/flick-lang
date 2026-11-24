//! Checking direct foreign declarations and `@native` overrides against the
//! active backend's versioned value-ABI table (`FLICK-F013`).

use std::collections::{BTreeSet, HashSet};

use flick_ir::ForeignMode;

use super::Checker;
use super::spec::ForeignBackendSpec;
use super::value_abi_spec::{AbiDirection, ValueAbiSpec};
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use crate::types::env::{Abi, EnumLit, Form};
use crate::types::lang::{self, Lang};
use crate::types::ty::{Stage, Type};
use crate::types::typing::ForeignInfo;

/// Built-in value and data lang items that require an explicit entry in a
/// backend's value-ABI table rather than falling back to `opaque_type`.
const CORE_VALUE_LANG_ITEMS: &[Lang] = &[
    Lang::Unit,
    Lang::Never,
    Lang::Bool,
    Lang::Int,
    Lang::Float,
    Lang::BigInt,
    Lang::Int8,
    Lang::Int16,
    Lang::Int64,
    Lang::UInt8,
    Lang::UInt16,
    Lang::UInt32,
    Lang::UInt64,
    Lang::Float32,
    Lang::String,
    Lang::Unknown,
    Lang::JsValue,
    Lang::List,
    Lang::Option,
    Lang::Result,
    // a host function pointer, never an opaque handle
    Lang::FuncRef,
];

/// Returns true for compiler/runtime-internal bindings (`from "flick:..."`)
/// that are lowered directly by backend runtime tables rather than direct host ABI.
pub(super) fn is_runtime_internal_foreign(decl: &Node, text: &str) -> bool {
    let Some(source) = decl.nodes().find(|n| n.kind == K::ForeignSource) else {
        return false;
    };
    if source.child_tokens().any(|t| t.kind == K::Ident) {
        return false;
    }
    source
        .child_tokens()
        .find(|t| t.kind == K::String)
        .is_some_and(|tok| literal::string_value(tok.text(text)).starts_with("flick:"))
}

/// Returns the `Lang` item corresponding to `module.name`, if any.
fn lang_of_con(module: &str, name: &str) -> Option<Lang> {
    let table = lang::table();
    Lang::ALL.iter().copied().find(|&l| {
        table
            .find(l)
            .is_some_and(|item| item.namespace == Namespace::Type && item.is(module, name))
    })
}

/// Defensive structural recursion cap so deeply nested or non-regular types
/// can never overflow the stack while checking value-ABI rules.
const MAX_ABI_DEPTH: usize = 32;

struct AbiCheck<'a, 'b> {
    checker: &'a Checker<'b>,
    table: &'a ValueAbiSpec,
    backend_id: &'a str,
    reported: BTreeSet<(String, &'static str, String)>,
    errors: Vec<String>,
    visited_types: HashSet<(String, String)>,
}

impl AbiCheck<'_, '_> {
    fn refuse(&mut self, ty: &Type, direction: AbiDirection, reason: &str) {
        let shown = self.checker.show(ty);
        let dir_label = match direction {
            AbiDirection::In => "parameter",
            AbiDirection::Out => "result",
            AbiDirection::Both => "value",
        };
        let key = (shown.clone(), dir_label, reason.to_owned());
        if !self.reported.insert(key) {
            return;
        }
        self.errors.push(format!(
            "type `{shown}` in foreign {dir_label} position is not supported by `{}` value ABI `{}@{}`: {reason}",
            self.backend_id, self.table.abi.name, self.table.abi.version
        ));
    }

    fn refuse_direction(&mut self, ty: &Type, direction: AbiDirection, allowed: AbiDirection) {
        let reason = format!(
            "allowed only in `{}` direction, not `{}`",
            allowed.as_str(),
            direction.as_str()
        );
        self.refuse(ty, direction, &reason);
    }

    fn check(&mut self, ty: &Type, direction: AbiDirection, depth: usize) {
        if depth >= MAX_ABI_DEPTH {
            return;
        }
        let (ty, entered) = self
            .checker
            .whnf_captured_stopping_at(ty, &self.visited_types);
        for id in &entered {
            self.visited_types.insert(id.clone());
        }
        self.check_reduced(&ty, direction, depth);
        for id in &entered {
            self.visited_types.remove(id);
        }
    }

    fn check_reduced(&mut self, ty: &Type, direction: AbiDirection, depth: usize) {
        match ty {
            Type::Error
            | Type::Var(_)
            | Type::Skolem(_)
            | Type::Row(_)
            | Type::Symbol(_)
            | Type::Union(_, _)
            | Type::Without { .. }
            | Type::Meta(_) => {}
            Type::Forall { body, .. } => self.check(body, direction, depth + 1),
            Type::Exists { body, .. } => {
                if self.table.admits_copied_data {
                    self.check(body, direction, depth + 1);
                } else {
                    let reason = self.table.fallback_reason.clone();
                    self.refuse(ty, direction, &reason);
                }
            }
            Type::Fun { .. } => self.check_fun(ty, direction, depth),
            Type::Tuple(items) => {
                if let Some(entry) = self.table.entry("tuple") {
                    if !entry.direction.allows(direction) {
                        self.refuse_direction(ty, direction, entry.direction);
                        return;
                    }
                    for item in items {
                        self.check(item, direction, depth + 1);
                    }
                } else {
                    let reason = self.table.refusal_reason("tuple").to_owned();
                    self.refuse(ty, direction, &reason);
                }
            }
            Type::Record(row) => {
                if self.table.admits_copied_data {
                    for (_, field) in &row.entries {
                        self.check(field, direction, depth + 1);
                    }
                } else {
                    let reason = self.table.refusal_reason("record").to_owned();
                    self.refuse(ty, direction, &reason);
                }
            }
            Type::Con { .. } | Type::App { .. } => self.check_con(ty, direction, depth),
            Type::Captured { ty: inner, .. } | Type::Bounded { ty: inner, .. } => {
                self.check(inner, direction, depth + 1);
            }
        }
    }

    fn check_fun(&mut self, ty: &Type, direction: AbiDirection, depth: usize) {
        let Some(entry) = self.table.entry("function") else {
            let reason = self.table.refusal_reason("function").to_owned();
            self.refuse(ty, direction, &reason);
            return;
        };
        if !entry.direction.allows(direction) {
            self.refuse_direction(ty, direction, entry.direction);
            return;
        }
        let stages = self.checker.arrow_stages(ty);
        let arity = stages
            .iter()
            .position(|stage| *stage != Stage::Pure)
            .map_or(stages.len(), |i| i + 1);
        let params = self.checker.params_of(ty);
        let flipped = direction.flip();
        for param in params.iter().take(arity) {
            self.check(param, flipped, depth + 1);
        }
        if let Some(result) = self.checker.call_result(ty, arity) {
            self.check(&result, direction, depth + 1);
        }
    }

    fn check_con(&mut self, ty: &Type, direction: AbiDirection, depth: usize) {
        let (head, args) = ty.spine();
        let Type::Con { module, name } = head else {
            // Higher-kinded type variable application `f a` crosses as opaque.
            return;
        };

        if let Some(lang) = lang_of_con(module, name)
            && self.check_lang_item(ty, lang, &args, direction, depth)
        {
            return;
        }
        if self.visited_types.contains(&(module.clone(), name.clone())) {
            for arg in args {
                self.check(arg, direction, depth + 1);
            }
            return;
        }
        self.check_type_form(ty, module, name, &args, direction, depth);
    }

    fn check_lang_item(
        &mut self,
        ty: &Type,
        lang: Lang,
        args: &[&Type],
        direction: AbiDirection,
        depth: usize,
    ) -> bool {
        let key = lang.name();
        if let Some(entry) = self.table.entry(key) {
            if entry.direction.allows(direction) {
                for arg in args {
                    self.check(arg, direction, depth + 1);
                }
            } else {
                self.refuse_direction(ty, direction, entry.direction);
            }
            return true;
        }
        if let Some(reason) = self.table.unsupported_reason(key) {
            let reason = reason.to_owned();
            self.refuse(ty, direction, &reason);
            return true;
        }
        if CORE_VALUE_LANG_ITEMS.contains(&lang) {
            let reason = self.table.fallback_reason.clone();
            self.refuse(ty, direction, &reason);
            return true;
        }
        false
    }

    fn check_type_form(
        &mut self,
        ty: &Type,
        module: &str,
        name: &str,
        args: &[&Type],
        direction: AbiDirection,
        depth: usize,
    ) {
        let Some(def) = self.checker.env.type_def(module, name) else {
            return;
        };
        match &def.form {
            Form::Nominal { literals, .. }
            | Form::Adt {
                abi: Abi::Literal(literals),
                ..
            } => {
                if let Some(first) = literals.first() {
                    self.check_enum_lit(ty, first, direction);
                }
                // Empty `literals`: opaque nominal handle admitted via `opaque_type`.
            }
            Form::Adt {
                constructors,
                abi: Abi::Unboxed,
            } => {
                let id = (module.to_owned(), name.to_owned());
                if self.visited_types.insert(id.clone()) {
                    if let Some(ctor) = constructors.first()
                        && let Some(field) = self.instantiated_single_field(module, ctor, args)
                    {
                        self.check(&field, direction, depth + 1);
                    }
                    self.visited_types.remove(&id);
                }
            }
            Form::Adt {
                constructors,
                abi: Abi::Untagged(_),
            } => {
                let id = (module.to_owned(), name.to_owned());
                if self.visited_types.insert(id.clone()) {
                    for ctor in constructors {
                        if let Some(field) = self.instantiated_single_field(module, ctor, args) {
                            self.check(&field, direction, depth + 1);
                        }
                    }
                    self.visited_types.remove(&id);
                }
            }
            Form::Adt {
                constructors,
                abi: Abi::Tagged,
            } => {
                if self.table.admits_copied_data {
                    let id = (module.to_owned(), name.to_owned());
                    if self.visited_types.insert(id.clone()) {
                        for ctor in constructors {
                            for field in self.instantiated_fields(module, ctor, args) {
                                self.check(&field, direction, depth + 1);
                            }
                        }
                        self.visited_types.remove(&id);
                    }
                } else {
                    let reason = self.table.refusal_reason("adt").to_owned();
                    self.refuse(ty, direction, &reason);
                }
            }
            Form::Error => {
                if !self.table.admits_copied_data {
                    let reason = self.table.refusal_reason("adt").to_owned();
                    self.refuse(ty, direction, &reason);
                }
            }
            Form::Alias { .. }
            | Form::Service { .. }
            | Form::Class { .. }
            | Form::ModuleContract { .. }
            | Form::ModuleSignature { .. }
            | Form::Kind
            | Form::KindMember => {}
        }
    }

    fn check_enum_lit(&mut self, ty: &Type, lit: &EnumLit, direction: AbiDirection) {
        let base = match lit {
            EnumLit::String(_) => Type::lang(Lang::String),
            EnumLit::Int(_) => Type::lang(Lang::Int),
        };
        let key = match lit {
            EnumLit::String(_) => "string",
            EnumLit::Int(_) => "int",
        };
        if let Some(entry) = self.table.entry(key) {
            if !entry.direction.allows(direction) {
                self.refuse_direction(ty, direction, entry.direction);
            }
        } else {
            let reason = self.table.refusal_reason(key).to_owned();
            self.refuse(&base, direction, &reason);
        }
    }

    fn instantiated_single_field(&self, module: &str, ctor: &str, args: &[&Type]) -> Option<Type> {
        self.instantiated_fields(module, ctor, args)
            .into_iter()
            .next()
    }

    fn instantiated_fields(&self, module: &str, ctor: &str, args: &[&Type]) -> Vec<Type> {
        let entity = Entity {
            module: module.to_owned(),
            namespace: Namespace::Constructor,
            name: ctor.to_owned(),
        };
        let Some(scheme) = self.checker.env.value(&entity) else {
            return Vec::new();
        };
        let instantiated = match scheme {
            Type::Forall { vars, body, .. } => {
                let subst: Vec<(String, Type)> = vars
                    .iter()
                    .zip(args.iter())
                    .map(|(b, arg)| (b.name.clone(), (*arg).clone()))
                    .collect();
                body.substitute(&subst)
            }
            other => other.clone(),
        };
        self.checker.params_of(&instantiated)
    }
}

impl Checker<'_> {
    /// Checks every parameter and result of a direct foreign binding (or
    /// selected `@native` override) against `spec.value_abi`, emitting
    /// `FLICK-F013` with the backend's refusal reason at `span`.
    pub(super) fn check_foreign_value_abi(
        &mut self,
        foreign_decl: Option<&Node>,
        info: &ForeignInfo,
        params: &[Type],
        body: &Type,
        spec: &ForeignBackendSpec,
        span: Span,
    ) {
        if !self.options.validate_foreign_metadata {
            return;
        }
        if foreign_decl.is_some_and(|decl| is_runtime_internal_foreign(decl, self.text)) {
            return;
        }
        let Some(table) = spec.value_abi.as_ref() else {
            return;
        };
        let arity = info.js_arity();
        let mut check = AbiCheck {
            checker: self,
            table,
            backend_id: &spec.backend_id,
            reported: BTreeSet::new(),
            errors: Vec::new(),
            visited_types: HashSet::new(),
        };
        for (i, param) in params.iter().take(arity).enumerate() {
            if info.adapter.optional.contains(&i) {
                let unwrapped = self.whnf_captured(param).0;
                let (head, args) = unwrapped.spine();
                if head.is_lang(Lang::Option)
                    && let [inner] = args.as_slice()
                {
                    check.check(inner, AbiDirection::In, 0);
                    continue;
                }
            }
            check.check(param, AbiDirection::In, 0);
        }
        let result = self.call_result(body, arity).or_else(|| {
            (arity == 0 && info.mode == ForeignMode::Pure && params.is_empty())
                .then(|| self.whnf_captured(body).0)
        });
        if let Some(result) = result {
            if info.adapter.result.nullable {
                let unwrapped = self.whnf_captured(&result).0;
                let (head, args) = unwrapped.spine();
                if head.is_lang(Lang::Option)
                    && let [inner] = args.as_slice()
                {
                    check.check(inner, AbiDirection::Out, 0);
                } else {
                    check.check(&result, AbiDirection::Out, 0);
                }
            } else {
                check.check(&result, AbiDirection::Out, 0);
            }
        }
        let errors = check.errors;
        for message in errors {
            self.error(codes::F013, span, message);
        }
    }
}
