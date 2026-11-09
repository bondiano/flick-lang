fn uncaptured<'a>(mut t: &'a Type, vars: &Vars) -> Result<&'a Type, String> {
    while let Type::Captured { ty, captures, .. } | Type::Bounded { ty, captures } = t {
        if !empty_captures(captures, vars) {
            return Err(
                "a value holding a scope's capabilities has no bare TypeScript export".to_owned(),
            );
        }
        t = ty;
    }
    Ok(t)
}

fn empty_captures(t: &Type, vars: &Vars) -> bool {
    match t {
        Type::Row(row) => {
            row.entries.iter().all(|(_, t)| empty_captures(t, vars))
                && row.tail.as_deref().is_none_or(|t| empty_captures(t, vars))
        }
        Type::Union(a, b) => empty_captures(a, vars) && empty_captures(b, vars),
        // TypeScript type parameters contain only scope-free data.
        Type::Var(name) => matches!(vars.get(name), Some(Some(_))),
        _ => false,
    }
}

/// The kinds of the parameters of a type constructor of kind `k`.
fn params(k: &Kind) -> Vec<Kind> {
    let mut out = Vec::new();
    let mut k = k;
    while let Kind::Arrow(p, r) = k {
        out.push((**p).clone());
        k = r;
    }
    out
}

/// The TypeScript name of a type variable: `a` is `A`, `elem` is `Elem`.
fn generic(name: &str) -> String {
    let mut chars = name.chars();
    let first = chars.next().map_or('T', |c| c.to_ascii_uppercase());
    let rest: String = chars
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    format!("{first}{rest}")
}

fn type_args(args: &[String]) -> String {
    if args.is_empty() {
        String::new()
    } else {
        format!("<{}>", args.join(", "))
    }
}

/// `Effect<A, E, R>` with `never` for an empty row.
fn effect(a: &str, e: &str, r: &str) -> String {
    format!("$Effect.Effect<{a}, {e}, {r}>")
}

fn key(label: &str) -> String {
    if names::is_property(label) {
        label.to_owned()
    } else {
        names::string(label)
    }
}

mod declare;
mod dts;
pub mod stages;

pub use dts::{Facade, declaration_file, facade_files, jsdoc};
