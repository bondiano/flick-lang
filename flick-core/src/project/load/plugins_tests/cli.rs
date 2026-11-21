use super::*;

#[test]
fn a_head_of_std_needs_no_import_and_no_other_plugin_gives_it() {
    // `cli_ir` imports nothing
    let block = "greet : Cli.Command {Host.Console} none\nlet greet = cli \"greet\" { handle fun {} -> Platform.stdout_line \"hi\" }\n\ntool : Cli.Command {Host.Console} none\nlet tool = cli \"tool\" { subcommand greet; subcommand version }";
    assert!(cli_ir(block).contains("Cli.handled"));
    let main = "module App.Main exposing (x)\n\nimport Pairs exposing (plugin sql)\n\nlet x = 1\n";
    assert_eq!(
        report(main),
        [
            "FLICK-P044 `plugin sql`: `sql` is a head of `std`'s plugin, which every module has, so the plugin of `pairs` cannot give it"
        ]
    );
}

/// The IR of `App.Main` with `greet` and `tool` as `body` writes them.
fn cli_ir(body: &str) -> String {
    const HANDLER: &str = "fun { loud; times; name } -> Platform.stdout_line (String.join \"\\n\" (List.repeat times (if loud then String.to_upper name else name)))";
    let main = format!(
        "module App.Main exposing (greet, tool)\n\nversion : Cli.Command {{Host.Console}} none\nlet version = Cli.command \"version\" |> Cli.handle (fun {{}} -> Platform.stdout_line \"0.1.0\")\n\n{}\n",
        body.replace("HANDLER", HANDLER)
    );
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let (diagnostics, compilation) = compile(&host(&main), &mut sources, Some(&project), &roots());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    ir::print(&compilation.modules[0].ir)
}

#[test]
fn a_cli_block_is_the_calls_of_cli_it_spells_out() {
    let block = cli_ir(
        "greet : Cli.Command {Host.Console} none\nlet greet = cli \"greet\" {\n  description \"Prints a greeting\"\n  /// Print in capitals\n  flag loud : Bool\n  /// How many\n  /// times\n  flag times \"n\" : Int = 1\n  arg name : String\n  handle HANDLER\n}\n\ntool : Cli.Command {Host.Console} none\nlet tool = cli \"tool\" { flag verbose \"v\" : Bool; subcommand greet; subcommand version }",
    );
    let calls = cli_ir(
        "greet : Cli.Command {Host.Console} none\nlet greet =\n  Cli.handled\n    (Cli.arg (Record.label : Record.Label \"name\") \"\" (Cli.required : Cli.Multi String String)\n      (Cli.flag (Record.label : Record.Label \"times\") [\"n\"] \"How many times\" (Cli.default 1)\n        (Cli.flag (Record.label : Record.Label \"loud\") [] \"Print in capitals\" Cli.switch\n          (Cli.description \"Prints a greeting\" (Cli.command \"greet\")))))\n    (HANDLER)\n\ntool : Cli.Command {Host.Console} none\nlet tool =\n  Cli.group\n    (Cli.subcommand version \"\"\n      (Cli.subcommand greet \"\"\n        (Cli.flag (Record.label : Record.Label \"verbose\") [\"v\"] \"\" Cli.switch (Cli.command \"tool\"))))",
    );
    assert_eq!(block, calls);
}
