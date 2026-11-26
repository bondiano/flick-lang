//! What `flick run`, `dev` and `test` do on the Lua target: a test build
//! runs on a Lua interpreter as LÖVE's; nothing else launches.

use flick_backend::{Backend, BuildRequest, LaunchKind, OptionValue, Toolchain};
use flick_ir::{ExprKind, Lit};

use super::{NoInputs, expr, function, request};
use crate::Lua;

/// The request of `flick <command>` on `platform`, with `tools` on PATH.
fn command(platform: &str, tags: &[&str], command: &str, tools: &[(&str, &str)]) -> BuildRequest {
    let mut clean = request(
        platform,
        tags,
        vec![function("main", expr(ExprKind::Lit(Lit::Unit), 0, 2))],
    );
    clean.target.options.push(OptionValue {
        key: "command".into(),
        value: command.into(),
    });
    clean.toolchains = (tools.iter())
        .map(|&(id, version)| Toolchain {
            id: id.into(),
            version: version.into(),
            sha256: "0".repeat(64),
        })
        .collect();
    clean
}

fn refused(request: &BuildRequest) -> String {
    let refused = Lua.generate(request, &mut NoInputs).expect_err("refused");
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].code, "P012");
    refused[0].message.clone()
}

#[test]
fn a_command_that_would_run_the_program_is_refused() {
    let lua = [("lua", "5.1.5")];
    for name in ["run", "dev"] {
        for platform in ["wow-retail", "love"] {
            let message = refused(&command(platform, &["lua", "lua51"], name, &lua));
            let expected = format!(
                "`flick {name}` is not available on the lua/{platform} target yet: a Lua program \
                 runs in its host, World of Warcraft or LÖVE"
            );
            assert!(message.contains(&expected), "{message}");
        }
    }
    let message = refused(&command("wow-forever", &["lua", "lua51"], "test", &lua));
    assert!(
        message.contains("no World of Warcraft client runs them here; test on `lua/love`"),
        "{message}"
    );
}

#[test]
fn a_test_build_runs_on_the_interpreter_its_dialect_allows() {
    let lua51 = ["lua", "lua51"];
    let luajit = ["lua", "luajit"];
    let launched = |request: &BuildRequest| {
        let built = Lua.generate(request, &mut NoInputs).expect("generated");
        let paths: Vec<String> = built.artifacts.iter().map(|a| a.path.clone()).collect();
        assert!(
            paths.contains(&"_flick/lua-kernel/runner.lua".to_owned()),
            "{paths:?}"
        );
        assert!(
            paths.contains(&"_flick/run-tests.lua".to_owned()),
            "{paths:?}"
        );
        assert_eq!(built.launches.len(), 1);
        let launch = &built.launches[0];
        assert_eq!(
            (launch.name.as_str(), launch.kind),
            ("test", LaunchKind::Test)
        );
        assert_eq!(launch.artifact, "lua:_flick/run-tests.lua");
        launch.runtime.as_ref().expect("an interpreter").id.clone()
    };
    let both = [("lua", "5.1.5"), ("luajit", "2.1.1785763465")];
    assert_eq!(launched(&command("love", &lua51, "test", &both)), "luajit");
    assert_eq!(
        launched(&command("love", &lua51, "test", &both[..1])),
        "lua"
    );
    assert_eq!(launched(&command("love", &luajit, "test", &both)), "luajit");
    let needs = refused(&command("love", &luajit, "test", &both[..1]));
    assert!(
        needs.contains("with the dialect `luajit` needs `luajit` on PATH"),
        "{needs}"
    );
    let none = refused(&command("love", &lua51, "test", &[]));
    assert!(none.contains("needs `luajit` or `lua` on PATH"), "{none}");
    let newer = refused(&command("love", &lua51, "test", &[("lua", "5.4.7")]));
    assert!(
        newer.contains("needs `lua` 5.1.x, the version its profile pins, not 5.4.7"),
        "{newer}"
    );
    // a plain build has no launch and no runner
    let built = Lua
        .generate(&command("love", &lua51, "build", &both), &mut NoInputs)
        .expect("generated");
    assert!(built.launches.is_empty());
    assert!(
        built
            .artifacts
            .iter()
            .all(|a| a.path != "_flick/run-tests.lua")
    );
}
