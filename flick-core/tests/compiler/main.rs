//! Integration tests of the compiler, one module per former test binary: each
//! of those linked the compiler again. Binaries of their own: the suites a
//! recipe or the docs select by name (`check`, `corpus`, `features`, `fuzz`,
//! `ir_round_trip`) and `scaling`, whose timings need no neighbours.

// `pub`, as each was a crate root: what a file exports still counts as used
pub mod alias_kinds;
pub mod builtin_provenance;
pub mod consteval_node;
pub mod counted_bigint_conversion;
pub mod counted_int_string_adapters;
pub mod counted_scalar_adapters;
pub mod implicit_imports;
pub mod implicit_packages;
pub mod module_contract_publication;
pub mod projection;
pub mod service_implementation_contracts;
pub mod source_package_layout;
pub mod stdlib_signatures;
pub mod transformer_captures;
