//! The sources of the built-in modules, in the order `builtins` checks
//! them.

/// The file `$path` of the sources of the package `$package` in
/// `packages/`, as written or, without `builtin-docs`, without its docs.
#[cfg(feature = "native-project")]
macro_rules! builtin_source {
    ($package:literal, $path:literal) => {
        include_str!(concat!("../../../packages/", $package, "/src/", $path))
    };
}

#[cfg(not(feature = "native-project"))]
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
    ("Enum", builtin_source!("core", "Enum.fl")),
    ("TypeEq", builtin_source!("core", "TypeEq.fl")),
    ("Ref", builtin_source!("core", "Ref.fl")),
    ("String", builtin_source!("core", "String.fl")),
    ("Float", builtin_source!("core", "Float.fl")),
    ("Int", builtin_source!("core", "Int.fl")),
    ("Numeric", builtin_source!("core", "Numeric.fl")),
    ("NonEmpty", builtin_source!("core", "NonEmpty.fl")),
    ("Option", builtin_source!("core", "Option.fl")),
    ("Result", builtin_source!("core", "Result.fl")),
    ("Fail", builtin_source!("core", "Fail.fl")),
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
    ("CallSite", builtin_source!("core", "CallSite.fl")),
    ("Flick.Debug", builtin_source!("core", "Flick/Debug.fl")),
];

/// An implicit package in the binary: its name, its modules in the order of
/// its `modules.txt`, and the go binding tables of its `src/go`.
#[cfg(feature = "embedded-std")]
pub(super) type Embedded = (
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static [(&'static str, &'static str)],
);

// `IMPLICIT`: each implicit package
#[cfg(feature = "embedded-std")]
include!(concat!(env!("OUT_DIR"), "/implicit_sources.rs"));
