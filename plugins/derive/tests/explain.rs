//! Every code std's derives, `server fn` and `codec` blocks report has its
//! explanation in std's package, as `flick explain std/D001` reads it.

use std::path::Path;

/// The codes of this crate, and the sources that report them.
const CODES: [&str; 10] = [
    "D001", "D002", "D003", "D004", "D005", "D007", "D009", "D010", "D011", "D012",
];

#[test]
fn std_explains_every_code_of_the_derives() {
    let explain = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/std/explain");
    for code in CODES.iter().chain(&["E010"]) {
        let path = explain.join(format!("{code}.md"));
        assert!(path.exists(), "no {}", path.display());
    }
    let sources = [
        include_str!("../src/lib.rs"),
        include_str!("../src/http_api.rs"),
        include_str!("../src/codec.rs"),
        include_str!("../src/server_fn.rs"),
    ]
    .concat();
    for code in CODES.iter().chain(&["E010"]) {
        assert!(
            sources.contains(&format!("\"{code}\"")),
            "{code} is not reported"
        );
    }
}
