//! Checked scalar conversions and self-last min/max, with owned outcomes.
use crate::causes;
use flick_core::{
    build,
    ir::{
        ItemKind, Lit,
        memory::{Name, Program},
    },
    opt::perceus,
};
use flick_model::run;
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};

const FUEL: u64 = 100_000;

const SOURCE: &str = r#"module App.Scalars exposing (numbers, minimum, maximum, escaped, cold)
numbers : Unit -> String
let numbers () =
  let converted : Float = into (7 : Int)
  show (Int.to_float (-2147483647 - 1)) ++ "|" ++ show converted
    ++ "|" ++ show (BigInt.to_number 9007199254740993n)
    ++ "|" ++ show (Numeric.float_of_bigint (-9007199254740995n))
    ++ "|" ++ show (BigInt.to_number 0X20000000000003n)

minimum : Unit -> String
let minimum () =
  let nan : Float = 0.0 / 0.0
  let select = Float.min 0.0
  show (select (-0.0)) ++ "|" ++ show (Float.min nan 2.0)
    ++ "|" ++ show (Float.min 2.0 nan)
    ++ "|" ++ show (Float.min (1.0 / 0.0) (-1.0 / 0.0))

maximum : Unit -> String
let maximum () =
  let nan : Float = 0.0 / 0.0
  let select = Float.max (-0.0)
  show (select 0.0) ++ "|" ++ show (Float.max 0.0 (-0.0))
    ++ "|" ++ show (Float.max nan 2.0) ++ "|" ++ show (Float.max 2.0 nan)

apply : (Float -> Float) -> Float -> Float
let apply callback value = callback value

escaped : Unit -> String
let escaped () =
  let make first =
    let lower = Float.min first
    fun second -> Float.max (lower second) first
  let callback = make 3.0
  show (apply callback 1.0) ++ "|" ++ show (apply callback 5.0)

cold : String -> String
let cold seed =
  let _minimum = Float.min 1.0
  let _maximum = Float.max (-0.0)
  seed
"#;

const NUMBER_ABI_SOURCE: &str = r#"module App.NumberABI exposing (text_number, bool_number, unit_number, float_number, ref_number, js_number, opaque_number)
type Opaque
text_number : String -> Float
foreign pure text_number from global "Number"
bool_number : Bool -> Float
foreign pure bool_number from global "Number"
unit_number : Unit -> Float
foreign pure unit_number from global "Number"
float_number : Float -> Float
foreign pure float_number from global "Number"
ref_number : Ref Int -> Float
foreign pure ref_number from global "Number"
js_number : JsValue -> Float
foreign pure js_number from global "Number"
opaque_number : Opaque -> Float
foreign pure opaque_number from global "Number"
"#;

fn programs(name: &str) -> (Name, [Program; 2]) {
    foreign_programs(SOURCE, "App.Scalars", name)
}

fn foreign_programs(source: &str, module: &str, name: &str) -> (Name, [Program; 2]) {
    let modules = run::compile_source(source).expect("accepted scalar source");
    let entry = Name::new(module, name);
    let programs = [false, true].map(|release| {
        let ir: Vec<_> = modules
            .iter()
            .map(|module| {
                if release {
                    build::optimized(&modules, module, false)
                } else {
                    (*module.ir).clone()
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
        .expect("scalar adapter lowers")
    });
    (entry, programs)
}

fn expected(name: &str) -> &'static str {
    match name {
        "numbers" => "-2147483648|7|9007199254740992|-9007199254740996|9007199254740996",
        "minimum" => "-0|NaN|NaN|-Infinity",
        "maximum" => "0|-0|2|2",
        "escaped" => "3|3",
        "cold" => "outside",
        _ => panic!("known independent oracle"),
    }
}
fn check(name: &str) {
    let (entry, programs) = programs(name);
    for program in programs {
        let mut heap = Heap::new();
        let outside = heap
            .literal(&Lit::String("outside".into()))
            .expect("external root");
        let argument = if name == "cold" {
            heap.dup(&outside).expect("owned input alias")
        } else {
            Value::Unit
        };
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![argument], FUEL);
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.string(&value), Ok(expected(name)));
        assert_eq!(report.stdout, "");
        assert_eq!(heap.string(&outside), Ok("outside"));
        heap.drop_value(value).expect("result owner");
        heap.drop_value(outside).expect("external owner");
        let stats = heap.finish().expect("all scalar source roots released");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn all_four_registered_number_conversions_are_executed() {
    check("numbers");
}
#[test]
fn minimum_is_nan_least_and_selects_second_signed_zero() {
    check("minimum");
}
#[test]
fn maximum_is_nan_least_and_selects_second_signed_zero() {
    check("maximum");
}
#[test]
fn partially_applied_and_escaped_callbacks_preserve_source_stages() {
    check("escaped");
    check("cold");
}

#[test]
fn every_prefix_through_source_completion_releases_owned_roots() {
    for name in ["numbers", "minimum", "maximum", "escaped", "cold"] {
        let (entry, programs) = programs(name);
        for (profile, program) in programs.into_iter().enumerate() {
            let mut completed = false;
            let mut exhausted = false;
            for fuel in 0..=FUEL {
                let mut heap = Heap::new();
                let outside = heap
                    .literal(&Lit::String("outside".into()))
                    .expect("external root");
                let argument = if name == "cold" {
                    heap.dup(&outside).expect("owned input alias")
                } else {
                    Value::Unit
                };
                let report =
                    execute_program_outcome(&mut heap, &program, &entry, vec![argument], fuel);
                assert_eq!(report.stdout, "", "{name} fuel={fuel}");
                match report.outcome {
                    ProgramOutcome::Returned(value) => {
                        assert_eq!(heap.string(&value), Ok(expected(name)));
                        heap.drop_value(value).expect("result owner");
                        completed = true;
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => exhausted = true,
                    outcome => {
                        let diagnostic = format!("{name} fuel={fuel}: {outcome:?}");
                        causes::unexpected(&mut heap, outcome, &diagnostic)
                    }
                }
                assert_eq!(heap.string(&outside), Ok("outside"));
                heap.drop_value(outside).expect("external owner");
                let stats = heap.finish().expect("no scalar roots leaked on fuel cut");
                assert_eq!(stats.live_objects, 0);
                assert_eq!(stats.allocations, stats.frees);
                if completed {
                    eprintln!(
                        "scalar source {name} release={} first_success_fuel={fuel} prefixes={}",
                        profile == 1,
                        fuel + 1
                    );
                    break;
                }
            }
            assert!(
                exhausted && completed,
                "{name}: prefixes and actual completion"
            );
        }
    }
}

#[test]
fn global_number_abi_cannot_distinguish_registered_numeric_types_from_other_inputs() {
    use flick_core::ir::ItemKind;
    let text = NUMBER_ABI_SOURCE;
    let modules = run::compile_source(text).expect("all declarations are accepted");
    let canonical = build::builtin_modules()
        .iter()
        .find(|module| module.name == "Int")
        .expect("Int module")
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Foreign(value) if value.name == "to_float" => Some(value),
            _ => None,
        })
        .expect("canonical Number conversion");
    let mut same = Vec::new();
    for item in &modules[0].ir.items {
        let ItemKind::Foreign(value) = &item.kind else {
            continue;
        };
        assert_ne!(
            value.ty, canonical.ty,
            "{} carries its own Flick signature in typed IR",
            value.name
        );
        let mut normalized = value.clone();
        normalized.name.clone_from(&canonical.name);
        normalized.source_name.clone_from(&canonical.source_name);
        normalized.type_params.clone_from(&canonical.type_params);
        normalized.ty.clone_from(&canonical.ty);
        assert_eq!(
            &normalized, canonical,
            "{} has indistinguishable physical metadata",
            value.name
        );
        same.push(value.name.as_str());
    }
    same.sort_unstable();
    assert_eq!(
        same,
        [
            "bool_number",
            "float_number",
            "js_number",
            "opaque_number",
            "ref_number",
            "text_number",
            "unit_number"
        ]
    );
}

#[test]
fn indistinguishable_non_numeric_number_domains_reject_at_lowering_not_runtime() {
    let modules =
        run::compile_source(NUMBER_ABI_SOURCE).expect("all source signatures still accepted");
    for release in [false, true] {
        let ir: Vec<_> = modules
            .iter()
            .map(|module| {
                if release {
                    build::optimized(&modules, module, false)
                } else {
                    (*module.ir).clone()
                }
            })
            .collect();
        for name in [
            "text_number",
            "bool_number",
            "unit_number",
            "ref_number",
            "js_number",
            "opaque_number",
        ] {
            let entry = Name::new("App.NumberABI", name);
            for copied_input in [false, true] {
                let mut candidate = ir.clone();
                for module in &mut candidate {
                    for item in &mut module.items {
                        if let ItemKind::Foreign(value) = &mut item.kind
                            && value.name == name
                        {
                            value.adapter.copies.args[0] = copied_input;
                        }
                    }
                }
                let failure = perceus::lower_with_declarations(
                    candidate.iter().chain(build::builtin_modules()),
                    modules
                        .iter()
                        .map(|module| module.declarations.as_ref())
                        .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
                    &entry,
                )
                .expect_err("physical ABI alone is not numeric type evidence");
                assert!(
                    failure.to_string().contains("Number conversion"),
                    "{name} copied_input={copied_input}: {failure}"
                );
            }
        }
    }
}

#[test]
fn an_identical_abi_int_minmax_declaration_uses_integer_not_float_operations() {
    let source = r#"module App.IntChoice exposing (minimum, maximum, answer, cold)
minimum : Int -> Int -> Int
foreign pure minimum from "flick:Number" "min"
  arity 1
maximum : Int -> Int -> Int
foreign pure maximum from "flick:Number" "max"
  arity 1
answer : Unit -> String
let answer () =
  let low = minimum 3
  let high = maximum 3
  show (low 7) ++ "|" ++ show (high 7)
    ++ "|" ++ show (minimum (-2147483647 - 1) 2147483647)
    ++ "|" ++ show (maximum (-2147483647 - 1) 2147483647)
cold : String -> String
let cold seed =
  let _low = minimum 1
  let _high = maximum 2
  seed
"#;
    for (name, expected) in [
        ("answer", "3|7|-2147483648|2147483647"),
        ("cold", "outside"),
    ] {
        let (entry, programs) = foreign_programs(source, "App.IntChoice", name);
        for (profile, program) in programs.into_iter().enumerate() {
            let mut completed = false;
            for fuel in 0..=FUEL {
                let mut heap = Heap::new();
                let outside = heap
                    .literal(&Lit::String("outside".into()))
                    .expect("external root");
                let argument = if name == "cold" {
                    heap.dup(&outside).expect("owned alias")
                } else {
                    Value::Unit
                };
                let report =
                    execute_program_outcome(&mut heap, &program, &entry, vec![argument], fuel);
                assert_eq!(report.stdout, "");
                match report.outcome {
                    ProgramOutcome::Returned(value) => {
                        assert_eq!(heap.string(&value), Ok(expected));
                        heap.drop_value(value).expect("result owner");
                        completed = true;
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    outcome => {
                        let diagnostic = format!("{name} fuel={fuel}: {outcome:?}");
                        causes::unexpected(&mut heap, outcome, &diagnostic)
                    }
                }
                assert_eq!(heap.string(&outside), Ok("outside"));
                heap.drop_value(outside).expect("external owner");
                assert_eq!(
                    heap.finish()
                        .expect("all integer selection roots released")
                        .live_objects,
                    0
                );
                if completed {
                    eprintln!(
                        "Int selection {name} profile={profile} first_success={fuel} prefixes={}",
                        fuel + 1
                    );
                    break;
                }
            }
            assert!(
                completed,
                "Int selection must complete under ordinary fuel cap"
            );
        }
    }
}

#[test]
fn a_float_number_declaration_preserves_signed_zero_and_nan_bits() {
    let (entry, programs) = foreign_programs(NUMBER_ABI_SOURCE, "App.NumberABI", "float_number");
    for (profile, program) in programs.into_iter().enumerate() {
        for bits in [
            0,
            0x8000_0000_0000_0000,
            0x3ff0_0000_0000_0000,
            0x7ff8_0000_0000_0001,
            0xfff8_0000_0000_0002,
        ] {
            let mut completed = false;
            for fuel in 0..=FUEL {
                let mut heap = Heap::new();
                let outside = heap
                    .literal(&Lit::String("outside".into()))
                    .expect("external owner");
                let report = execute_program_outcome(
                    &mut heap,
                    &program,
                    &entry,
                    vec![Value::Float(f64::from_bits(bits))],
                    fuel,
                );
                assert_eq!(report.stdout, "");
                match report.outcome {
                    ProgramOutcome::Returned(Value::Float(value)) => {
                        assert_eq!(value.to_bits(), bits);
                        completed = true;
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    outcome => {
                        let diagnostic = format!("Float identity fuel={fuel}: {outcome:?}");
                        causes::unexpected(&mut heap, outcome, &diagnostic)
                    }
                }
                assert_eq!(heap.string(&outside), Ok("outside"));
                heap.drop_value(outside).expect("external owner");
                let stats = heap.finish().expect("float identity roots released");
                assert_eq!(stats.live_objects, 0);
                assert_eq!(stats.allocations, stats.frees);
                if completed {
                    eprintln!(
                        "Float identity profile={profile} bits={bits:x} first_success={fuel} prefixes={}",
                        fuel + 1
                    );
                    break;
                }
            }
            assert!(
                completed,
                "Float identity completes under ordinary fuel cap"
            );
        }
    }
}
