//! Integration tests of the `flick` command, one module per former test
//! binary: most of those linked the compiler again. Binaries of their own: the
//! suites a recipe selects by name (`e2e`, `go_main`, `unicode_path`,
//! `sql_postgres`, `conduit`, `plugins`, `backends`, `default_backend`),
//! `ffi_command`, whose snapshot files are named by its module path, and
//! `ignored_tests`, which the justfile names.

#[path = "../common/mod.rs"]
mod common;

// `pub`, as each was a crate root: what a file exports still counts as used
pub mod backend_options;
pub mod build_command;
pub mod build_inputs;
pub mod build_runner;
pub mod build_ui;
pub mod builtins_cache;
pub mod builtins_timings;
pub mod check_command;
pub mod cli_devtools;
pub mod cli_entries;
pub mod cli_fullstack;
pub mod cli_project;
pub mod cli_run;
pub mod cli_run_client;
pub mod cli_template;
pub mod cli_toolchain;
pub mod content_mapper;
pub mod control_handlers;
pub mod diff_command;
pub mod doc_run_examples;
pub mod dts_command;
pub mod e2e_native_primitives;
pub mod e2e_numbers;
pub mod examples;
pub mod fetched_dependencies;
pub mod ffi_forms;
pub mod first_slice;
pub mod fmt_command;
pub mod help_command;
pub mod lsp_command;
pub mod module_store;
pub mod package_cache;
pub mod package_js;
pub mod package_ts;
pub mod project_check;
pub mod prove_bundles;
pub mod prove_command;
pub mod rpc_cross;
pub mod rpc_rt;
pub mod rt_stages;
pub mod schema_export;
pub mod source_packages;
pub mod test_blocks;
pub mod test_command;
pub mod test_companions;
pub mod test_each;
pub mod test_fixtures;
pub mod test_hooks;
pub mod test_suite;
pub mod wasm_demo;
pub mod workflows;
pub mod workspace;
