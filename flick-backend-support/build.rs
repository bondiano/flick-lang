use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo supplies OUT_DIR"));
    binding_policy(&output)
}

/// Compile package-owned binding metadata; the compiler has no module tables.
fn binding_policy(output: &Path) -> io::Result<()> {
    let data = binding_data()?;
    let mut out = String::new();
    for (key, name) in [
        ("packages", "PACKAGES"),
        ("families", "FAMILIES"),
        ("web_family", "WEB_FAMILY"),
        ("ui_renderer", "UI_RENDERER"),
        ("ui_requirements", "UI_REQUIRED"),
        ("zone_exports", "ZONE_EXPORTS"),
    ] {
        let values: Vec<_> = data[key]
            .as_array()
            .expect("string array")
            .iter()
            .map(|v| v.as_str().expect("string"))
            .collect();
        if name == "ZONE_EXPORTS" {
            out.push_str("#[cfg(feature = \"native-project\")]\n");
        }
        writeln!(out, "pub const {name}: &[&str] = &{values:?};").expect("write string");
    }
    for (key, name) in [
        ("key", "KEY"),
        ("owned_prefix", "OWNED_PREFIX"),
        ("adapter_prefix", "ADAPTER_PREFIX"),
        ("native_entry", "NATIVE_ENTRY"),
        ("adapter_bridge", "ADAPTER_BRIDGE"),
        ("adapter_ui", "ADAPTER_UI"),
        ("zone_module", "ZONE_MODULE"),
    ] {
        if name == "ZONE_MODULE" {
            out.push_str("#[cfg(feature = \"native-project\")]\n");
        }
        writeln!(
            out,
            "pub const {name}: &str = {:?};",
            data[key].as_str().expect("string")
        )
        .expect("write string");
    }
    let aliases: Vec<_> = data["aliases"]
        .as_array()
        .expect("aliases")
        .iter()
        .map(|r| {
            (
                r[0].as_str().expect("from"),
                r[1].as_str().expect("to"),
                r[2].as_bool().expect("prefix"),
            )
        })
        .collect();
    writeln!(
        out,
        "pub const ALIASES: &[(&str, &str, bool)] = &{aliases:?};"
    )
    .expect("write string");
    let routes: Vec<_> = data["routes"]
        .as_array()
        .expect("routes")
        .iter()
        .map(|r| (r[0].as_str().expect("from"), r[1].as_str()))
        .collect();
    writeln!(
        out,
        "pub const ROUTES: &[(&str, Option<&str>)] = &{routes:?};"
    )
    .expect("write string");
    for (path, name) in [
        ("../runtime/rt_exports.txt", "EXPORTS"),
        ("../runtime/rt_stubs.txt", "STUBS"),
        ("../runtime/rt_unavailable.txt", "UNAVAILABLE"),
        ("../packages/effect-ts/rt_bridge_stubs.txt", "BRIDGE_STUBS"),
    ] {
        println!("cargo:rerun-if-changed={path}");
        writeln!(
            out,
            "pub const {name}: &str = {:?};",
            fs::read_to_string(path)?
        )
        .expect("write string");
    }
    binding_go(&data, &mut out);
    fs::write(output.join("bindings.rs"), out)
}

fn binding_data() -> io::Result<serde_json::Value> {
    let mut data = serde_json::Map::new();
    for path in [
        "../packages/effect-ts/compiler-bindings.json",
        "../packages/effect-ts/compiler-families.json",
    ] {
        println!("cargo:rerun-if-changed={path}");
        let part: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&fs::read_to_string(path)?)?;
        for (key, value) in part {
            if data.insert(key.clone(), value).is_some() {
                return Err(io::Error::other(format!("duplicate binding key: {key}")));
            }
        }
    }
    Ok(serde_json::Value::Object(data))
}

fn binding_go(data: &serde_json::Value, out: &mut String) {
    out.push_str("#[cfg(feature = \"native-project\")]\npub const GO_OPTIONAL: &[(&str, &str, &[&str])] = &[\n");
    for row in data["go_optional"].as_array().expect("Go packages") {
        let exports: Vec<_> = row[2]
            .as_array()
            .expect("exports")
            .iter()
            .map(|v| v.as_str().expect("export"))
            .collect();
        writeln!(
            out,
            "({:?}, {:?}, &{exports:?}),",
            row[0].as_str().expect("package"),
            row[1].as_str().expect("module")
        )
        .expect("write string");
    }
    out.push_str("];\n");
}
