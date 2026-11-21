//! Stage 4D Wave 2 counted-execution conformance tests: `Arbitrary`, `Cause` laws,
//! `Config`, `Logger`, and Wave 2 fuel-prefix leak sweeps.

use std::path::Path;

use flick_model::memory::{Error, Heap, ProgramOutcome, Value};
use flick_model::run;

use super::check_fixture;

fn check_source(src: &str, expected: &str, label: &str, fuel: u64) {
    let modules = run::compile_source(src).unwrap_or_else(|e| panic!("{label}: compile:\n{e}"));
    let main = run::main_of(&modules).unwrap_or_else(|e| panic!("{label}: main: {e}"));
    for release in [false, true] {
        let counted = run::counted_main(&modules, &main, release)
            .unwrap_or_else(|e| panic!("{label} (release={release}): lower: {e}"));
        let mut heap = Heap::new();
        let report = counted.execute(&mut heap, fuel);
        let ok = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
        let detail = format!("{:?}", report.outcome);
        let stdout = report.stdout.clone();
        report
            .outcome
            .release(&mut heap)
            .unwrap_or_else(|e| panic!("{label} (release={release}): release: {e}"));
        let stats = heap
            .finish()
            .unwrap_or_else(|e| panic!("{label} (release={release}): leak: {e}"));
        assert_eq!(stats.live_objects, 0, "{label} (release={release}): live");
        assert_eq!(
            stats.allocations, stats.frees,
            "{label} (release={release}): allocs"
        );
        assert!(
            ok,
            "{label} (release={release}): outcome {detail}\nstdout:\n{stdout}"
        );
        assert_eq!(
            stdout, expected,
            "{label} (release={release}): stdout mismatch"
        );
    }
}

#[test]
fn stage4d_wave2_arbitrary_and_cause_laws_match_stdout_with_zero_leaks() {
    let run_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    let cases = ["stdlib/effect/arbitrary", "runtime/cause_laws"];
    let mut failures = Vec::new();
    for case in cases {
        if let Err(msg) = check_fixture(&run_root.join(format!("{case}.fl")), case, 200_000_000) {
            failures.push(msg);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} Stage 4D Wave 2 fixtures failed:\n\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n\n")
    );
}

const CFG_SRC: &str = r#"module App.Stage4dConfig exposing (main)

type alias Server = { host : String; port : Float }

env : ConfigProvider.ConfigProvider
let env =
  ConfigProvider.from_env_record { preserveEmptyStrings = false } [
    ("HOST", "localhost"),
    ("PORT", "8080"),
    ("DEBUG", "yes"),
    ("RATIO", "0.5"),
    ("NAN", "NaN"),
    ("COUNT", "3"),
    ("BIG", "3000000000"),
    ("TIMEOUT", "10 seconds"),
    ("SIZE", "2 KiB"),
    ("LEVEL", "Warn"),
    ("SITE", "https://example.com/a/../b"),
    ("DAY", "2024-01-02T00:00:00Z"),
    ("BAD_DAY", "not a date"),
    ("OLD_DAY", "Tue, 15 Nov 1994 08:12:31 GMT"),
    ("MODE", "prod"),
    ("TAGS", "a,b,c"),
    ("PAIRS", "a=1;b=2"),
    ("DB_HOST", "db"),
    ("EMPTY", ""),
    ("BAD_PORT", "99999"),
  ]

read : Config.Config a -> (a -> String) ->{} String
let read config describe =
  try describe (Config.parse env config) with
  | Config.ConfigError message -> "error: " ++ message

server : Config.Config Server
let server =
  Config.string "HOST"
  |> Config.flat_map (fun host ->
    Config.port "PORT" |> Config.map (fun port -> { host = host; port = port })
  )

even : Int ->{; Config.ConfigError} Int
let even n = if n % 2 == 0 then n else Fail.fail (Config.ConfigError "odd")

own : ConfigProvider.ConfigProvider
let own =
  ConfigProvider.make (fun (path : ConfigProvider.Path) ->
    match path with
    | [ConfigProvider.Key "NAME"] -> Some (ConfigProvider.make_value "own")
    | [ConfigProvider.Key "LIST"] -> Some (ConfigProvider.make_array 2 None)
    | [ConfigProvider.Key "LIST", ConfigProvider.Index i] ->
      Some (ConfigProvider.make_value (show (i * 10)))
    | [ConfigProvider.Key "REC"] -> Some (ConfigProvider.make_record ["x"] None)
    | [ConfigProvider.Key "REC", ConfigProvider.Key "x"] -> Some (ConfigProvider.ValueNode "1")
    | [ConfigProvider.Key "BROKEN"] -> Fail.fail (ConfigProvider.SourceError "unreachable")
    | _ -> None
  )

main : Unit ->{Host.Environment, Host.Console, Async; AppError} Unit
let main () =
  Platform.stdout_line (read server (fun s -> s.host ++ ":" ++ show s.port))
  Platform.stdout_line (read (Config.boolean "DEBUG") show)
  Platform.stdout_line (read (Config.all [Config.number "RATIO", Config.finite "RATIO", Config.map Int.to_float (Config.int "COUNT")]) show)
  Platform.stdout_line (read (Config.number "NAN") show)
  Platform.stdout_line (read (Config.finite "NAN") show)
  Platform.stdout_line (read (Config.int "RATIO") show)
  Platform.stdout_line (read (Config.int "BIG") show)
  Platform.stdout_line (read (Config.duration "TIMEOUT") Duration.format)
  Platform.stdout_line (read (Config.byte_size "SIZE") show)
  Platform.stdout_line (read (Config.log_level "LEVEL") show)
  Platform.stdout_line (read (Config.url "SITE") (fun u -> u))
  Platform.stdout_line (read (Config.date "DAY") show)
  Platform.stdout_line (read (Config.date "BAD_DAY") show)
  Platform.stdout_line (read (Config.date "OLD_DAY") show)
  Platform.stdout_line (read (Config.literal "prod" "MODE") (fun m -> m))
  Platform.stdout_line (read (Config.literals ["dev", "test"] "MODE") (fun m -> m))
  Platform.stdout_line (read (Config.array { separator = None } Schema.string "TAGS") show)
  Platform.stdout_line (read (Config.record { separator = Some ";"; keyValueSeparator = None } Schema.number "PAIRS") show)
  Platform.stdout_line (read (Config.schema Schema.number "COUNT") show)
  Platform.stdout_line (read (Config.nested "DB" (Config.string "HOST")) (fun h -> h))
  Platform.stdout_line (read (Config.non_empty_string "EMPTY") (fun h -> h))
  Platform.stdout_line (read (Config.port "BAD_PORT") show)
  Platform.stdout_line (read (Config.string "MISSING") (fun h -> h))
  Platform.stdout_line (read (Config.option (Config.string "MISSING")) show)
  Platform.stdout_line (read (Config.with_default "fallback" (Config.string "MISSING")) (fun h -> h))
  Platform.stdout_line (read (Config.or_else (fun (Config.ConfigError _) -> Config.succeed "recovered") (Config.string "MISSING")) (fun h -> h))
  Platform.stdout_line (read (Config.fail "refused") (fun h -> h))
  Platform.stdout_line (read (Config.map_effect even (Config.int "COUNT")) show)
  Platform.stdout_line (read (Config.map_effect even (Config.int "PORT")) show)
  let from_own =
    try Config.parse own (Config.all [Config.string "NAME", Config.string "LIST"]) with
    | Config.ConfigError m -> ["error " ++ m]
  Platform.stdout_line ("own " ++ show from_own)
  Platform.stdout_line ("own array " ++ show (try Config.parse own (Config.array { separator = None } Schema.number "LIST") with | Config.ConfigError _ -> []))
  Platform.stdout_line ("own record " ++ show (try Config.parse own (Config.record { separator = None; keyValueSeparator = None } Schema.number "REC") with | Config.ConfigError _ -> []))
  Platform.stdout_line ("own broken " ++ (try Config.parse own (Config.string "BROKEN") with | Config.ConfigError m -> m))
  let from_json =
    match Json.parse "{\"app\": {\"name\": \"json\", \"ports\": [1, 2]}}" with
    | Ok json ->
      let unknown = ConfigProvider.from_unknown { preserveEmptyStrings = false } json
      try Config.parse unknown (Config.nested "app" (Config.string "name")) with
      | Config.ConfigError m -> m
    | Err _ -> "not JSON"
  Platform.stdout_line ("from_unknown " ++ from_json)
  let dotenv = ConfigProvider.from_dot_env_contents { expandVariables = true; preserveEmptyStrings = false } "A=1\nB=${A}2\n"
  Platform.stdout_line ("dotenv " ++ (try Config.parse dotenv (Config.string "B") with | Config.ConfigError m -> m))
  let cased = ConfigProvider.constant_case env
  Platform.stdout_line ("constant_case " ++ (try Config.parse cased (Config.string "dbHost") with | Config.ConfigError m -> m))
  let prefixed = ConfigProvider.nested "DB" env
  Platform.stdout_line ("nested " ++ (try Config.parse prefixed (Config.string "HOST") with | Config.ConfigError m -> m))
  let mapped = ConfigProvider.map_input (fun _ -> [ConfigProvider.Key "MODE"]) env
  Platform.stdout_line ("map_input " ++ (try Config.parse mapped (Config.string "ANY") with | Config.ConfigError m -> m))
  let both = own |> ConfigProvider.or_else env
  Platform.stdout_line ("or_else " ++ (try Config.parse both (Config.string "HOST") with | Config.ConfigError m -> m))
  let provided = try provide (ConfigProvider.layer env) { Config.get (Config.string "MODE") } with | Config.ConfigError m -> m
  Platform.stdout_line ("layer " ++ provided)
  let host_vars = [("MODE", "from the host"), ("PORTS_0", "80"), ("PORTS_1", "443"), ("LABELS_TEAM", "core"), ("LABELS_TIER", "web")]
  let host_env = Layer.succeed Host.Environment {
    env = fun name -> List.find (fun (key, _) -> key == name) host_vars |> Option.map (fun (_, value) -> value)
    vars = fun () -> host_vars
  }
  let from_host = try provide host_env { Config.get (Config.all [Config.string "MODE", Config.string "HOST"]) } with | Config.ConfigError m -> ["missing " ++ show (String.contains "HOST" m)]
  let read_mode = try provide host_env { Config.get (Config.with_default "unset" (Config.string "MODE")) } with | Config.ConfigError m -> m
  Platform.stdout_line ("host " ++ show from_host ++ " " ++ read_mode)
  let ports = try provide host_env { Config.get (Config.array { separator = None } Schema.number "PORTS") } with | Config.ConfigError _ -> []
  let labels = try provide host_env { Config.get (Config.record { separator = None; keyValueSeparator = None } Schema.string "LABELS") } with | Config.ConfigError m -> [("error", m)]
  Platform.stdout_line ("host array " ++ show ports ++ " record " ++ show labels)
  let fallen_back = try Config.get_with (fun () -> env) (Config.string "HOST") with | Config.ConfigError m -> m
  let current = try provide (ConfigProvider.layer own) { Config.get_with (fun () -> env) (Config.string "NAME") } with | Config.ConfigError m -> m
  Platform.stdout_line ("get_with " ++ fallen_back ++ " " ++ current)
  let added = try provide (ConfigProvider.layer env) { provide (ConfigProvider.layer_add { asPrimary = true } own) { Config.get (Config.all [Config.string "NAME", Config.string "HOST"]) } } with | Config.ConfigError m -> [m]
  Platform.stdout_line ("layer_add " ++ show added)
  let added_to_host = try provide host_env { provide (ConfigProvider.layer_add { asPrimary = false } own) { Config.get (Config.all [Config.string "NAME", Config.string "MODE"]) } } with | Config.ConfigError m -> [m]
  Platform.stdout_line ("layer_add to the host " ++ show added_to_host)
  let process = ConfigProvider.from_env { preserveEmptyStrings = false }
  Platform.stdout_line ("from_env " ++ (try Config.parse process (Config.string "FLICK_NO_SUCH_VARIABLE") with | Config.ConfigError _ -> "missing"))
"#;
const CFG_EXPECTED: &str = "\
localhost:8080\n\
true\n\
[0.5, 0.5, 3]\n\
NaN\n\
error: SchemaError(Expected a string representing a finite number\n  at [\"NAN\"])\n\
error: SchemaError(Expected an integer\n  at [\"RATIO\"])\n\
error: SchemaError(Expected a value between -2147483648 and 2147483647\n  at [\"BIG\"])\n\
10s\n\
2048n\n\
Warn\n\
https://example.com/b\n\
1704153600000\n\
error: SchemaError(Expected a valid Date\n  at [\"BAD_DAY\"])\n\
784887151000\n\
prod\n\
error: SchemaError(Expected \"dev\" | \"test\"\n  at [\"MODE\"])\n\
[\"a\", \"b\", \"c\"]\n\
[(\"a\", 1), (\"b\", 2)]\n\
3\n\
db\n\
error: SchemaError(Expected string\n  at [\"EMPTY\"])\n\
error: SchemaError(Expected a value between 1 and 65535\n  at [\"BAD_PORT\"])\n\
error: SchemaError(Expected string\n  at [\"MISSING\"])\n\
None\n\
fallback\n\
recovered\n\
error: SourceError: refused\n\
error: odd\n\
8080\n\
own [\"error SchemaError(Expected string\\n  at [\\\"LIST\\\"])\"]\n\
own array [0, 10]\n\
own record [(\"x\", 1)]\n\
own broken SourceError: unreachable\n\
from_unknown json\n\
dotenv 12\n\
constant_case db\n\
nested db\n\
map_input prod\n\
or_else localhost\n\
layer prod\n\
host [\"missing true\"] from the host\n\
host array [80, 443] record [(\"TEAM\", \"core\"), (\"TIER\", \"web\")]\n\
get_with localhost own\n\
layer_add [\"own\", \"localhost\"]\n\
layer_add to the host [\"own\", \"from the host\"]\n\
from_env missing\n";
const LOG_SRC: &str = r#"module App.Stage4dLogger exposing (main)

ms : Float -> Duration.Duration
let ms n = Result.with_default Duration.zero (Duration.millis n)

simple : Logger.Logger String
let simple = Logger.make (fun o -> show o.logLevel ++ " " ++ String.join " " o.message)

replace : Logger.LayerOptions
let replace = { mergeWithExisting = false }

logged : Layer none none none -> (Unit ->{Log.Log} Unit) ->{Async} Unit
let logged loggers body = provide (Layer.merge loggers Logger.effect_log) { body () }

main : Unit ->{Host.Console, Async} Unit
let main () =
  logged (Logger.layer replace [Logger.with_console_log simple]) (fun () ->
    Log.info "hello"
    Log.warn "careful"
    Log.debug "below"
  )
  logged (Logger.layer replace [simple |> Logger.map String.to_upper |> Logger.with_console_log]) (fun () -> Log.error "mapped")
  logged (Logger.layer replace [Logger.with_leveled_console simple]) (fun () ->
    Log.info "leveled info"
    Log.error "leveled error"
  )
  let seen = Ref.make []
  let collect = { window = ms 1; flush = fun outputs -> Ref.set seen (Ref.get seen ++ outputs) }
  logged (Logger.batched collect replace (Logger.make (fun (o : Logger.Options) -> o.date > 0))) (fun () -> Log.info "batched")
  Platform.stdout_line ("batched " ++ show (Ref.get seen))
  logged (Logger.layer replace []) (fun () ->
    Log.info "no logger"
    Logger.with_loggers [Logger.with_console_log simple] (task { Log.info "with_loggers" })
  )
"#;
const LOG_EXPECTED: &str = "Info hello\nWarn careful\nERROR MAPPED\nInfo leveled info\nbatched [true]\nInfo with_loggers\n";

#[test]
fn stage4d_wave2_config_and_logger_non_host_match_stdout_with_zero_leaks() {
    check_source(CFG_SRC, CFG_EXPECTED, "config_non_host", 10_000_000);
    check_source(LOG_SRC, LOG_EXPECTED, "logger_non_host", 10_000_000);
}

#[test]
fn stage4d_wave2_fuel_prefix_sweeps_release_config_and_arbitrary_roots() {
    let src = r#"module App.Stage4dWave2Sweep exposing (main)

type alias Pt = { x : Float; tags : List String }
  deriving Schema

main : Unit ->{Host.Environment, Async; AppError} Unit
let main () =
  let p = ConfigProvider.from_dot_env_contents { expandVariables = true; preserveEmptyStrings = false } "A=1\nB=${A}2\n"
  let _ = try Config.parse p (Config.int "B") with | Config.ConfigError _ -> 0
  let opts : Arbitrary.CheckOptions = {
    runs = Some 3
    size = Some 2
    maxDiscards = None
    maxShrinks = Some 4
    seed = Some 7
    replay = None
  }
  let res = Arbitrary.check opts (fun pt -> pt.x < 5) (Arbitrary.from_schema Pt.schema)
  let _ = Arbitrary.format_check_failure res
  ()
"#;
    let modules = run::compile_source(src).expect("w2 sweep compiles");
    let main = run::main_of(&modules).expect("w2 sweep main");
    let counted = run::counted_main(&modules, &main, false).expect("w2 sweep lowers");

    let mut terminal = None;
    let mut fuel = 0u64;
    while fuel <= 250_000u64 {
        let mut heap = Heap::new();
        let report = counted.execute(&mut heap, fuel);
        let exhausted = matches!(report.outcome, ProgramOutcome::Fault(Error::OutOfFuel));
        let ok = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
        let detail = format!("{:?}", report.outcome);
        report.outcome.release(&mut heap).expect("outcome releases");
        let stats = heap.finish().expect("no leak at fuel prefix");
        assert_eq!(stats.live_objects, 0, "live objects at fuel={fuel}");
        assert_eq!(stats.allocations, stats.frees, "alloc/free at fuel={fuel}");
        if !exhausted {
            assert!(ok, "unexpected terminal outcome at fuel={fuel}: {detail}");
            terminal = Some(fuel);
            break;
        }
        fuel += if fuel < 2_000 {
            1
        } else if fuel < 20_000 {
            13
        } else {
            97
        };
    }
    assert!(terminal.is_some(), "w2 sweep did not reach terminal fuel");
}
