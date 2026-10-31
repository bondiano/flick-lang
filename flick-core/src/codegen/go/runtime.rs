//! The Go runtime, written beside every program at `flickrt/`.

/// The runtime's files: name and contents, every `.go` file of
/// `runtime-go/flickrt` but its tests (`build.rs`).
pub const RUNTIME: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/go_runtime.rs"));

/// The `PostgreSQL` driver, the package `flickpg`, written beside a program
/// that reaches the `Database` service.
pub const DRIVER: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/go_driver.rs"));

/// The `go.mod` and `go.sum` of a program with the driver: the runtime's,
/// which require pgx and pin it and what it needs.
pub const DRIVER_GO_MOD: &str = include_str!("../../../../runtime-go/go.mod");
pub const DRIVER_GO_SUM: &str = include_str!("../../../../runtime-go/go.sum");
