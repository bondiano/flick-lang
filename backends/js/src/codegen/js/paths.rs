/// The import specifier of `to` from `from`, both relative to the output
/// root; `npm:` marks a module of a package.
pub fn relative(from: &str, to: &str) -> String {
    if let Some(bare) = to.strip_prefix("npm:") {
        return bare.to_owned();
    }
    let from_dir: Vec<&str> = from.split('/').collect();
    let from_dir = &from_dir[..from_dir.len() - 1];
    let to_parts: Vec<&str> = to.split('/').collect();
    let common = from_dir
        .iter()
        .zip(&to_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let up = from_dir.len() - common;
    let rest = to_parts[common..].join("/");
    if up == 0 {
        format!("./{rest}")
    } else {
        format!("{}{rest}", "../".repeat(up))
    }
}
