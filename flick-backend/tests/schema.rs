mod common;

#[test]
fn component_abi_has_a_reviewable_golden() {
    let mut resolve = wit_parser::Resolve::default();
    let (package, _) = resolve
        .push_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/wit"))
        .expect("WIT");
    let mut printer = wit_component::WitPrinter::default();
    printer
        .emit_docs(false)
        .print(&resolve, package, &[])
        .expect("WIT schema");
    insta::assert_snapshot!("backend_abi", printer.output.to_string());
    assert_eq!(
        resolve.packages[package]
            .name
            .version
            .as_ref()
            .expect("version")
            .to_string(),
        flick_backend::VERSION
    );
}

#[test]
fn native_records_have_reviewable_schema_goldens() {
    let mut request = serde_json::to_value(common::request()).expect("request JSON");
    request["program"] = "CBOR portable IR".into();
    insta::assert_snapshot!(
        "backend_request",
        serde_json::to_string_pretty(&request).expect("JSON")
    );
    insta::assert_snapshot!(
        "backend_result",
        serde_json::to_string_pretty(&common::result()).expect("JSON")
    );
}
