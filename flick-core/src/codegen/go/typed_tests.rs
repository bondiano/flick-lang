//! The Go of bindings to functions of the runtime that take and give Go's
//! own types.

use super::tests::app;

#[test]
fn a_binding_of_numbers_takes_and_gives_them_in_their_go_type() {
    let go = app("module App exposing (main)\n\n\
         root : Int -> Float\nlet root n = Float.floor (Float.sqrt (Int.to_float n))\n\n\
         odd : Int -> Bool\nlet odd n = Int.bit_and n 1 == 1\n\n\
         main : Unit ->{; AppError} Unit\n\
         let main () = Platform.stdout_line (show (root 9, odd 3, List.map Float.abs [1.5]))\n");
    // a number goes from one function of the runtime to the next as it is
    assert!(
        go.contains("return F_Float__floor(fb, F_Float__sqrt(fb, F_Int__to_float(fb, l_n)))"),
        "{go}"
    );
    assert!(
        go.contains("return (F_Int__bit_and(fb, l_n, int32(1)) == int32(1))"),
        "{go}"
    );
    // given as a value, such a function is wrapped in one of values
    assert!(
        go.contains("return F_Float__abs(fb, flickrt.AsFloat("),
        "{go}"
    );
}
