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
