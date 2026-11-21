//! Formatting and developer-tool projections of parameter variance.

use std::fmt::Write as _;

use super::{Variance, param_kinds};
use crate::iface::{Decl, Export, Interface, Namespace};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::env::Form;

/// Formats one parameter's variance for Markdown (hover, `flick doc`).
pub fn format_parameter_markdown(name: Option<&str>, variance: Variance) -> String {
    match name {
        Some(name) => format!("`{name}` (`{}`, {})", variance.symbol(), variance.name()),
        None => format!("`{}` ({})", variance.symbol(), variance.name()),
    }
}

/// Formats parameter variances for Markdown (hover, `flick doc`).
pub fn format_markdown(names: Option<&[String]>, variance: &[Variance]) -> Option<String> {
    if variance.is_empty() {
        return None;
    }
    let names = names.filter(|ns| ns.len() == variance.len());
    let parts: Vec<String> = variance
        .iter()
        .enumerate()
        .map(|(index, &v)| {
            let name = names.and_then(|ns| ns.get(index)).map(String::as_str);
            format_parameter_markdown(name, v)
        })
        .collect();
    Some(parts.join(", "))
}

/// Formats parameter variances for plain text (`flick inspect --emit variance`).
pub fn format_plain(names: Option<&[String]>, variance: &[Variance]) -> String {
    let names = names.filter(|ns| ns.len() == variance.len());
    variance
        .iter()
        .enumerate()
        .map(|(index, &v)| match names.and_then(|ns| ns.get(index)) {
            Some(name) => format!("{name} ({}, {})", v.symbol(), v.name()),
            None => format!("{} ({})", v.symbol(), v.name()),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Extracts declared parameter names for type, error or class declaration `name`.
pub fn decl_param_names(tree: &Node, text: &str, name: &str) -> Option<Vec<String>> {
    let decl = tree.nodes().find(|n| {
        matches!(n.kind, K::TypeDecl | K::ErrorDecl | K::ClassDecl)
            && n.child_tokens()
                .find(|t| t.kind == K::Con)
                .is_some_and(|t| t.text(text) == name)
    })?;
    let params: Vec<String> = decl
        .nodes()
        .filter(|n| n.kind == K::TypeParam)
        .filter_map(|p| {
            p.child_tokens()
                .find(|t| t.kind == K::Ident)
                .map(|t| t.text(text).to_owned())
        })
        .collect();
    (!params.is_empty()).then_some(params)
}

/// Renders the inferred parameter variances of exported declarations in
/// `interface` for `flick inspect <M> --emit variance`.
///
/// Designed as a single projection over `TypeDef.variance` so class variance
/// (#448) flows through the same formatting path.
pub fn format_interface_summary(
    interface: &Interface,
    param_names: &impl Fn(&Export) -> Option<Vec<String>>,
) -> String {
    let mut out = format!("module {}\n", interface.module);
    for export in &interface.exports {
        if export.entity.namespace != Namespace::Type || export.parent.is_some() {
            continue;
        }
        let Some(def) = &export.definition else {
            continue;
        };
        let arity = param_kinds(&def.kind).len();
        if arity == 0 {
            continue;
        }
        let keyword = match (&def.form, export.decl) {
            (Form::Class { .. }, _) => "class",
            (Form::Error, _) => "type error",
            (Form::Nominal { .. }, Decl::Opaque) => "opaque type",
            (Form::Nominal { .. } | Form::Adt { .. }, _) => "type",
            _ => continue,
        };
        let names = match &def.form {
            Form::Class { params, .. } => Some(params.clone()),
            _ => param_names(export),
        };
        let shown = if def.variance.is_empty() {
            match names.as_deref() {
                Some(names) if names.len() == arity => names
                    .iter()
                    .map(|name| format!("{name} (inv, unknown)"))
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => vec!["inv (unknown)"; arity].join(", "),
            }
        } else {
            format_plain(names.as_deref(), &def.variance)
        };
        let _ = writeln!(out, "{keyword} {} : {shown}", export.entity.name);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Variance, format_markdown, format_plain};

    #[test]
    fn mismatched_parameter_name_arity_falls_back_to_positional_formatting() {
        let signs = [Variance::Invariant, Variance::Covariant];
        let short = vec!["r".to_owned()];
        let exact = vec!["r".to_owned(), "a".to_owned()];
        assert_eq!(
            format_markdown(Some(&short), &signs).as_deref(),
            Some("`inv` (invariant), `+` (covariant)")
        );
        assert_eq!(
            format_markdown(Some(&exact), &signs).as_deref(),
            Some("`r` (`inv`, invariant), `a` (`+`, covariant)")
        );
        assert_eq!(
            format_plain(Some(&short), &signs),
            "inv (invariant), + (covariant)"
        );
        assert_eq!(
            format_plain(Some(&exact), &signs),
            "r (inv, invariant), a (+, covariant)"
        );
    }
}
