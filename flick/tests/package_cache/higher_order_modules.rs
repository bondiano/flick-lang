#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn higher_order_templates_run_offline_without_package_sources_on_js_and_go() {
    for emit in ["js", "go"] {
        for profile in ["debug", "release"] {
            let (root, _) = fetched(&format!("hom-package-offline-{emit}-{profile}"));
            let package = root.join(format!("store/{HASH}/lib/src"));
            for (file, source) in [
                (
                    "Geo/Contracts.fl",
                    "module Geo.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  seed : Int\n  equality : Eq.Dict String\n  instance Eq String from equality\ntype module Output =\n  answer : Int\ntype module Factory = (K : Input) -> Output\n",
                ),
                (
                    "Geo/Internal.fl",
                    "private module Geo.Internal exposing (next)\nnext : Int -> Int\nlet next x = x + 1\n",
                ),
                (
                    "Geo/Make.fl",
                    "module Geo.Make(K : Geo.Contracts.Input) : Geo.Contracts.Output\nmodule Built = Geo.Nest.Make(K)\nanswer : Int\nlet answer = Built.answer\n",
                ),
                (
                    "Geo/Nest.fl",
                    "module Geo.Nest exposing (Make)\nimport Geo.Internal as Internal\noutside : Bool\nlet outside = eq \"x\" \"x\"\nstep : Int -> Int\nlet step = Internal.next\nmodule Make(K : Geo.Contracts.Input) : Geo.Contracts.Output =\n  answer : Int\n  let answer = if outside && not (eq \"x\" \"x\") then step K.seed else 0\n",
                ),
                (
                    "Geo/Return.fl",
                    "module Geo.Return(F : Geo.Contracts.Factory) : Geo.Contracts.Factory = module (K : Geo.Contracts.Input) ->\n  module Built = F(K)\n  answer : Int\n  let answer = Built.answer\n",
                ),
                (
                    "Geo/Published.fl",
                    "module Geo.Published exposing (Selected)\nmodule Selected = Geo.Return(Geo.Make)\n",
                ),
            ] {
                let path = package.join(file);
                fs::create_dir_all(path.parent().expect("module directory")).expect("directory");
                fs::write(path, source).expect("package module");
            }
            fs::write(root.join("app/src/App/Instances.fl"), "module App.Instances exposing (Chosen)\nmodule Chosen = Geo.Published.Selected(App.Argument)\n").expect("application");
            fs::write(root.join("app/src/App/Main.fl"), "module App.Main exposing (main)\nimport App.Instances.Chosen as Chosen\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (show Chosen.answer ++ \" \" ++ show (eq \"x\" \"x\"))\n").expect("entry");
            for (stage, seed, expected) in [("warm", 41, "42 true\n"), ("offline", 5, "6 true\n")] {
                if stage == "offline" {
                    fs::remove_dir_all(&package).expect("remove original package sources");
                    assert!(!package.exists());
                }
                fs::write(
                    root.join("app/src/App/Argument.fl"),
                    format!("module App.Argument exposing (seed, equality)\nseed : Int\nlet seed = {seed}\nequality : Eq.Dict String\nlet equality = Eq.dictionary {{ eq = fun _ _ -> false }}\n"),
                )
                .expect("new static argument");
                let (status, stderr) = flick_output(
                    &root,
                    "cache",
                    &[
                        "build",
                        "--offline",
                        "--emit",
                        emit,
                        "--profile",
                        profile,
                        "--out-dir",
                        stage,
                    ],
                );
                assert_eq!(status, Some(0), "{emit}/{profile}/{stage}: {stderr}");
                let dist = root.join("app").join(stage);
                assert_hom_template_interfaces(&root, &dist, emit);
                let output = if emit == "js" {
                    Command::new("node")
                        .arg(dist.join(runner_path(false)))
                        .output()
                } else {
                    Command::new(dist.join(executable(MAIN_ENTRY))).output()
                }
                .expect("program runs");
                assert!(
                    output.status.success(),
                    "{emit}/{profile}/{stage}: {output:?}"
                );
                assert_eq!(
                    output.stdout,
                    expected.as_bytes(),
                    "{emit}/{profile}/{stage}"
                );
                assert!(output.stderr.is_empty(), "{output:?}");
            }
        }
    }
}

fn assert_hom_template_interfaces(root: &Path, dist: &Path, emit: &str) {
    let saved: Vec<Value> = if emit == "go" {
        files(&root.join("cache/flick/packages"))
            .into_iter()
            .flat_map(|(_, bytes)| {
                serde_json::from_slice::<Vec<Value>>(&bytes).expect("saved package modules")
            })
            .collect()
    } else {
        Vec::new()
    };
    for module in ["Geo.Make", "Geo.Published.Selected"] {
        let interface: Value = if emit == "js" {
            let path = dist.join(module.replace('.', "/")).with_extension("iface");
            let interface: Value =
                serde_json::from_slice(&fs::read(path).expect("published interface"))
                    .expect("interface JSON");
            assert_eq!(interface["iface-version"], IFACE_VERSION);
            interface
        } else {
            saved
                .iter()
                .find(|m| m["name"] == module)
                .expect("saved template")["interface"]
                .clone()
        };
        assert!(interface["parameterized module"].is_object());
        assert!(interface["template"]["source"].is_string());
        if module.ends_with("Selected") {
            assert!(
                !interface["template"]["capture"]["bindings"]
                    .as_array()
                    .expect("captured bindings")
                    .is_empty()
            );
            assert_eq!(
                interface["template"]["capture"]["definition_package"],
                "geo"
            );
        }
    }
}
