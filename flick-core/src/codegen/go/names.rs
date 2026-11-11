//! Go identifiers of what the IR names. Every program is one package, so a
//! top-level name carries its module; a local is unique in its definition.

use std::fmt::Write;

/// `name` with every character Go does not allow in an identifier escaped.
fn mangle(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            let _ = write!(out, "_x{:x}_", u32::from(c));
        }
    }
    out
}

fn top(prefix: &str, module: &str, name: &str) -> String {
    format!(
        "{prefix}_{}__{}",
        mangle(&module.replace('.', "_")),
        mangle(name)
    )
}

/// The variable of a top-level value that is not a function.
pub fn var(module: &str, name: &str) -> String {
    top("V", module, name)
}

/// The Go function of a top-level function, taking every argument at once.
pub fn func(module: &str, name: &str) -> String {
    top("F", module, name)
}

/// The function running a group of mutually tail-recursive functions,
/// named after its first member.
pub fn group(module: &str, first: &str) -> String {
    top("G", module, first)
}

/// The dictionary of an instance, or the function of its context making it.
pub fn instance(module: &str, name: &str) -> String {
    top("I", module, name)
}

/// The function that initializes the top-level values of a module.
pub fn init(module: &str) -> String {
    format!("init_{}", mangle(&module.replace('.', "_")))
}

/// The initializer of one reached top-level item.
pub fn item_init(module: &str, index: usize) -> String {
    format!("{}__item_{index}", init(module))
}

/// The file of a module's code.
pub fn file(module: &str) -> String {
    format!("m_{}.go", mangle(&module.replace('.', "_")))
}

pub fn local(name: &str) -> String {
    format!("l_{}", mangle(name))
}

/// A Go string literal; everything outside printable ASCII is escaped.
pub fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            c if u32::from(c) <= 0xFFFF => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => {
                let _ = write!(out, "\\U{:08x}", u32::from(c));
            }
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_go_identifiers() {
        assert_eq!(var("App.Main", "main"), "V_App_Main__main");
        assert_eq!(
            func("Prelude", "$default_Foldable_fold_map"),
            "F_Prelude___x24_default_Foldable_fold_map"
        );
        assert_eq!(local("x'"), "l_x_x27_");
        assert_eq!(string("a\"\n\u{1F30D}"), "\"a\\\"\\u000a\\U0001f30d\"");
    }
}
