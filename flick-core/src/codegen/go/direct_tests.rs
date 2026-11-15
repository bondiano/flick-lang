//! The Go of methods of known instances: a function of its own, called
//! directly where the checker resolved the instance, while every other
//! dictionary is looked up.

use super::tests::app;

const WEIGHT: &str = "module App exposing (main)\n\n\
     type Color = Red | Green\n  deriving Show\n\n\
     class Weight a =\n  weight : a -> Int\n  label : a -> String\n  let label x = show (weight x)\n\n\
     instance Weight Color =\n  let weight c =\n    match c with\n    | Red -> 1\n    | Green -> 2\n\n";

#[test]
fn a_method_of_a_known_instance_is_a_function_called_directly() {
    let go = app(&format!(
        "{WEIGHT}main : Unit ->{{; AppError}} Unit\n\
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
    assert!(
        go.contains("F_Instances__Show_x20_Int_x24_show(fb, F_App__Weight"),
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
         instance Weight a => Weight (Option a) =\n  \
         let weight o =\n    match o with\n    | None -> 0\n    | Some x -> weight x\n\n\
         type Held =\n  | Weight b => Held b : Held\n\n\
         held : Held -> Int\nlet held h =\n  match h with\n  | Held x -> weight x\n\n\
         type Tag = Tag\n\n\
         instance Weight Tag =\n  let weight =\n    let base = String.length \"four\"\n    fun _ -> base\n\n\
         main : Unit ->{{; AppError}} Unit\n\
         let main () =\n  \
         Platform.stdout_line (show (heavy Red, weight (Some Green), held (Held Red)))\n  \
         Platform.stdout_line (show (weight Tag) ++ label Red)\n"
    ));
    // a constraint's dictionary is the caller's
    assert!(go.contains("flickrt.Method(fb, d0, \"weight\")"), "{go}");
    // one a value holds, bound by the pattern of its constructor
    assert!(
        go.lines()
            .any(|l| l.contains("flickrt.Method(fb, l_") && l.contains("\"weight\")")),
        "{go}"
    );
    // a method computed before it is a function, and a default taken
    assert!(
        go.contains("flickrt.Method(fb, I_App__Weight_x20_Tag, \"weight\")"),
        "{go}"
    );
    assert!(
        go.contains("flickrt.Method(fb, I_App__Weight_x20_Color, \"label\")"),
        "{go}"
    );
    // nor is a method of an instance with a context its own function
    assert_eq!(
        go.matches("_x24_weight(fb *flickrt.Fiber").count(),
        1,
        "{go}"
    );
    // a known instance passed as a dictionary is the dictionary still
    assert!(
        go.contains("F_App__heavy(fb, I_App__Weight_x20_Color, K_App__"),
        "{go}"
    );
}
