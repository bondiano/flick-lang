//! The sources of the built-in modules, in the order `builtins` checks
//! them.

/// The file `$path` of the sources of the package `$package` in
/// `packages/`, as written or, without `builtin-docs`, without its docs.
#[cfg(feature = "builtin-docs")]
macro_rules! builtin_source {
    ($package:literal, $path:literal) => {
        include_str!(concat!("../../../packages/", $package, "/src/", $path))
    };
}

#[cfg(not(feature = "builtin-docs"))]
macro_rules! builtin_source {
    ($package:literal, $path:literal) => {
        include_str!(concat!(env!("OUT_DIR"), "/", $package, "/", $path))
    };
}

/// The core modules, in dependency order: each module sees the ones before
/// it. The compiler relies on them, so they are part of it.
pub(super) const CORE: &[(&str, &str)] = &[
    ("Prelude", builtin_source!("core", "Prelude.fl")),
    ("Instances", builtin_source!("core", "Instances.fl")),
    ("Record", builtin_source!("core", "Record.fl")),
    ("Ref", builtin_source!("core", "Ref.fl")),
    ("String", builtin_source!("core", "String.fl")),
    ("Float", builtin_source!("core", "Float.fl")),
    ("Int", builtin_source!("core", "Int.fl")),
    ("Numeric", builtin_source!("core", "Numeric.fl")),
    ("NonEmpty", builtin_source!("core", "NonEmpty.fl")),
    ("Option", builtin_source!("core", "Option.fl")),
    ("Result", builtin_source!("core", "Result.fl")),
    ("List.Core", builtin_source!("core", "List/Core.fl")),
    ("List.Slice", builtin_source!("core", "List/Slice.fl")),
    ("List.Search", builtin_source!("core", "List/Search.fl")),
    (
        "List.Transform",
        builtin_source!("core", "List/Transform.fl"),
    ),
    ("List", builtin_source!("core", "List.fl")),
    ("Seq", builtin_source!("core", "Seq.fl")),
    ("Monoids", builtin_source!("core", "Monoids.fl")),
    ("Validation", builtin_source!("core", "Validation.fl")),
    ("Flick.Debug", builtin_source!("core", "Flick/Debug.fl")),
];

// `STD`: the modules of `packages/std`, in the order of its `modules.txt`
#[cfg(feature = "embedded-std")]
include!(concat!(env!("OUT_DIR"), "/std_sources.rs"));

// `STD_GO`: the go binding tables of `packages/std/src/go`
#[cfg(feature = "embedded-std")]
include!(concat!(env!("OUT_DIR"), "/std_go.rs"));
