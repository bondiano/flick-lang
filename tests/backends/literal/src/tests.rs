//! A small, independently implemented pure-test subset. Unsupported
//! expressions are refused; assertions execute in the real test runner.

use std::fmt::Write;

use flick_backend::{
    Artifact, ArtifactKind, BuildRequest, Launch, LaunchKind, ModuleRole, RuntimeDependency,
};
use flick_ir::{Expr, ExprKind, Item, ItemKind, Lit, Program, Rep, Stage};

fn quote(text: &str) -> String {
    serde_json::to_string(text).expect("JS string")
}

fn expression(expr: &Expr) -> Option<String> {
    Some(match &expr.kind {
        ExprKind::Lit(Lit::String(text)) => quote(text),
        ExprKind::Global { module, name } if module == "Test" && name == "test" => {
            "((name) => (body) => testPure(name, body))".into()
        }
        ExprKind::Con(c)
            if c.module == "Test"
                && c.ty == "Assertion"
                && c.rep == Rep::Tagged
                && c.dicts == 0 =>
        {
            let tag = format!(
                "tag({})",
                quote(&format!("{}.{}.{}", c.module, c.ty, c.name))
            );
            match (c.name.as_str(), c.arity) {
                ("Pass", 0) => format!("({{ _tag: {tag} }})"),
                ("Failed", 1) => format!("((message) => ({{ _tag: {tag}, _0: message }}))"),
                _ => return None,
            }
        }
        ExprKind::App {
            func,
            arg,
            stage: Stage::Pure,
            ..
        } => format!("({})({})", expression(func)?, expression(arg)?),
        ExprKind::Lambda { params, body }
            if params.len() == 1 && params[0].stage == Stage::Pure =>
        {
            format!("((_) => {})", expression(body)?)
        }
        ExprKind::List(items) => format!(
            "[{}]",
            items
                .iter()
                .map(expression)
                .collect::<Option<Vec<_>>>()?
                .join(", ")
        ),
        _ => return None,
    })
}

pub(super) fn supported(item: &Item, request: &BuildRequest) -> bool {
    let external_test = request
        .modules
        .iter()
        .any(|m| m.name == "Test" && m.role == ModuleRole::External && m.package.is_none());
    external_test
        && request
            .runtime_dependencies
            .iter()
            .any(|r| r.package == "@flick-lang/runtime" && r.version == "0.1.0")
        && matches!(&item.kind, ItemKind::Value { name, dicts, body } if name == "tests" && dicts.is_empty() && expression(body).is_some())
}

pub(super) fn source(request: &BuildRequest, module: &str) -> Option<Vec<u8>> {
    let program = Program::from_cbor(&request.program).ok()?;
    let module = program.modules.iter().find(|m| m.name == module)?;
    let item = module.items.first()?;
    if !supported(item, request) {
        return None;
    }
    let ItemKind::Value { body, .. } = &item.kind else {
        return None;
    };
    Some(format!("import {{ testPure }} from \"@flick-lang/runtime/rt\";\nimport {{ tag }} from \"@flick-lang/runtime/tags\";\nexport const tests = {};\n", expression(body)?).into_bytes())
}

pub(super) fn runner(
    request: &BuildRequest,
    artifacts: &mut Vec<Artifact>,
    launches: &mut Vec<Launch>,
) -> Vec<RuntimeDependency> {
    let Some(node) = request.toolchains.iter().find(|t| t.id == "node") else {
        return vec![];
    };
    artifacts.push(Artifact {
        id: "package".into(),
        path: "package.json".into(),
        kind: ArtifactKind::Metadata,
        bytes: b"{\"type\":\"module\"}\n".to_vec(),
    });
    let test = request
        .target
        .options
        .iter()
        .any(|o| o.key == "command" && o.value == "test");
    if !test {
        return vec![];
    }
    let Some(runtime) = request
        .runtime_dependencies
        .iter()
        .find(|r| r.package == "@flick-lang/runtime")
    else {
        return vec![];
    };
    let mut source = "import { runTests } from \"@flick-lang/runtime/rt\";\n".to_owned();
    let mut suites = vec![];
    for (i, root) in request.test_roots.iter().enumerate() {
        let module = request
            .modules
            .iter()
            .find(|m| m.name == root.module)
            .expect("test root module");
        let _ = writeln!(
            source,
            "import {{ tests as suite{i} }} from {};",
            quote(&format!("./{}", module.output))
        );
        suites.push(format!("[{}, suite{i}]", quote(&root.module)));
    }
    let _ = writeln!(source, "await runTests([{}]);", suites.join(", "));
    artifacts.push(Artifact {
        id: "test-launch".into(),
        path: "invoke-tests.mjs".into(),
        kind: ArtifactKind::Source,
        bytes: source.into_bytes(),
    });
    launches.push(Launch {
        name: "test".into(),
        kind: LaunchKind::Test,
        artifact: "test-launch".into(),
        runtime: Some(node.clone()),
        arguments: vec![],
    });
    vec![runtime.clone()]
}
