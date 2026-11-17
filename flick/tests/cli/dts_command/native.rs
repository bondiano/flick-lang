use std::fs;
use std::path::Path;
use std::process::Command;

use super::common::flick_in;

#[test]
#[ignore = "needs Node and pnpm install"]
fn native_service_declarations_pass_tsc_without_effect_types() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("dts_native_service");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/Lib")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n[project]\nkind = \"lib\"\n",
    )
    .expect("config");
    fs::write(
        dir.join("package.json"),
        "{\"type\":\"module\",\"dependencies\":{\"@flick-lang/runtime\":\"workspace:*\"}}\n",
    )
    .expect("package");
    fs::write(
        dir.join("src/Lib/Service.fl"),
        "module Lib.Service exposing (Greeter)\nservice Greeter =\n  hello : String ->{} String\n",
    )
    .expect("source");
    let built = flick_in(&dir, &["build", "--emit-dts"]);
    assert!(built.status.success(), "{built:?}");
    let declaration = fs::read_to_string(dir.join("dist/Lib/Service.d.ts")).expect("declaration");
    assert!(!declaration.contains("$Context"), "{declaration}");
    assert!(
        !declaration.contains("@flick-lang/effect-ts"),
        "{declaration}"
    );
    fs::write(dir.join("consumer.ts"),
        "import { $s_Greeter } from './dist/Lib/Service.js';\n\
         import type { Greeter } from './dist/Lib/Service.js';\n\
         import type { ServiceValue, EffectValue } from '@flick-lang/runtime/rt/types';\n\
         const key: string = $s_Greeter.key;\n\
         const tag: ServiceValue<Greeter, { readonly hello: (name: string) => EffectValue<string, never, never> }> = $s_Greeter;\n\
         // @ts-expect-error the implementation cannot return a number\n\
         const wrong: ServiceValue<Greeter, { readonly hello: (name: string) => number }> = $s_Greeter;\n\
         export { key, tag, wrong };\n",
    ).expect("consumer");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    fs::write(
        dir.join("tsconfig.json"),
        serde_json::json!({
            "compilerOptions": {
                "strict": true, "noEmit": true, "module": "nodenext",
                "moduleResolution": "nodenext", "target": "es2022",
                "lib": ["es2022", "dom", "esnext.disposable"], "types": ["node"],
                "typeRoots": [repo.join("runtime/node_modules/@types")],
            },
            "files": ["consumer.ts"],
        })
        .to_string(),
    )
    .expect("tsconfig");
    let checked = Command::new(repo.join("node_modules/.bin/tsc"))
        .arg("-p")
        .arg(&dir)
        .output()
        .expect("tsc");
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stdout)
    );
}
