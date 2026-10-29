//! The Go runtime, written beside every program at `flickrt/`.

/// The runtime's files: name and contents.
pub const RUNTIME: &[(&str, &str)] = &[
    (
        "data.go",
        include_str!("../../../../runtime-go/flickrt/data.go"),
    ),
    (
        "dict.go",
        include_str!("../../../../runtime-go/flickrt/dict.go"),
    ),
    (
        "list.go",
        include_str!("../../../../runtime-go/flickrt/list.go"),
    ),
    (
        "number.go",
        include_str!("../../../../runtime-go/flickrt/number.go"),
    ),
    (
        "numbers.go",
        include_str!("../../../../runtime-go/flickrt/numbers.go"),
    ),
    (
        "platform.go",
        include_str!("../../../../runtime-go/flickrt/platform.go"),
    ),
    (
        "string.go",
        include_str!("../../../../runtime-go/flickrt/string.go"),
    ),
    (
        "value.go",
        include_str!("../../../../runtime-go/flickrt/value.go"),
    ),
];
