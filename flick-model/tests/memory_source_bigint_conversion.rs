//! Exact global `BigInt` conversion through genuine checked source domains.
#[path = "support/causes.rs"]
pub mod causes;
use std::{fmt::Write as _, fs, path::Path};

use flick_core::{
    build,
    ir::{
        ItemKind, Lit,
        memory::{Name, Program},
    },
    opt::perceus,
};
use flick_model::{
    memory::{Error, Heap, ProgramOutcome, Value, execute_program_outcome},
    run,
};

const FUEL: u64 = 100_000;
const SOURCE: &str = r#"module App.BigIntConversion exposing (int_value, bigint_value, bool_value, float_value, text_value, unit_value, bad_result, unknown, ref_value, js_value, list_value, all, cold)
int_value : Int -> BigInt
foreign pure int_value from global "BigInt"
bigint_value : BigInt -> BigInt
foreign pure bigint_value from global "BigInt"
bool_value : Bool -> BigInt
foreign pure bool_value from global "BigInt"
float_value : Float -> BigInt
foreign pure float_value from global "BigInt"
text_value : String -> BigInt
foreign pure text_value from global "BigInt"
unit_value : Unit -> BigInt
foreign pure unit_value from global "BigInt"
bad_result : Int -> String
foreign pure bad_result from global "BigInt"
unknown : a -> BigInt
foreign pure unknown from global "BigInt"
ref_value : Ref Int -> BigInt
foreign pure ref_value from global "BigInt"
js_value : JsValue -> BigInt
foreign pure js_value from global "BigInt"
list_value : List Int -> BigInt
foreign pure list_value from global "BigInt"
all : Unit -> String
let all () =
  show (int_value (-2147483647 - 1)) ++ "|" ++ show (bigint_value 0x10000000000000000n)
    ++ "|" ++ show (bool_value true) ++ "|" ++ show (bool_value false)
    ++ "|" ++ show (float_value (-0.0)) ++ "|" ++ show (float_value 1e20)
    ++ "|" ++ show (text_value "\u{feff}+0012\u{feff}") ++ "|" ++ show (text_value "0Xff")
cold : String -> String
let cold seed =
  let _unused = fun () -> if seed == "" then unit_value () else float_value 0.5
  seed
"#;
const ALL: &str = "-2147483648n|18446744073709551616n|1n|0n|0n|100000000000000000000n|12n|255n";

fn programs(source: &str, name: &str) -> (Name, [Program; 2]) {
    let modules = run::compile_source(source).expect("genuine checked global BigInt declarations");
    let entry = Name::new("App.BigIntConversion", name);
    let programs = [false, true].map(|release| {
        let ir: Vec<_> = modules
            .iter()
            .map(|module| {
                if release {
                    build::optimized(&modules, module, false)
                } else {
                    module.ir.clone()
                }
            })
            .collect();
        perceus::lower_with_declarations(
            ir.iter().chain(build::builtin_modules()),
            modules
                .iter()
                .map(|module| module.declarations.as_ref())
                .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
            &entry,
        )
        .expect("checked primitive conversion lowers")
    });
    (entry, programs)
}

fn source_case(
    source: &str,
    name: &str,
    input: &Lit,
    expected: &Result<&str, Error>,
    output_string: bool,
    prefixes: bool,
) -> usize {
    let (entry, programs) = programs(source, name);
    let mut count = 0;
    for (profile, program) in programs.into_iter().enumerate() {
        let mut completed = false;
        for fuel in if prefixes { 0..=FUEL } else { FUEL..=FUEL } {
            let mut heap = Heap::new();
            let external = heap.literal(input).expect("external input");
            let argument = heap.dup(&external).expect("owned argument");
            let report = execute_program_outcome(&mut heap, &program, &entry, vec![argument], fuel);
            match report.outcome {
                ProgramOutcome::Returned(value) => {
                    let text = if output_string {
                        heap.string(&value)
                    } else {
                        heap.bigint(&value)
                    };
                    assert_eq!(&Ok(text.expect("expected primitive result")), expected);
                    heap.drop_value(value).expect("owned result release");
                    completed = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) if prefixes => {}
                ProgramOutcome::Failed(cause) => {
                    assert_eq!(&Err(causes::defect_error(&mut heap, cause)), expected);
                    completed = true;
                }
                outcome => {
                    let diagnostic = format!("{name} profile={profile} fuel={fuel}: {outcome:?}");
                    causes::unexpected(&mut heap, outcome, &diagnostic)
                }
            }
            assert_eq!(report.stdout, "");
            if let Lit::String(text) = input {
                assert_eq!(heap.string(&external), Ok(text.as_str()));
            }
            if let Lit::BigInt(text) = input {
                assert_eq!(heap.bigint(&external), Ok(text.as_str()));
            }
            assert_eq!(
                heap.reference_count(&external),
                Ok(usize::from(external.object_id().is_some()))
            );
            heap.drop_value(external).expect("external input release");
            let stats = heap.finish().expect("actual zero occupied source slots");
            assert_eq!(stats.allocations, stats.frees);
            count += 1;
            if completed {
                if prefixes {
                    eprintln!("{name} profile={profile} first terminal fuel={fuel}");
                }
                break;
            }
        }
        assert!(
            completed,
            "ordinary unchanged cap reaches the source outcome"
        );
    }
    count
}

#[test]
fn numeric_classes_corpus_executes_in_both_profiles_after_checked_bigint_conversion() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run/codegen/numeric_classes.fl");
    let expected = fs::read_to_string(path.with_extension("stdout")).expect("corpus oracle");
    for release in [false, true] {
        let mut heap = Heap::new();
        let report = run::run_file_counted_outcome(&mut heap, &path, 1_000_000, release)
            .expect("checked numeric corpus lowers completely");
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(value, Value::Unit);
        assert_eq!(report.stdout, expected);
        heap.drop_value(value).expect("caller releases outcome");
        let stats = heap.finish().expect("actual zero occupied corpus slots");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn all_known_primitive_domains_and_cold_faulting_conversions_execute_in_both_profiles() {
    source_case(SOURCE, "all", &Lit::Unit, &Ok(ALL), true, false);
    source_case(
        SOURCE,
        "cold",
        &Lit::String("outside".into()),
        &Ok("outside"),
        true,
        false,
    );
    for (name, input, expected) in [
        ("int_value", Lit::Int(i32::MAX), Ok("2147483647")),
        (
            "bigint_value",
            Lit::BigInt("0X10000000000000000".into()),
            Ok("0X10000000000000000"),
        ),
        ("bool_value", Lit::Bool(false), Ok("0")),
        (
            "float_value",
            Lit::Float(0.5),
            Err(Error::BigIntNonIntegralNumber),
        ),
        (
            "text_value",
            Lit::String("+-1".into()),
            Err(Error::BigIntInvalidString),
        ),
        ("unit_value", Lit::Unit, Err(Error::BigIntUndefined)),
    ] {
        source_case(SOURCE, name, &input, &expected, false, false);
    }
}

#[test]
fn every_prefix_through_multidomain_results_cold_closures_and_specific_faults_releases_roots() {
    let mut count = 0;
    for (name, input, expected, string_result) in [
        ("all", Lit::Unit, Ok(ALL), true),
        ("cold", Lit::String("outside".into()), Ok("outside"), true),
        (
            "float_value",
            Lit::Float(0.5),
            Err(Error::BigIntNonIntegralNumber),
            false,
        ),
        (
            "text_value",
            Lit::String("+-1".into()),
            Err(Error::BigIntInvalidString),
            false,
        ),
        ("unit_value", Lit::Unit, Err(Error::BigIntUndefined), false),
    ] {
        count += source_case(SOURCE, name, &input, &expected, string_result, true);
    }
    eprintln!("checked {count} consecutive source BigInt-conversion prefixes");
}

#[test]
fn identical_abi_does_not_prove_wrong_results_unknown_inputs_or_missing_declarations() {
    let modules =
        run::compile_source(SOURCE).expect("all foreign source signatures remain accepted");
    let ir: Vec<_> = modules.iter().map(|module| &module.ir).collect();
    for name in [
        "bad_result",
        "unknown",
        "ref_value",
        "js_value",
        "list_value",
    ] {
        let entry = Name::new("App.BigIntConversion", name);
        let error = perceus::lower_with_declarations(
            ir.iter().copied().chain(build::builtin_modules()),
            modules
                .iter()
                .map(|module| module.declarations.as_ref())
                .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
            &entry,
        )
        .expect_err("ABI cannot manufacture a checked primitive/BigInt proof");
        assert!(
            error
                .to_string()
                .contains("unsupported checked BigInt domain"),
            "{name}: {error}"
        );
    }
    let entry = Name::new("App.BigIntConversion", "int_value");
    assert!(
        perceus::lower(ir.iter().copied().chain(build::builtin_modules()), &entry)
            .expect_err("no-evidence entrypoint rejects")
            .to_string()
            .contains("requires a checked declaration context")
    );
    assert!(
        perceus::lower_with_declarations(
            ir.iter().copied().chain(build::builtin_modules()),
            build::builtin_declarations().iter().map(AsRef::as_ref),
            &entry
        )
        .expect_err("missing exact source scheme rejects")
        .to_string()
        .contains("missing checked declaration scheme")
    );
}

#[test]
fn deep_opaque_aliases_discharge_both_copy_bits_after_both_domain_proofs() {
    let mut source = "module App.BigIntConversion exposing (convert)\n".to_owned();
    for (prefix, base) in [("Input", "Int"), ("Output", "BigInt")] {
        for depth in 0..200 {
            let body = if depth == 0 {
                base.to_owned()
            } else {
                format!("{prefix}{}", depth - 1)
            };
            writeln!(source, "opaque type alias {prefix}{depth} = {body}")
                .expect("writing into a String cannot fail");
        }
    }
    source
        .push_str("convert : Input199 -> Output199\nforeign pure convert from global \"BigInt\"\n");
    let modules = run::compile_source(&source).expect("real deep opaque declarations");
    let foreign = modules
        .iter()
        .flat_map(|module| &module.ir.items)
        .find_map(|item| {
            if let ItemKind::Foreign(value) = &item.kind {
                Some(value)
            } else {
                None
            }
        })
        .expect("actual foreign");
    assert_eq!(foreign.adapter.copies.args, [true]);
    assert!(foreign.adapter.copies.result);
    source_case(
        &source,
        "convert",
        &Lit::Int(i32::MIN),
        &Ok("-2147483648"),
        false,
        false,
    );
}

#[test]
fn only_explicit_source_name_provenance_restores_a_generated_foreign_identity() {
    let modules = run::compile_source(SOURCE).expect("checked source identity");
    for source_name in [Some("int_value"), None, Some("missing")] {
        let mut ir: Vec<_> = modules.iter().map(|module| module.ir.clone()).collect();
        let foreign = ir
            .iter_mut()
            .flat_map(|module| &mut module.items)
            .find_map(|item| match &mut item.kind {
                ItemKind::Foreign(value) if value.name == "int_value" => Some(value),
                _ => None,
            })
            .expect("actual declaration");
        foreign.name = "$raw$int_value".into();
        foreign.source_name = source_name.map(str::to_owned);
        let result = perceus::lower_with_declarations(
            ir.iter().chain(build::builtin_modules()),
            modules
                .iter()
                .map(|module| module.declarations.as_ref())
                .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
            &Name::new("App.BigIntConversion", "$raw$int_value"),
        );
        if source_name == Some("int_value") {
            assert!(result.is_ok());
        } else {
            assert!(
                result
                    .expect_err("no name-prefix guessing")
                    .to_string()
                    .contains("missing checked declaration scheme")
            );
        }
    }
}
