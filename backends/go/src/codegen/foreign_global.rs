//! Direct Go code generation for qualified Go package globals such as
//! `@native "math.Hypot" when go` or `foreign pure ... from global "strings.HasPrefix"`.

use flick_backend_support::Known;
use flick_ir::{Binding as IrBinding, Foreign, ForeignMode, IrType, Span};
use flick_pretty::{Document, Documentable, docvec};

use super::arith::Native;
use super::go_type::GoType;
use super::layout::{call, signature, single, typed_signature};
use super::shape::Types;
use super::{Gen, names};

/// A parsed Go qualified package symbol: import path (e.g. `"math"` or `"math/bits"`),
/// default package identifier (`"math"`, `"bits"`), and exported symbol (`"Hypot"`).
pub(super) struct GoGlobal<'a> {
    pub(super) import_path: &'a str,
    pub(super) default_pkg: &'a str,
    pub(super) export: &'a str,
}

/// Parses a Go qualified global target such as `"math.Hypot"` or `"math/bits.OnesCount32"`.
/// Returns `None` for non-Go or unqualified names (such as JS globals like `"Math.hypot"`
/// where the package starts with uppercase or has invalid Go export syntax).
pub(super) fn parse_go_global(raw: &str) -> Option<GoGlobal<'_>> {
    let (import_path, export) = raw.rsplit_once('.')?;
    if import_path.is_empty()
        || import_path.starts_with('.')
        || import_path.starts_with('/')
        || import_path.ends_with('/')
    {
        return None;
    }
    let mut default_pkg = "";
    for seg in import_path.split('/') {
        if seg.is_empty()
            || !seg
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        {
            return None;
        }
        default_pkg = seg;
    }
    // Go package identifiers are lowercase; avoid mistaking JS globals (`Math.hypot`).
    if !default_pkg
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_lowercase())
        || !default_pkg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return None;
    }
    // Go exported symbols start with an ASCII uppercase letter.
    if !export
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_uppercase())
        || !export
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return None;
    }
    Some(GoGlobal {
        import_path,
        default_pkg,
        export,
    })
}

/// Infers direct Go native scalar [`Types`] for a qualified Go package global
/// from its IR signature `f.ty` when no wrapper adapters alter its ABI.
pub(super) fn go_global_types(lang: &Known, f: &Foreign, params: usize) -> Types {
    let IrBinding::Global { name } = &f.binding else {
        return Types::default();
    };
    if parse_go_global(name).is_none() {
        return Types::default();
    }
    if f.dicts > 0
        || f.mode == ForeignMode::Async
        || f.catch.is_some()
        || f.adapter.result.nullable
        || f.adapter.result.unit
        || !f.adapter.optional.is_empty()
        || !f.adapter.words.is_empty()
    {
        return Types::default();
    }
    let Some(mut ty) = f.ty.as_deref() else {
        return Types::default();
    };
    while let IrType::Forall { body, .. } = ty {
        ty = body;
    }
    let mut param_natives = Vec::with_capacity(params);
    for _ in 0..params {
        match ty {
            IrType::Fun { param, result, .. } => {
                param_natives.push(Native::of(lang, Some(param)));
                ty = result;
            }
            _ => return Types::default(),
        }
    }
    let result_native = Native::of(lang, Some(ty));
    if f.arity == 0 {
        // `Unit ->{} T`: Go function takes 0 args; keep param 0 (`()`) as `flickrt.Value`.
        Types::of_natives(result_native, &[None])
    } else if f.arity == params {
        Types::of_natives(result_native, &param_natives)
    } else {
        Types::default()
    }
}

impl<'a> Gen<'a> {
    /// Registers a Go import path for this module, returning the package identifier
    /// to qualify exported symbols with.
    pub(super) fn add_go_import(&mut self, import_path: &str, default_pkg: &str) -> &'a str {
        if let Some(pkg) = self.go_imports.get(import_path) {
            return self.a.alloc_str(pkg);
        }
        let used = self.go_import_idents.contains(default_pkg) || default_pkg == "flickrt";
        let (alias, ident) = if used {
            let mut n = 0usize;
            let chosen = loop {
                let candidate = format!("{default_pkg}_{n}");
                if !self.go_import_idents.contains(&candidate) {
                    break candidate;
                }
                n += 1;
            };
            (Some(chosen.clone()), chosen)
        } else {
            (None, default_pkg.to_owned())
        };
        self.go_import_idents.insert(ident.clone());
        self.go_imports
            .insert(import_path.to_owned(), ident.clone());
        self.go_extra_imports.push((alias, import_path.to_owned()));
        self.a.alloc_str(&ident)
    }

    /// Emits a Go wrapper for a qualified Go package global such as `"math.Hypot"`.
    pub(super) fn global_foreign(
        &mut self,
        f: &Foreign,
        raw_name: &str,
        _span: Span,
    ) -> Option<Document<'a>> {
        let parsed = parse_go_global(raw_name)?;
        let a = self.a;
        let pkg = self.add_go_import(parsed.import_path, parsed.default_pkg);
        let target = a.alloc_str(&format!("{pkg}.{}", parsed.export));
        let taken = super::foreign::parameters(f);
        if f.dicts == 0 && taken == 0 {
            let var = names::var(self.module, &f.name);
            return Some(docvec![a, "var ", var, " flickrt.Value = ", target]);
        }
        let types = go_global_types(self.input.lang, f, taken);
        let heads: Vec<String> = (0..f.dicts)
            .map(|i| format!("d{i}"))
            .chain((0..taken).map(|i| format!("a{i}")))
            .collect();
        let head_refs: Vec<&str> = heads.iter().map(String::as_str).collect();
        let call_args: Vec<Document<'a>> = if f.arity == 0 {
            Vec::new()
        } else {
            (0..taken)
                .map(|i| heads[f.dicts + i].clone().to_doc(a))
                .collect()
        };
        let go_call = call(a, target, call_args);
        if types.any() {
            let result = types
                .result()
                .map_or_else(|| "flickrt.Value".to_owned(), GoType::go_type);
            let params = typed_signature(&head_refs, f.dicts, &types);
            return Some(self.wrapper(f, params, &result, go_call));
        }
        let go = if f.adapter.result.unit {
            let name = names::func(self.module, &f.name);
            let body = super::layout::block(
                a,
                [
                    super::layout::statement(a, go_call),
                    super::layout::statement(a, "return flickrt.Unit"),
                ],
            );
            return Some(docvec![
                a,
                "func ",
                name,
                "(",
                signature(&head_refs),
                ") flickrt.Value ",
                body
            ]);
        } else {
            self.wrap_foreign_result(f, go_call, false)
        };
        let name = names::func(self.module, &f.name);
        let body = single(a, docvec![a, "return ", go]);
        Some(docvec![
            a,
            "func ",
            name,
            "(",
            signature(&head_refs),
            ") flickrt.Value ",
            body
        ])
    }
}
