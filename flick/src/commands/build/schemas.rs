//! The compiled decoders of an app's codecs, which Node writes.

use crate::backends::Selected;
use std::path::Path;

/// Loads the app's modules on Node and writes the compiled decoders of the
/// codecs they make, which the runner installs.
pub(super) fn compile_schemas(
    out_dir: &Path,
    module: &Path,
    file: &str,
    backend: &Selected,
) -> Result<(), String> {
    let script = "import { writeSchemaAot } from \"@flick-lang/effect-ts\";\nawait writeSchemaAot(process.argv[1], process.argv[2]);\n";
    let output = backend
        .schema_aot_command(out_dir)?
        .args(["--input-type=module", "-e", script])
        .arg(module)
        .arg(out_dir.join(file))
        .current_dir(out_dir)
        .output()
        .map_err(|err| format!("cannot run node: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}
