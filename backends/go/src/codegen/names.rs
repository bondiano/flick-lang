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

/// The Go type of the values of a constructor that has one of its own.
pub fn own_type(module: &str, constructor: &str) -> String {
    top("T", module, constructor)
}

/// The package variable of a value a module makes once.
pub fn once(module: &str, index: usize) -> String {
    top("K", module, &index.to_string())
}

/// The function that initializes the top-level values of a module.
pub fn init(module: &str) -> String {
    format!("init_{}", mangle(&module.replace('.', "_")))
}

/// The initializer of one reached top-level item.
pub fn item_init(module: &str, index: usize) -> String {
    format!("{}__item_{index}", init(module))
}

/// The file of a module's code. `go build` reads the last `_` part of a file
/// name in lower case as a constraint, `_test` as a test `go test` runs and
/// `_linux` or `_386` as a platform, so such a name ends in `-`, which no
/// mangled name has: a test companion `App.Users$test`, or `App.Users_linux`.
pub fn file(module: &str) -> String {
    let stem = mangle(&module.replace('.', "_"));
    let last = stem.rsplit('_').next().unwrap_or_default();
    let constraint =
        !last.is_empty() && (last.bytes()).all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    format!("m_{stem}{}.go", if constraint { "-" } else { "" })
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

    #[test]
    fn no_file_of_a_module_is_one_go_build_leaves_out() {
        assert_eq!(file("App.Users"), "m_App_Users.go");
        assert_eq!(file("App.Users$test"), "m_App_Users_x24_test-.go");
        assert_eq!(file("App.Users_test"), "m_App_Users_test-.go");
        assert_eq!(file("test"), "m_test-.go");
        assert_eq!(file("App.Users_linux"), "m_App_Users_linux-.go");
        assert_eq!(file("App.Users_386"), "m_App_Users_386-.go");
        // two modules never share a file
        assert_eq!(file("App.Users_test_"), "m_App_Users_test_.go");
    }
}
