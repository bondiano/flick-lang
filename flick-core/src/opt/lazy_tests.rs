use crate::ir::print;
use crate::ir::tests::check_text;

use super::tests::{builtin_arity, builtin_generic};

/// The optimized IR of a self-contained module.
fn optimized(text: &str) -> String {
    let (mut ir, interface) = check_text("App", text).expect("lowers");
    super::optimize(
        &mut ir,
        &|name: &str| interface.exports_value(name),
        &builtin_generic,
        &builtin_arity,
    );
    print(&ir)
}

const MARK: &str = "module App exposing (..)\nmark : Int ->{} Int\nlet mark n = n\n";

/// O1a: a cell nothing reads is not made, nor its task.
#[test]
fn a_lazy_nothing_reads_is_not_made() {
    let ir = optimized(&format!(
        "{MARK}run : Unit ->{{}} Int\nlet run () =\n  ignore (lazy {{ mark 1 }})\n  mark 2\n"
    ));
    assert!(!ir.contains("lazy_make") && !ir.contains("(task "), "{ir}");
}

/// O1a: nor is one a function takes and drops, once it is inlined.
#[test]
fn a_lazy_a_function_drops_is_not_made() {
    let ir = optimized(&format!(
        "{MARK}dropped : Lazy r e Int -> Int -> Int\nlet dropped _ n = n\n\
         run : Unit ->{{}} Int\nlet run () = dropped (lazy {{ mark 1 }}) 2\n"
    ));
    assert!(!ir.contains("lazy_make") && !ir.contains("(task "), "{ir}");
}

/// O1b: a cell of a value is made done; forcing it runs nothing.
#[test]
fn a_lazy_of_a_value_is_made_done() {
    let ir = optimized(&format!(
        "{MARK}run : Unit ->{{Async}} Int\nlet run () =\n  let l = lazy {{ 5 }}\n  await l + await l\n"
    ));
    assert!(
        ir.contains("lazy_done 5") && !ir.contains("lazy_make"),
        "{ir}"
    );
}

/// O1c: a cell forced where it is made is not made; its task runs there.
#[test]
fn a_lazy_forced_where_it_is_made_is_not_made() {
    let ir = optimized(&format!(
        "{MARK}run : Unit ->{{Async}} Int\nlet run () = await (lazy {{ mark 7 }})\n"
    ));
    assert!(
        !ir.contains("lazy_make") && !ir.contains("lazy_force"),
        "{ir}"
    );
}

/// O1b and O1c at once: a value forced where its cell is made is the value.
#[test]
fn a_lazy_of_a_value_forced_where_it_is_made_is_the_value() {
    let ir = optimized(&format!(
        "{MARK}run : Unit ->{{Async}} Int\nlet run () = await (lazy {{ 5 }}) + 1\n"
    ));
    assert!(!ir.contains("lazy_") && ir.contains("-> 6)"), "{ir}");
}

/// A cell two forces share stays: its task runs once.
#[test]
fn a_shared_lazy_stays() {
    let ir = optimized(&format!(
        "{MARK}run : Unit ->{{Async}} Int\nlet run () =\n  let l = lazy {{ mark 7 }}\n  await l + await l\n"
    ));
    assert_eq!(ir.matches("lazy_make").count(), 1, "{ir}");
    assert_eq!(ir.matches("lazy_force").count(), 2, "{ir}");
}
