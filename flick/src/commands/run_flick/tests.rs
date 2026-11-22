use std::path::Path;

use super::extract_line_from_stderr;

#[test]
fn extract_line_requires_boundary_before_filename_suffix() {
    let target = Path::new("/proj/src/Lib/Api.fl");
    assert_eq!(
        extract_line_from_stderr("    at fn (file:///proj/src/Lib/OtherApi.fl:9:2)", target),
        None
    );
    assert_eq!(
        extract_line_from_stderr("    at fn (file:///proj/src/Lib/Api.fl:12:4)", target),
        Some(12)
    );
    assert_eq!(extract_line_from_stderr("(Api.fl:7:1)", target), Some(7));
}

#[cfg(feature = "wasm-backends")]
#[test]
fn load_origin_span_rejects_non_utf8_and_mid_character_spans() {
    use flick_backend::command::{File, Invocation, Span as CommandSpan};
    use flick_core::source::SourceMap;

    use super::load_origin_span;

    let inv = Invocation {
        name: "cmd".into(),
        arguments: vec![],
        sources: "src".into(),
        inputs: vec![
            File {
                path: "src/Utf8.fl".into(),
                bytes: "// café\n".as_bytes().to_vec(),
                executable: false,
            },
            File {
                path: "src/Bin.fl".into(),
                bytes: vec![0xff, 0xfe, b'\n'],
                executable: false,
            },
        ],
        tools: vec![],
    };
    let mut sources = SourceMap::default();
    // `é` occupies bytes 6..8 in "// café\n"
    assert!(
        load_origin_span(
            &mut sources,
            &inv,
            Some(&CommandSpan {
                path: "src/Utf8.fl".into(),
                start: 0,
                end: 7,
            }),
        )
        .is_none()
    );
    assert!(
        load_origin_span(
            &mut sources,
            &inv,
            Some(&CommandSpan {
                path: "src/Bin.fl".into(),
                start: 0,
                end: 2,
            }),
        )
        .is_none()
    );
    let ok = load_origin_span(
        &mut sources,
        &inv,
        Some(&CommandSpan {
            path: "src/Utf8.fl".into(),
            start: 3,
            end: 8,
        }),
    )
    .expect("valid char boundary span");
    assert_eq!(&sources.get(ok.file).text[ok.range()], "café");
}
