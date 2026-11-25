//! Projection summaries: which lambdas are projections, and the summary
//! surviving both wire forms beside the module's other exports.

use std::collections::BTreeMap;

use super::{PROJECTION_DEPTH, PROJECTION_VERSION, Projection, projection_path};
use crate::{
    Exported, Exports, Expr, ExprKind, FileId, Initialization, IrType, Module, Param, Pat, Program,
    Span, Stage, TypeRef,
};

fn span() -> Span {
    Span::new(FileId::new(0), 0, 1)
}

fn named(name: &str) -> TypeRef {
    TypeRef::new(IrType::Named {
        module: "App".to_owned(),
        name: name.to_owned(),
        args: Vec::new(),
    })
}

/// `fun r -> r.l₁.….lₙ` for `labels`, the parameter of `stage`.
fn reads(labels: &[&str], stage: Stage) -> Expr {
    let mut body = Expr::new(ExprKind::Local("r".to_owned()), span());
    for label in labels {
        body = Expr::new(ExprKind::Field(Box::new(body), (*label).to_owned()), span());
    }
    body.ty = Some(named("String"));
    let param = Param {
        pat: Pat::Bind("r".to_owned()),
        stage,
        ty: Some(named("User")),
        binder_types: Vec::new(),
    };
    Expr::new(
        ExprKind::Lambda {
            params: vec![param],
            body: Box::new(body),
        },
        span(),
    )
}

fn path_of(lambda: &Expr) -> Option<Vec<String>> {
    let ExprKind::Lambda { params, body } = &lambda.kind else {
        unreachable!("a lambda")
    };
    projection_path(params, body)
}

#[test]
fn a_projection_reads_a_bounded_path_of_its_one_parameter() {
    assert_eq!(
        path_of(&reads(&["address", "city"], Stage::Pure)),
        Some(vec!["address".to_owned(), "city".to_owned()])
    );
    let deepest: Vec<&str> = ["a"; PROJECTION_DEPTH].to_vec();
    assert!(path_of(&reads(&deepest, Stage::Pure)).is_some());
    let deeper: Vec<&str> = ["a"; PROJECTION_DEPTH + 1].to_vec();
    assert_eq!(path_of(&reads(&deeper, Stage::Pure)), None);
    // the parameter alone reads nothing, and a runtime arrow is no projection
    assert_eq!(path_of(&reads(&[], Stage::Pure)), None);
    assert_eq!(path_of(&reads(&["a"], Stage::Runtime)), None);
}

#[test]
fn a_summary_carries_the_types_and_a_reader_takes_only_its_version() {
    let summary = Projection::of(&["a".to_owned()], &reads(&["address", "city"], Stage::Pure))
        .expect("a projection");
    assert_eq!(summary.version, PROJECTION_VERSION);
    assert_eq!(summary.type_params, ["a"]);
    assert_eq!(summary.param, Some(named("User")));
    assert_eq!(summary.result, Some(named("String")));
    assert!(summary.readable());
    let later = Projection {
        version: PROJECTION_VERSION + 1,
        ..summary.clone()
    };
    assert!(!later.readable());
    let literal = Expr::new(ExprKind::Local("r".to_owned()), span());
    assert_eq!(Projection::of(&[], &literal), None);
}

fn module(exports: Exports) -> Module {
    Module {
        name: "App".to_owned(),
        items: Vec::new(),
        types: Vec::new(),
        preambles: Vec::new(),
        labels: BTreeMap::default(),
        initialization: Initialization::default(),
        records: BTreeMap::default(),
        package: None,
        exports,
    }
}

/// The types of a summary go into the program's type table, and a module
/// written without summaries reads as one without them.
#[test]
fn a_summary_survives_both_forms_and_its_absence_reads_as_none() {
    let summary = Projection::of(&[], &reads(&["name"], Stage::Pure)).expect("a projection");
    let exports = Exports {
        values: vec![
            Exported {
                name: "name_of".to_owned(),
                sync: None,
                projection: Some(summary),
            },
            Exported {
                name: "plain".to_owned(),
                sync: None,
                projection: None,
            },
        ],
        reexports: Vec::new(),
        host: Vec::new(),
    };
    let program = Program::new("flick test", vec![module(exports)], Vec::new(), &|_| {
        "file.fl".to_owned()
    });
    let json = program.to_json();
    assert_eq!(Program::from_json(&json), Ok(program.clone()));
    #[cfg(feature = "cbor")]
    assert_eq!(Program::from_cbor(&program.to_cbor()), Ok(program.clone()));
    let written: serde_json::Value = serde_json::from_str(&json).expect("JSON");
    assert_eq!(
        written["modules"][0]["exports"]["values"][1],
        serde_json::json!({ "name": "plain", "sync": null })
    );
    let old = serde_json::json!({ "name": "plain", "sync": null });
    let read: Exported = serde_json::from_value(old).expect("an export without a summary");
    assert_eq!(read.projection, None);
}
