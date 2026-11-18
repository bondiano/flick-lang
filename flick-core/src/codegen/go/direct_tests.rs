//! The Go of methods of known instances: a function or variable of its
//! own, called directly where the checker resolved the instance, with the
//! dictionaries of its context, while every other dictionary is looked up.

use super::tests::app;

const WEIGHT: &str = "module App exposing (main)\n\n\
     type Color = Red | Green\n  deriving Show\n\n\
     class Weight a =\n  weight : a -> Int\n  label : a -> String\n  let label x = show (weight x)\n\n\
     instance Weight Color =\n  let weight c =\n    match c with\n    | Red -> 1\n    | Green -> 2\n\n";

#[test]
fn a_method_of_a_known_instance_is_a_function_called_directly() {
    let go = app(&format!(
        "{WEIGHT}main : Unit ->{{Host.Console; AppError}} Unit\n\
         let main () = Platform.stdout_line (show (weight Green) ++ show Red)\n"
    ));
    // a method written in the instance, a derived one and one of the core
    let written = "F_App__Weight_x20_Color_x24_weight";
    assert!(
        go.contains(&format!(
            "func {written}(fb *flickrt.Fiber, l_c flickrt.Value) flickrt.Value {{"
        )),
        "{go}"
    );
    assert!(go.contains(&format!("{written}(fb, K_App__")), "{go}");
    assert!(
        go.contains("F_App__Show_x20_Color_x24_show(fb, K_App__"),
        "{go}"
    );
    // the call may break over lines, as wide as its arguments are
    let flat = go.replace(char::is_whitespace, "");
    assert!(
        flat.contains("F_Instances__Show_x20_Int_x24_show(fb,F_App__Weight"),
        "{go}"
    );
    // generic code reaches the same function through the dictionary,
    // which holds it as it is, not a wrapper converting its values
    assert!(
        go.contains(&format!("d.Set(\"weight\", {written})")),
        "{go}"
    );
    assert!(
        go.contains("d.Set(\"show\", F_App__Show_x20_Color_x24_show)"),
        "{go}"
    );
    assert!(!go.contains("flickrt.Method(fb, I_App__"), "{go}");
}

#[test]
fn a_dictionary_the_checker_does_not_know_is_looked_up() {
    let go = app(&format!(
        "{WEIGHT}heavy : Weight a => a -> Bool\nlet heavy x = weight x > 1\n\n\
         type Held =\n  | Weight b => Held b : Held\n\n\
         held : Held -> Int\nlet held h =\n  match h with\n  | Held x -> weight x\n\n\
         main : Unit ->{{Host.Console; AppError}} Unit\n\
         let main () =\n  \
         Platform.stdout_line (show (heavy Red, held (Held Red)))\n  \
         Platform.stdout_line (label Red)\n"
    ));
    // a constraint's dictionary is the caller's
    assert!(go.contains("flickrt.Method(fb, d0, \"weight\")"), "{go}");
    // one a value holds, bound by the pattern of its constructor
    assert!(
        go.lines()
            .any(|l| l.contains("flickrt.Method(fb, l_") && l.contains("\"weight\")")),
        "{go}"
    );
    // a default taken from the class
    assert!(
        go.contains("flickrt.Method(fb, I_App__Weight_x20_Color, \"label\")"),
        "{go}"
    );
    // a known instance passed as a dictionary is the dictionary still
    assert!(
        go.contains("F_App__heavy(fb, I_App__Weight_x20_Color, K_App__"),
        "{go}"
    );
}

#[test]
fn a_method_of_an_instance_with_a_context_takes_its_dictionaries() {
    let go = app(&format!(
        "{WEIGHT}instance Weight a => Weight (Option a) =\n  \
         let weight o =\n    match o with\n    | None -> 0\n    | Some x -> weight x\n  \
         let label _ = \"option\"\n\n\
         heavy : Weight a => a -> Bool\nlet heavy x = weight (Some x) > 1\n\n\
         main : Unit ->{{Host.Console; AppError}} Unit\n\
         let main () = Platform.stdout_line (show (weight (Some Green), heavy (Some Red)))\n"
    ));
    let function = "F_App__Weight_x20__x28_Option_x20_a_x29__x24_weight";
    // the dictionaries of the context first, as a constrained function's
    assert!(
        go.contains(&format!(
            "func {function}(fb *flickrt.Fiber, d0, l_o flickrt.Value) flickrt.Value {{"
        )),
        "{go}"
    );
    // given the known instance, and the caller's dictionary in a constrained
    // function: no dictionary of `Weight (Option a)` is built for either
    let flat: String = go.split_whitespace().collect();
    assert!(
        flat.contains(&format!("{function}(fb,I_App__Weight_x20_Color,")),
        "{go}"
    );
    assert!(
        flat.contains(&format!("{function}(fb,d0,flickrt.NewData")),
        "{go}"
    );
    // its own dictionary, passed on, holds the function applied to the context's
    assert!(flat.contains(&format!("return{function}(fb,d0,t")), "{go}");
    // the method it calls of the context's dictionary is that dictionary's
    assert!(go.contains("flickrt.Method(fb, d0, \"weight\")"), "{go}");
}

#[test]
fn an_instance_with_a_context_that_computes_a_method_is_built() {
    let go = app(&format!(
        "{WEIGHT}instance Weight a => Weight (Option a) =\n  \
         let weight =\n    let base = String.length \"four\"\n    fun _ -> base\n  \
         let label _ = \"option\"\n\n\
         main : Unit ->{{Host.Console; AppError}} Unit\n\
         let main () = Platform.stdout_line (label (Some Green))\n"
    ));
    // building it computes `weight`, which a direct call of `label` would skip
    assert!(!go.contains("_x28_Option_x20_a_x29__x24_"), "{go}");
    assert!(
        go.contains("I_App__Weight_x20__x28_Option_x20_a_x29_(fb, I_App__Weight_x20_Color)"),
        "{go}"
    );
}

#[test]
fn a_computed_method_of_an_instance_without_a_context_is_a_variable() {
    let go = app(&format!(
        "{WEIGHT}type Tag = Tag\n\n\
         instance Weight Tag =\n  let weight =\n    let base = String.length \"four\"\n    fun _ -> base\n\n\
         main : Unit ->{{Host.Console; AppError}} Unit\n\
         let main () = Platform.stdout_line (show (weight Tag))\n"
    ));
    let var = "V_App__Weight_x20_Tag_x24_weight";
    // computed once, as its dictionary is built, which holds it too
    assert!(go.contains(&format!("var {var} flickrt.Value")), "{go}");
    assert!(go.contains(&format!("d.Set(\"weight\", {var})")), "{go}");
    assert!(go.contains(&format!("flickrt.Apply(fb, {var}, ")), "{go}");
    assert!(
        !go.contains("flickrt.Method(fb, I_App__Weight_x20_Tag"),
        "{go}"
    );
}
