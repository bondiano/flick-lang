//! The integration tests of the reference model, one module per former test
//! binary: each of those linked the model and the compiler again. `certs` and
//! `run` stay binaries of their own, since recipes select them by name.

// shared by the modules below, loaded once
#[path = "../support/causes.rs"]
pub mod causes;
mod live_support;
#[path = "../support/recovery.rs"]
mod recovery;

// `pub`, as each was a crate root: what a file exports still counts as used
pub mod accepted;
pub mod arena_regions;
pub mod disagreements;
pub mod foreign_staging;
pub mod gadt_rows;
pub mod generic;
pub mod leaks;
pub mod live;
pub mod live_cases;
pub mod live_core;
pub mod live_install;
pub mod live_kept;
pub mod live_scope;
pub mod memory;
pub mod memory_arena_defects;
pub mod memory_arenas;
pub mod memory_array_observers;
pub mod memory_bigint_arithmetic;
pub mod memory_bigint_conversion;
pub mod memory_builtin_fallback;
pub mod memory_causes;
pub mod memory_derived_dictionaries;
pub mod memory_dictionaries;
pub mod memory_failures;
pub mod memory_handlers;
pub mod memory_int_string_primitives;
pub mod memory_layer_merge;
pub mod memory_layers;
pub mod memory_lexical_owners;
pub mod memory_numeric;
pub mod memory_owner_evidence;
pub mod memory_patterns;
pub mod memory_program;
pub mod memory_recovery_selector;
pub mod memory_retired_owner;
pub mod memory_reuse_receivers;
pub mod memory_scalar_context;
pub mod memory_scalars;
pub mod memory_source;
pub mod memory_source_app_fail;
pub mod memory_source_bigint_arithmetic;
pub mod memory_source_bigint_conversion;
pub mod memory_source_dictionaries;
pub mod memory_source_eff_combinators;
pub mod memory_source_failures;
pub mod memory_source_fold_tasks;
pub mod memory_source_handlers;
pub mod memory_source_host_root;
pub mod memory_source_int_string;
pub mod memory_source_layer_merge;
pub mod memory_source_layers;
pub mod memory_source_list_collections;
pub mod memory_source_list_flat_map;
pub mod memory_source_list_fold;
pub mod memory_source_list_fold_right;
pub mod memory_source_list_map2;
pub mod memory_source_list_predicates;
pub mod memory_source_list_range;
pub mod memory_source_numeric;
pub mod memory_source_option_map2;
pub mod memory_source_recovery_selector;
pub mod memory_source_result_map2;
pub mod memory_source_scalars;
pub mod memory_source_strings;
pub mod memory_string_predicates;
pub mod memory_structural_dictionaries;
pub mod memory_tasks_refs;
pub mod opaque;
pub mod owner_evidence;
pub mod owner_scheme_preservation;
pub mod partial_handlers;
pub mod props;
pub mod props_errors;
