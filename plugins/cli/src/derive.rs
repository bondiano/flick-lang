//! `deriving Cli.Value`: for a choice among constructors without fields,
//! `Cli.choice` over their names in kebab case, `GreenLight` as
//! `green-light`, or over an enum's literals; for a refined type,
//! `Cli.refined` over its base and its `parse`, which checks its predicate.

use flick_plugin::{Arena, Builder, DeriveTarget, Diagnostic, Name, Ty, flick, literal_text};

/// The code of a type that derives nothing of `Cli.Value`.
pub const NOT_DERIVABLE: &str = "L010";

pub fn derive(target: &DeriveTarget, class: &str) -> Result<Arena, Vec<Diagnostic>> {
    let whole = target.decl.whole();
    if class != "Cli.Value" {
        let message = format!("`{class}` has no derive in std");
        return Err(vec![Diagnostic::error(message, whole)]);
    }
    let mut b = Builder::new(whole);
    let named = b.ty(Ty::Name(target.name.clone()));
    let params: Vec<_> = (target.params.iter())
        .map(|p| b.ty(Ty::Name(p.clone())))
        .collect();
    let head = if params.is_empty() {
        named
    } else {
        b.ty(Ty::Apply {
            function: named,
            arguments: params,
        })
    };
    let own = target.name.rsplit('.').next().unwrap_or(&target.name);
    let parse = if target.wraps.is_some() {
        let check = format!("{}.parse", target.name);
        flick!(b, Cli.refined #own #(Name(&check)) text)
    } else if !target.cases.is_empty() && target.cases.iter().all(|c| c.fields.is_empty()) {
        let choices: Vec<_> = (target.cases.iter())
            .map(|c| {
                let name = c.name.rsplit('.').next().unwrap_or(&c.name);
                let word = match c.literal {
                    Some(span) => literal_text(target.decl.text_of(span)),
                    None => kebab(name),
                };
                flick!(b, (#word, #(Name(&c.name))))
            })
            .collect();
        flick!(b, Cli.choice [#(..choices)] text)
    } else {
        let message = format!(
            "cannot derive `Cli.Value` for `{own}`: it is derived for a choice among constructors without fields, or for a refined type"
        );
        return Err(vec![
            Diagnostic::error(message, whole).with_code(NOT_DERIVABLE),
        ]);
    };
    let instance = flick!(b, instance Cli.Value #head { let parse text = #parse });
    Ok(b.finish(instance))
}

/// `GreenLight` as `green-light`.
fn kebab(name: &str) -> String {
    let mut out = String::new();
    let mut previous: Option<char> = None;
    for c in name.chars() {
        if c.is_ascii_uppercase()
            && previous.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit())
        {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
        previous = Some(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::kebab;

    #[test]
    fn names_are_kebab_case_as_the_runtime_had_them() {
        assert_eq!(kebab("GreenLight"), "green-light");
        assert_eq!(kebab("Red"), "red");
        assert_eq!(kebab("HTTP2Server"), "http2-server");
    }
}
