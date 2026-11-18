//! The Go runtime, written beside every program at `flickrt/`.

/// The runtime's files: name and contents, every `.go` file of
/// `runtime-go/flickrt` but its tests (`build.rs`).
pub const RUNTIME: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/go_runtime.rs"));

/// An optional package of the runtime, written beside a program that
/// reaches it, such as the `PostgreSQL` driver `flickpg`.
pub struct Package {
    pub name: &'static str,
    /// Its `.go` files but tests, by path in the program's Go module.
    pub files: &'static [(&'static str, &'static str)],
    /// The modules it needs, one path a line (`modules.txt`); `GO_MOD`
    /// has their versions.
    pub modules: &'static str,
}

/// Every directory of `runtime-go` but `flickrt` (`build.rs`).
pub const PACKAGES: &[Package] = include!(concat!(env!("OUT_DIR"), "/go_packages.rs"));

/// The runtime's `go.mod` and `go.sum`, which require and pin the modules
/// of every package; a program's `go.mod` keeps those of its packages.
pub const GO_MOD: &str = include_str!("../../../../runtime-go/go.mod");
pub const GO_SUM: &str = include_str!("../../../../runtime-go/go.sum");

/// The `go.mod` of a program with `packages`: the runtime's, requiring only
/// the modules of those packages, so a program without one builds offline
/// and a package's version skew never makes Go load the others.
pub fn go_mod(packages: &[&Package]) -> String {
    if packages.is_empty() {
        return "module flick.app\n\ngo 1.23\n".to_owned();
    }
    let needed: Vec<&str> = packages.iter().flat_map(|p| p.modules.lines()).collect();
    let mut header = String::new();
    let mut requires = String::new();
    let mut block = false;
    for line in GO_MOD.lines() {
        let line = line.trim();
        let require = if block {
            block = line != ")";
            line
        } else if line == "require (" {
            block = true;
            continue;
        } else if let Some(rest) = line.strip_prefix("require ") {
            rest
        } else {
            if line.starts_with("module ") || line.starts_with("go ") {
                header.push_str(line);
                header.push_str("\n\n");
            }
            continue;
        };
        if require
            .split_whitespace()
            .next()
            .is_some_and(|m| needed.contains(&m))
        {
            requires.push('\t');
            requires.push_str(require);
            requires.push('\n');
        }
    }
    format!("{header}require (\n{requires})\n")
}
