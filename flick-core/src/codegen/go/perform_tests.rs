//! The Go of service operations: one given all its arguments up to its
//! first effectful stage, where that stage runs, is one `flickrt.Perform`
//! of a package variable; any other use is the curried value.

use super::tests::app;

const GREETER: &str = "module App exposing (main)\n\n\
     service Greeter =\n  greet : String ->{} String\n  join : String -> String ->{} String\n\n\
     named : String -> Greeter.Impl\n\
     let named who = {\n  greet = fun n -> who ++ n\n  join = fun a b -> who ++ a ++ b\n}\n\n";

#[test]
fn an_operation_given_its_arguments_is_performed_through_a_package_variable() {
    let go = app(&format!(
        "{GREETER}main : Unit ->{{Host.Console; AppError}} Unit\n\
         let main () = provide Greeter (named \"a\") {{\n  \
         Platform.stdout_line (Greeter.greet \"x\")\n  \
         Platform.stdout_line (Greeter.join \"y\" \"z\")\n  \
         Platform.stdout_line (Greeter.greet \"w\")\n}}\n"
    ));
    // one variable for each operation of the module, however often it runs
    assert_eq!(
        go.matches("= flickrt.NewOp(\"App.Greeter\", \"greet\")")
            .count(),
        1,
        "{go}"
    );
    assert_eq!(
        go.matches("= flickrt.NewOp(\"App.Greeter\", \"join\")")
            .count(),
        1,
        "{go}"
    );
    let flat = go.replace(char::is_whitespace, "");
    assert_eq!(
        flat.matches("flickrt.Perform(fb,K_App__").count(),
        3,
        "{go}"
    );
    assert!(flat.contains(",\"y\",\"z\")"), "{go}");
    assert!(!go.contains("flickrt.Operation("), "{go}");
}

#[test]
fn an_operation_not_given_its_arguments_stays_a_value() {
    let go = app(&format!(
        "{GREETER}main : Unit ->{{Host.Console; AppError}} Unit\n\
         let main () = provide Greeter (named \"a\") {{\n  \
         let greet = Greeter.greet\n  \
         let join = Greeter.join \"y\"\n  \
         Platform.stdout_line (greet \"x\" ++ join \"z\")\n}}\n"
    ));
    // the function looks the implementation up when its task runs
    assert!(
        go.contains("flickrt.Operation(\"App.Greeter\", \"greet\", 1)"),
        "{go}"
    );
    assert!(
        go.contains("flickrt.Operation(\"App.Greeter\", \"join\", 2)"),
        "{go}"
    );
    assert!(!go.contains("flickrt.Perform("), "{go}");
}
