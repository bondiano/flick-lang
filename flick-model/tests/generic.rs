//! `deriving Generic` on random shapes of types, run by the model: each
//! value comes back from its representation as it was, and `GCount`,
//! written once over the representation, counts the fields of its
//! constructor. Not part of `just test`: `just model` runs it, and
//! `PROPTEST_CASES` sets how many types it sees.

use std::fmt::Write as _;
use std::{env, fs, process};

use flick_model::eval::Outcome;
use flick_model::run;
use proptest::collection::{btree_set, vec};
use proptest::prelude::*;

const FUEL: u64 = 5_000_000;

/// The classes and instances every program uses.
const HEAD: &str = "module App.Main exposing (main)

import Generic exposing (Generic(..), Sum(..), Con(..), Field(..))

class GCount rep =
  gcount : rep -> Int

instance GCount Unit =
  let gcount _ = 0

instance GCount rest => GCount (Field l a, rest) =
  let gcount (_, rest) = 1 + gcount rest

instance GCount fields => GCount (Con n fields) =
  let gcount (Con fields) = gcount fields

instance (GCount a, GCount b) => GCount (Sum a b) =
  let gcount s =
    match s with
    | Inl a -> gcount a
    | Inr b -> gcount b

count : (Generic a rep, GCount rep) => a -> Int
let count x = gcount (to_rep x)
";

/// The type of a field and a value of it.
const FIELDS: [(&str, &str); 3] = [("Float", "1.5"), ("String", "\"s\""), ("Bool", "true")];

/// A constructor: its fields by position, or the labels of one record.
#[derive(Debug, Clone)]
enum Case {
    Positional(Vec<usize>),
    Record(Vec<(String, usize)>),
}

impl Case {
    fn arity(&self) -> usize {
        match self {
            Case::Positional(fields) => fields.len(),
            Case::Record(fields) => fields.len(),
        }
    }
}

fn case() -> impl Strategy<Value = Case> {
    let field = 0..FIELDS.len();
    let labels = btree_set(prop::sample::select(vec!["a", "b", "label", "width"]), 1..4);
    prop_oneof![
        vec(field.clone(), 0..4).prop_map(Case::Positional),
        (labels, vec(field, 4)).prop_map(|(labels, tys)| {
            Case::Record(
                labels
                    .into_iter()
                    .zip(tys)
                    .map(|(l, t)| (l.to_owned(), t))
                    .collect(),
            )
        }),
    ]
}

/// The program over a type of `cases`, and what it prints.
fn program(cases: &[Case]) -> (String, String) {
    let mut text = format!("{HEAD}\ntype T\n");
    let mut values = Vec::new();
    for (i, c) in cases.iter().enumerate() {
        let bar = if i == 0 { "=" } else { "|" };
        match c {
            Case::Positional(fields) => {
                let tys: Vec<&str> = fields.iter().map(|&f| FIELDS[f].0).collect();
                let _ = writeln!(text, "  {bar} C{i} {}", tys.join(" "));
                let vals: Vec<&str> = fields.iter().map(|&f| FIELDS[f].1).collect();
                values.push(format!("C{i} {}", vals.join(" ")));
            }
            Case::Record(fields) => {
                let tys: Vec<String> = (fields.iter())
                    .map(|(l, f)| format!("{l} : {}", FIELDS[*f].0))
                    .collect();
                let _ = writeln!(text, "  {bar} C{i} {{ {} }}", tys.join("; "));
                let vals: Vec<String> = (fields.iter())
                    .map(|(l, f)| format!("{l} = {}", FIELDS[*f].1))
                    .collect();
                values.push(format!("C{i} {{ {} }}", vals.join("; ")));
            }
        }
    }
    text.push_str("  deriving Generic, Eq\n\n");
    let _ = write!(
        text,
        "main : Unit ->{{; AppError}} Unit\nlet main () =\n  let values : List T = [{}]\n  Platform.stdout_line (show (List.map (fun v -> ((from_rep (to_rep v) : T) == v, count v)) values))\n",
        values.join(", ")
    );
    let expected: Vec<String> = cases
        .iter()
        .map(|c| format!("(true, {})", c.arity()))
        .collect();
    (text, format!("[{}]\n", expected.join(", ")))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]

    #[test]
    #[ignore = "runs the model on random types; run `just model`"]
    fn a_value_comes_back_from_its_representation(cases in vec(case(), 1..4)) {
        let (text, expected) = program(&cases);
        let file = env::temp_dir().join(format!("flick-model-generic-{}.fl", process::id()));
        fs::write(&file, &text).expect("write the program");
        let ran = run::run_file(&file, FUEL);
        let ran = ran.unwrap_or_else(|e| panic!("{e}\n{text}"));
        prop_assert!(
            matches!(ran.outcome, Outcome::Value(_)),
            "{:?}\n{text}",
            ran.outcome
        );
        prop_assert_eq!(ran.stdout, expected, "{}", text);
    }
}
