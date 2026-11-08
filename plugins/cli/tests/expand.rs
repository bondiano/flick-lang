//! The code `cli` blocks stand for, and what is wrong with the ones that
//! stand for none.

use flick_core::plugin::testing::expand;
use flick_plugin_cli::Cli;

/// What `block` stands for, the one block of a module.
fn one(block: &str) -> Result<String, Vec<String>> {
    let source = format!("module App.Main exposing (..)\n\nimport Cli\n\nlet c = {block}\n");
    let mut found = expand(&Cli, &source);
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

fn code(block: &str) -> String {
    one(block).unwrap_or_else(|errors| panic!("{errors:?}"))
}

fn errors(block: &str) -> String {
    one(block).expect_err("errors").join("\n")
}

#[test]
fn each_clause_is_a_call_and_the_handler_gets_the_options() {
    insta::assert_snapshot!(code(
        "cli \"greet\" {\n  description \"Prints a greeting\"\n  /// In capitals\n  flag loud \"l\" : Bool\n  flag times : Float = 1\n  arg name : Option String\n  handle fun { loud; times; name } -> ()\n}"
    ), @r#"
    Cli.handled
      ((Cli.arg
        (Record.label : (Record.Label "name"))
        ""
        (Cli.optional : (Cli.Multi _ (Option String)))
        (Cli.flag
          (Record.label : (Record.Label "times"))
          []
          ""
          (Cli.default (1 : Float))
          (Cli.flag
            (Record.label : (Record.Label "loud"))
            ["l"]
            "In capitals"
            (Cli.switch : (Cli.Multi _ Bool))
            (Cli.description "Prints a greeting" (Cli.command "greet"))))) : (Cli.Spec
        _
        _
        { loud : _; times : _; name : _ }))
      (fun { loud; times; name } -> ())
    "#);
}

#[test]
fn a_command_of_subcommands_is_a_group() {
    insta::assert_snapshot!(
        code("cli \"tool\" { subcommand greet; subcommand version }"),
        @r#"Cli.group (Cli.subcommand version "" (Cli.subcommand greet "" (Cli.command "tool")))"#
    );
}

#[test]
fn the_rules_of_a_command_are_its_errors() {
    insta::assert_snapshot!(
        errors("cli \"x\" { flag help : Bool; arg a : List String; arg b : String }"),
        @"
    L001 `help`: `--help` is reserved by the CLI
    L003 `List String`: an `arg` of many values takes the rest of the line: only the last one can
    L005 on the block: a command needs `handle` or at least one `subcommand`
    "
    );
    insta::assert_snapshot!(
        errors("cli { handle f }"),
        @r#"L009 on the block: a `cli` block names its command: `cli "name" { … }`"#
    );
    insta::assert_snapshot!(
        errors("cli \"x\" { flag : Bool }"),
        @"L009 `flag`: `flag` takes a name: `flag name : T`"
    );
}
