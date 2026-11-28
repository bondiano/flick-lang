//! Lowering module type declarations, constructor field types, and solved
//! typing tables into IR types after type checking finishes.

use std::collections::BTreeSet;
use std::mem;

use flick_ir::{ConstructorDecl, ForeignTypeForm, IrType, Rep, TypeDecl, TypeRef};

use super::Checker;
use super::env::Form;
use super::gadt::scheme::{built, is_hidden_captures};
use super::ir_types::{IrTypeLower, is_layout_kind, kind_params, peel_scheme};
use super::kinds::Kind;
use super::options::ModuleLookup;
use super::ty::Type;
use super::typing::Typing;
use super::variance::decl_param_names;
use crate::iface::Namespace;
use crate::syntax::attr::attr_parts;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal::string_value;

impl Checker<'_> {
    /// Populates the IR type tables on `typing` once the module is fully solved.
    pub(super) fn finish_ir_types(
        &mut self,
        typing: &mut Typing,
        ir_lookup: Option<ModuleLookup<'_>>,
    ) {
        let pending_ir = mem::take(&mut typing.pending_ir_values);
        let pending_args = mem::take(&mut typing.pending_type_args);
        let pending_cons = mem::take(&mut typing.pending_con_uses);
        let pending_locals = mem::take(&mut typing.pending_ir_local_schemes);
        let pending_coercions = mem::take(&mut typing.pending_coercion_targets);

        let mut lower = IrTypeLower::with_lookup(self, ir_lookup);

        for (at, ty) in pending_ir.iter().chain(&typing.pending_types) {
            if typing.ir_values.contains_key(at) {
                continue;
            }
            if let Some(ir_ty) = lower.lower(ty) {
                typing.ir_values.insert(*at, ir_ty);
            }
        }
        for (at, ty) in &pending_coercions {
            if let Some(ir_ty) = lower.lower(ty) {
                typing.ir_coercion_targets.insert(*at, ir_ty);
            }
        }
        for (at, args) in &pending_args {
            let lowered: Option<Vec<TypeRef>> = args
                .iter()
                .filter(|(k, _)| is_layout_kind(k))
                .map(|(_, t)| lower.lower(t))
                .collect();
            if let Some(lowered) = lowered {
                typing.ir_type_args.entry(*at).or_insert(lowered);
            }
        }
        for (at, entity, args) in &pending_cons {
            let type_args: Option<Vec<TypeRef>> = args
                .iter()
                .filter(|(k, _)| is_layout_kind(k))
                .map(|(_, t)| lower.lower(t))
                .collect();
            if let Some(type_args) = type_args {
                typing.ir_type_args.entry(*at).or_insert(type_args.clone());
                if let Some(scheme) = self.env.value(entity) {
                    let (vars, _, body) = peel_scheme(scheme);
                    let layout_vars: Vec<String> = vars
                        .iter()
                        .filter(|v| !is_hidden_captures(v) && is_layout_kind(&v.kind))
                        .map(|v| v.name.clone())
                        .collect();
                    let raw_fields = fun_params(body);
                    let decl_fields: Option<Vec<TypeRef>> =
                        raw_fields.iter().map(|f| lower.lower(f)).collect();
                    if let Some(decl_fields) = decl_fields {
                        if layout_vars.len() == type_args.len() {
                            let subst: Vec<(&str, &TypeRef)> = layout_vars
                                .iter()
                                .map(String::as_str)
                                .zip(type_args.iter())
                                .collect();
                            let inst: Vec<TypeRef> = decl_fields
                                .iter()
                                .map(|f| f.substitute(&subst, &mut lower.interner))
                                .collect();
                            typing.ir_con_fields.insert(*at, inst);
                        } else if layout_vars.is_empty() {
                            typing.ir_con_fields.insert(*at, decl_fields);
                        }
                    }
                }
            }
        }
        for (at, scheme) in &pending_locals {
            if let Some(lowered) = lower.lower_scheme(scheme) {
                typing.ir_local_schemes.insert(*at, lowered);
            }
        }

        // Top-level value, foreign and class-default schemes.
        let top_entities: Vec<(String, Type)> = self
            .env
            .local_values()
            .map(|(n, t)| (n.to_owned(), t.clone()))
            .collect();
        for (name, scheme) in &top_entities {
            if let Some(lowered) = lower.lower_scheme(scheme) {
                typing.ir_top_schemes.insert(name.clone(), lowered);
            }
        }

        // Module type declarations and constructor field types.
        self.collect_ir_type_decls(&mut lower, typing);
    }

    /// Lowers only the top-level schemes of `foreign` declarations when full
    /// IR type lowering is skipped (for example during `CachedIr::Untyped`
    /// target preflight), so backends always have `Foreign.ty`.
    pub(super) fn finish_foreign_ir_types(
        &self,
        typing: &mut Typing,
        ir_lookup: Option<ModuleLookup<'_>>,
    ) {
        let Some(root) = self.root else { return };
        let mut lower = IrTypeLower::with_lookup(self, ir_lookup);
        for decl in root.nodes() {
            if decl.kind == K::ForeignDecl {
                let Some(name) = super::toplevel::foreign_name(decl, self.text) else {
                    continue;
                };
                self.lower_foreign_top_scheme(&mut lower, typing, name);
            } else if decl.kind == K::LetDecl
                && typing
                    .native_overrides
                    .iter()
                    .any(|(r, _)| *r == decl.range())
            {
                if let Some(name) = super::bindings::let_parts(decl, self.text).name {
                    self.lower_foreign_top_scheme(&mut lower, typing, &name);
                }
            } else if decl.kind == K::RecGroup {
                for member in decl
                    .nodes()
                    .filter(|n| n.kind == K::Members)
                    .flat_map(Node::nodes)
                {
                    if member.kind == K::LetDecl
                        && typing
                            .native_overrides
                            .iter()
                            .any(|(r, _)| *r == member.range())
                        && let Some(name) = super::bindings::let_parts(member, self.text).name
                    {
                        self.lower_foreign_top_scheme(&mut lower, typing, &name);
                    }
                }
            }
        }
    }

    fn lower_foreign_top_scheme(
        &self,
        lower: &mut IrTypeLower<'_, '_>,
        typing: &mut Typing,
        name: &str,
    ) {
        let entity = self.env.entity(Namespace::Value, name);
        if let Some(scheme) = self.env.value(&entity)
            && let Some(lowered) = lower.lower_scheme(scheme)
        {
            typing.ir_top_schemes.insert(name.to_owned(), lowered);
        }
    }

    fn collect_ir_type_decls(&self, lower: &mut IrTypeLower<'_, '_>, typing: &mut Typing) {
        let Some(root) = self.root else { return };
        let module = &self.env.module;
        let mut seen = BTreeSet::new();
        let mut pending_attrs: Vec<&Node> = Vec::new();
        for decl in root.nodes() {
            if decl.kind == K::Attribute {
                pending_attrs.push(decl);
                continue;
            }
            let attrs = mem::take(&mut pending_attrs);
            if !matches!(decl.kind, K::TypeDecl | K::ErrorDecl) {
                continue;
            }
            let Some(name_tok) = decl.child_tokens().find(|t| t.kind == K::Con) else {
                continue;
            };
            let name = name_tok.text(self.text).to_owned();
            if !seen.insert(name.clone()) {
                continue;
            }
            let Some(def) = self.env.type_def(module, &name) else {
                continue;
            };
            if matches!(def.form, Form::Alias { opaque: false, .. }) {
                continue;
            }
            let foreign_forms = self.collect_foreign_type_forms(&attrs);
            let kinds = kind_params(&def.kind);
            let written_names = decl_param_names(root, self.text, &name);
            let params: Vec<String> = kinds
                .iter()
                .enumerate()
                .filter(|(_, k)| is_layout_kind(k))
                .map(|(i, _)| {
                    written_names
                        .as_ref()
                        .and_then(|ns| ns.get(i))
                        .cloned()
                        .unwrap_or_else(|| format!("a{i}"))
                })
                .collect();
            let error = matches!(def.form, Form::Error);
            let alias = match &def.form {
                Form::Alias {
                    body, opaque: true, ..
                } => lower.lower(body),
                _ => None,
            };
            let mut constructors = Vec::new();
            let variants: Vec<&Node> = decl.nodes().filter(|n| n.kind == K::Variant).collect();
            let abi = typing.abis.get(&name).cloned().unwrap_or_default();
            for (i, variant) in variants.iter().enumerate() {
                let Some(cname_tok) = variant.child_tokens().find(|t| t.kind == K::Con) else {
                    continue;
                };
                let cname = cname_tok.text(self.text).to_owned();
                let rep = abi.rep(i);
                let record = variant_record_labels(variant, self.text);
                if let Some(cdecl) =
                    self.build_ctor_decl(lower, &name, &params, &kinds, &cname, rep, record)
                {
                    typing.ir_ctor_fields.insert(cname, cdecl.fields.clone());
                    constructors.push(cdecl);
                }
            }
            if error
                && variants.is_empty()
                && let Some(cdecl) =
                    self.build_ctor_decl(lower, &name, &params, &kinds, &name, Rep::Tagged, None)
            {
                typing
                    .ir_ctor_fields
                    .insert(name.clone(), cdecl.fields.clone());
                constructors.push(cdecl);
            }
            typing.ir_type_decls.push(TypeDecl {
                name,
                params,
                error,
                constructors,
                alias,
                foreign_forms,
            });
        }
    }

    fn collect_foreign_type_forms(&self, attrs: &[&Node]) -> Vec<ForeignTypeForm> {
        let mut forms = Vec::new();
        for attr in attrs {
            let Some(parts) = attr_parts(attr, self.text) else {
                continue;
            };
            if parts.namespaced
                || parts.name == "ts"
                || self
                    .options
                    .foreign_backend
                    .type_form_attribute(parts.name)
                    .is_none()
            {
                continue;
            }
            let rest: Vec<(K, &str)> = attr
                .child_tokens()
                .filter(|t| t.start >= parts.name_end)
                .map(|t| (t.kind, t.text(self.text)))
                .collect();
            match rest.as_slice() {
                [(K::String, form)] => forms.push(ForeignTypeForm {
                    attr: parts.name.to_owned(),
                    form: string_value(form),
                    from: None,
                }),
                [
                    (K::Ident, "from" | "import"),
                    (K::String, pkg),
                    (K::String, form),
                ] => forms.push(ForeignTypeForm {
                    attr: parts.name.to_owned(),
                    form: string_value(form),
                    from: Some(string_value(pkg)),
                }),
                _ => {}
            }
        }
        forms
    }

    #[allow(clippy::too_many_arguments)]
    fn build_ctor_decl(
        &self,
        lower: &mut IrTypeLower<'_, '_>,
        type_name: &str,
        decl_params: &[String],
        decl_kinds: &[&Kind],
        ctor_name: &str,
        rep: Rep,
        record: Option<Vec<String>>,
    ) -> Option<ConstructorDecl> {
        let entity = self.env.entity(Namespace::Constructor, ctor_name);
        let scheme = self.env.value(&entity)?;
        let (vars, raw_constraints, body) = peel_scheme(scheme);
        let type_params: Vec<String> = vars
            .iter()
            .filter(|v| !is_hidden_captures(v) && is_layout_kind(&v.kind))
            .map(|v| v.name.clone())
            .collect();
        let constraints = lower.lower_constraints(&raw_constraints);
        let raw_fields = fun_params(body);
        let fields: Vec<TypeRef> = raw_fields
            .iter()
            .map(|f| lower.lower(f))
            .collect::<Option<_>>()?;
        let (res_head, res_args) = built(body).spine();
        let ir_res_args: Option<Vec<TypeRef>> = if matches!(
            res_head,
            Type::Con { module, name } if *module == self.env.module && name == type_name
        ) {
            res_args
                .iter()
                .enumerate()
                .filter(|(i, _)| decl_kinds.get(*i).is_none_or(|k| is_layout_kind(k)))
                .map(|(_, a)| lower.lower(a))
                .collect()
        } else {
            None
        };
        let is_plain = ir_res_args.as_ref().is_some_and(|args| {
            args.len() == decl_params.len()
                && type_params == decl_params
                && args
                    .iter()
                    .zip(decl_params)
                    .all(|(a, p)| matches!(&**a, IrType::Var(v) if v == p))
        });
        let result_args = if is_plain { None } else { ir_res_args.clone() };
        let mut free_in_res = BTreeSet::new();
        if let Some(args) = &ir_res_args {
            for a in args {
                collect_free_vars(a, &mut free_in_res);
            }
        }
        let existentials: Vec<String> = type_params
            .iter()
            .filter(|v| !free_in_res.contains(v.as_str()))
            .cloned()
            .collect();
        Some(ConstructorDecl {
            name: ctor_name.to_owned(),
            rep,
            type_params,
            existentials,
            constraints,
            fields,
            record,
            result_args,
        })
    }
}

fn fun_params(mut ty: &Type) -> Vec<&Type> {
    let mut out = Vec::new();
    while let Type::Fun { param, result, .. } = ty {
        out.push(param.as_ref());
        ty = result;
    }
    out
}

fn collect_free_vars(ty: &IrType, out: &mut BTreeSet<String>) {
    match ty {
        IrType::Var(v) => {
            out.insert(v.clone());
        }
        IrType::Record { fields, tail } => {
            for (_, t) in fields {
                collect_free_vars(t, out);
            }
            if let Some(t) = tail {
                out.insert(t.clone());
            }
        }
        IrType::Forall {
            vars,
            constraints,
            body,
        } => {
            let mut inner = BTreeSet::new();
            for c in constraints {
                for a in &c.args {
                    collect_free_vars(a, &mut inner);
                }
            }
            collect_free_vars(body, &mut inner);
            for v in vars {
                inner.remove(v);
            }
            out.extend(inner);
        }
        other => other.each_child(&mut |c| collect_free_vars(c, out)),
    }
}

fn variant_record_labels(variant: &Node, text: &str) -> Option<Vec<String>> {
    let mut fields = variant.nodes().filter(|n| super::convert::is_type(n.kind));
    let (record, None) = (fields.next()?, fields.next()) else {
        return None;
    };
    if record.kind != K::RecordType {
        return None;
    }
    let mut labels: Vec<String> = record
        .nodes()
        .map(|f| {
            (f.kind == K::RecordTypeField).then(|| {
                f.child_tokens()
                    .find(|t| t.kind == K::Ident)
                    .map(|t| t.text(text).to_owned())
            })?
        })
        .collect::<Option<_>>()?;
    flick_ir::sort_strings_utf16(&mut labels);
    Some(labels)
}
