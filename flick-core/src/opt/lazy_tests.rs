use crate::ir::print;
use crate::ir::tests::check_text;

use super::tests::{builtin_arity, builtin_generic};

/// The optimized IR of a self-contained module.
pub(super) fn optimized(text: &str) -> String {
    let (mut ir, interface) = check_text("App", text).expect("lowers");
    super::optimize(
        &mut ir,
        &|name: &str| interface.exports_value(name),
        &builtin_generic,
        &builtin_arity,
    );
    print(&ir)
}

pub(super) const MARK: &str = "module App exposing (..)\nmark : Int ->{} Int\nlet mark n = n\n";

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

/// O2a: a force after `let a = await l` is `a`: the cell is done.
#[test]
fn a_lazy_forced_again_reuses_the_first_force() {
    let ir = optimized(&format!(
        "{MARK}run : Unit ->{{Async}} Int\nlet run () =\n  let l = lazy {{ mark 7 }}\n  let a = await l\n  \
         let f = fun () -> await l\n  a + await l + f ()\n"
    ));
    assert_eq!(ir.matches("lazy_force").count(), 1, "{ir}");
}

/// O2a reuses a force of the same cell only: another cell is forced.
#[test]
fn a_force_of_another_lazy_stays() {
    let ir = optimized(&format!(
        "{MARK}run : Unit ->{{Async}} Int\nlet run () =\n  let l = lazy {{ mark 7 }}\n  \
         let m = lazy {{ mark 8 }}\n  let a = await l\n  a + await m + await l\n"
    ));
    assert_eq!(ir.matches("lazy_force").count(), 2, "{ir}");
}

/// A pure recursive helper the inliner will not erase, so `lazy { work x }`
/// stays a `lazy_make (task ...)` rather than folding to `lazy_done`.
pub(super) const PURE_WORK: &str = "module App exposing (..)\n\
    work : Int -> Int\n\
    let rec work n = if n <= 0 then 1 else n + work (n - 1)\n";

/// O4: a pure loop-invariant `lazy` inside a `while` loop hoists before the loop.
#[test]
fn a_pure_loop_invariant_lazy_in_a_while_loop_is_hoisted() {
    let ir = optimized(&format!(
        "{PURE_WORK}run : Int ->{{Async}} Int\nlet run x =\n  let mut i = 0\n  let mut acc = 0\n  \
         while i < 3 do {{\n    let l = lazy {{ work x }}\n    acc = acc + await l\n    i = i + 1\n  }}\n  acc\n"
    ));
    let make = ir.find("lazy_make").expect(&ir);
    let loop_at = ir.find("(control-loop ").expect(&ir);
    assert!(make < loop_at, "{ir}");
    assert_eq!(ir.matches("lazy_make").count(), 1, "{ir}");
    assert_eq!(ir.matches("lazy_force").count(), 1, "{ir}");
}

/// O4: a pure loop-invariant `lazy` inside `for ... do` hoists before the loop.
#[test]
fn a_pure_loop_invariant_lazy_in_a_for_loop_is_hoisted() {
    let ir = optimized(&format!(
        "{PURE_WORK}run : Int -> List Int ->{{Async}} Int\nlet run x xs =\n  let mut acc = 0\n  \
         for item <- xs do {{\n    let l = lazy {{ work x }}\n    acc = acc + item + await l\n  }}\n  acc\n"
    ));
    let make = ir.find("lazy_make").expect(&ir);
    let loop_at = ir.find("(control-loop ").expect(&ir);
    assert!(make < loop_at, "{ir}");
    assert_eq!(ir.matches("lazy_make").count(), 1, "{ir}");
}

/// O4: a pure loop-invariant `lazy` inside a local `let rec` loop in a runtime
/// block hoists before the local function.
#[test]
fn a_pure_loop_invariant_lazy_in_a_recursive_loop_is_hoisted() {
    let ir = optimized(&format!(
        "{PURE_WORK}run : Int ->{{Async}} Int\nlet run x =\n  \
         go : Int -> Int ->{{Async}} Int\n  let rec go n acc =\n    \
         if n <= 0 then acc\n    else\n      let l = lazy {{ work x }}\n      go (n - 1) (acc + await l)\n  \
         go 3 0\n"
    ));
    let make = ir.find("lazy_make").expect(&ir);
    let rec_at = ir.find("let rec go").expect(&ir);
    assert!(make < rec_at, "{ir}");
    assert_eq!(ir.matches("lazy_make").count(), 1, "{ir}");
}

/// O4: nested loops hoist each pure lazy to its outermost invariant loop.
#[test]
fn a_pure_lazy_hoists_through_nested_loops_to_its_outermost_invariant_scope() {
    let ir = optimized(&format!(
        "{PURE_WORK}run : Int -> List Int ->{{Async}} Int\nlet run k xs =\n  let mut acc = 0\n  \
         for x <- xs do {{\n    let mut i = 0\n    while i < 2 do {{\n      \
         let outer = lazy {{ work k }}\n      let mid = lazy {{ work x }}\n      \
         acc = acc + await outer + await mid\n      i = i + 1\n    }}\n  }}\n  acc\n"
    ));
    let loops: Vec<usize> = ir.match_indices("(control-loop ").map(|(i, _)| i).collect();
    let makes: Vec<usize> = ir.match_indices("lazy_make").map(|(i, _)| i).collect();
    assert_eq!(loops.len(), 2, "{ir}");
    assert_eq!(makes.len(), 2, "{ir}");
    // `outer` hoists before the outer `for`; `mid` hoists before the inner `while`
    assert!(makes[0] < loops[0], "{ir}");
    assert!(loops[0] < makes[1] && makes[1] < loops[1], "{ir}");
}

/// O4 runs before O1c on non-value pure lazies in a loop: `await (lazy { work x })`
/// hoists the cell so `work x` runs once, while `await (lazy { 5 })` still folds to `5`.
#[test]
fn a_pure_lazy_forced_directly_in_a_loop_is_hoisted_and_a_value_still_folds() {
    let ir = optimized(&format!(
        "{PURE_WORK}run : Int ->{{Async}} Int\nlet run x =\n  let mut i = 0\n  let mut acc = 0\n  \
         while i < 3 do {{\n    acc = acc + await (lazy {{ work x }}) + await (lazy {{ 5 }})\n    i = i + 1\n  }}\n  acc\n"
    ));
    let make = ir.find("lazy_make").expect(&ir);
    let loop_at = ir.find("(control-loop ").expect(&ir);
    assert!(make < loop_at, "{ir}");
    assert_eq!(ir.matches("lazy_make").count(), 1, "{ir}");
    assert_eq!(ir.matches("lazy_force").count(), 1, "{ir}");
    assert!(!ir.contains("lazy_done"), "{ir}");
}

/// O4: a pure lazy inside a branch of a loop hoists before the loop;
/// hoisting allocates an unforced cell without running its initializer.
#[test]
fn a_pure_lazy_in_a_branch_inside_a_loop_is_hoisted() {
    let ir = optimized(&format!(
        "{PURE_WORK}run : Int -> Bool ->{{Async}} Int\nlet run x flag =\n  let mut i = 0\n  let mut acc = 0\n  \
         while i < 3 do {{\n    if flag then {{\n      let l = lazy {{ work x }}\n      acc = acc + await l\n    }} else ()\n    \
         i = i + 1\n  }}\n  acc\n"
    ));
    let make = ir.find("lazy_make").expect(&ir);
    let loop_at = ir.find("(control-loop ").expect(&ir);
    assert!(make < loop_at, "{ir}");
}

/// Negative O4: a lazy reading a loop-variant local stays inside the loop.
#[test]
fn a_lazy_reading_a_loop_variant_local_stays_in_the_loop() {
    let ir_for = optimized(&format!(
        "{PURE_WORK}run : List Int ->{{Async}} Int\nlet run xs =\n  let mut acc = 0\n  \
         for item <- xs do {{\n    let l = lazy {{ work item }}\n    acc = acc + await l\n  }}\n  acc\n"
    ));
    let make = ir_for.find("lazy_make").expect(&ir_for);
    let loop_at = ir_for.find("(control-loop ").expect(&ir_for);
    assert!(loop_at < make, "{ir_for}");

    let ir_while = optimized(&format!(
        "{PURE_WORK}run : Unit ->{{Async}} Int\nlet run () =\n  let mut i = 0\n  let mut acc = 0\n  \
         while i < 3 do {{\n    let cur = i\n    let l = lazy {{ work cur }}\n    acc = acc + await l\n    i = i + 1\n  }}\n  acc\n"
    ));
    let make = ir_while.find("lazy_make").expect(&ir_while);
    let loop_at = ir_while.find("(control-loop ").expect(&ir_while);
    assert!(loop_at < make, "{ir_while}");

    let ir_rec = optimized(&format!(
        "{PURE_WORK}run : Unit ->{{Async}} Int\nlet run () =\n  \
         go : Int -> Int ->{{Async}} Int\n  let rec go n acc =\n    \
         if n <= 0 then acc\n    else\n      let l = lazy {{ work n }}\n      go (n - 1) (acc + await l)\n  \
         go 3 0\n"
    ));
    let make = ir_rec.find("lazy_make").expect(&ir_rec);
    let rec_at = ir_rec.find("let rec go").expect(&ir_rec);
    assert!(rec_at < make, "{ir_rec}");
}

/// Negative O4: a runtime initializer (even with empty service & error rows,
/// e.g. touching a `Ref`) stays inside the loop.
#[test]
fn a_runtime_lazy_with_empty_rows_stays_in_the_loop() {
    let ir = optimized(&format!(
        "{MARK}run : Int ->{{Async}} Int\nlet run x =\n  let mut i = 0\n  let mut acc = 0\n  \
         while i < 3 do {{\n    let l = lazy {{ mark x }}\n    acc = acc + await l\n    i = i + 1\n  }}\n  acc\n"
    ));
    let make = ir.find("lazy_make").expect(&ir);
    let loop_at = ir.find("(control-loop ").expect(&ir);
    assert!(loop_at < make, "{ir}");

    let ir_ref = optimized(
        "module App exposing (..)\n\
         run : Unit ->{Async} Int\n\
         let run () =\n  let r = Ref.make 0\n  let mut i = 0\n  let mut acc = 0\n  \
         while i < 3 do {\n    let l = lazy { Ref.set r (Ref.get r + 1); Ref.get r }\n    \
         acc = acc + await l\n    i = i + 1\n  }\n  acc\n",
    );
    let make = ir_ref.find("lazy_make").expect(&ir_ref);
    let loop_at = ir_ref.find("(control-loop ").expect(&ir_ref);
    assert!(loop_at < make, "{ir_ref}");
}

/// Negative O4: a lazy using a service or typed error stays inside the loop.
#[test]
fn a_lazy_with_a_service_or_error_stays_in_the_loop() {
    let ir_svc = optimized(
        "module App exposing (..)\n\
         service Counter =\n  next : Unit ->{} Int\n\
         run : Unit ->{Counter, Async} Int\n\
         let run () =\n  let mut i = 0\n  let mut acc = 0\n  \
         while i < 3 do {\n    let l = lazy { Counter.next () }\n    acc = acc + await l\n    i = i + 1\n  }\n  acc\n",
    );
    let make = ir_svc.find("lazy_make").expect(&ir_svc);
    let loop_at = ir_svc.find("(control-loop ").expect(&ir_svc);
    assert!(loop_at < make, "{ir_svc}");

    let ir_err = optimized(
        "module App exposing (..)\n\
         type error Boom = Boom String\n\
         fail_now : Int ->{; Boom} Int\n\
         let fail_now _ = Fail.fail (Boom \"boom\")\n\
         run : Int ->{Async; Boom} Int\n\
         let run x =\n  let mut i = 0\n  let mut acc = 0\n  \
         while i < 3 do {\n    let l = lazy { fail_now x }\n    acc = acc + await l\n    i = i + 1\n  }\n  acc\n",
    );
    let make = ir_err.find("lazy_make").expect(&ir_err);
    let loop_at = ir_err.find("(control-loop ").expect(&ir_err);
    assert!(loop_at < make, "{ir_err}");
}

/// Negative O4: a lazy inside a runtime lambda created in a pure loop is NOT
/// hoisted out of the lambda into the pure loop.
#[test]
fn a_lazy_inside_a_lambda_in_a_pure_loop_is_not_hoisted() {
    let ir = optimized(&format!(
        "{PURE_WORK}make_thunk : Int -> Option (Unit ->{{Async}} Int)\n\
         let make_thunk x =\n  let mut i = 0\n  let mut out = None\n  \
         while i < 3 do {{\n    let f = fun () ->\n      let l = lazy {{ work x }}\n      await l\n    \
         out = Some f\n    i = i + 1\n  }}\n  out\n"
    ));
    let make = ir.find("lazy_make").expect(&ir);
    let loop_at = ir.find("(control-loop ").expect(&ir);
    assert!(loop_at < make, "{ir}");
}

/// Output identity gate: a loop with no lazy cell is completely untouched by
/// lazy hoisting.
#[test]
fn a_loop_without_lazy_is_unchanged() {
    let ir = optimized(&format!(
        "{PURE_WORK}run : Int -> Int\nlet run x =\n  let mut i = 0\n  let mut acc = 0\n  \
         while i < 3 do {{\n    acc = acc + work x\n    i = i + 1\n  }}\n  acc\n"
    ));
    assert!(!ir.contains("lazy_") && !ir.contains("$lazy"), "{ir}");
    assert!(ir.contains("(control-loop "), "{ir}");
}

/// O2b: a `let lazy` read once, in a branch, keeps no state: its `await`
/// runs the task.
#[test]
fn a_let_lazy_read_once_keeps_no_state() {
    let ir = optimized(&format!(
        "{MARK}run : Bool -> Unit ->{{Async}} Int\nlet run b () =\n  let lazy x = mark 7\n  \
         let k = 1\n  if b then await x + k else k\n"
    ));
    assert!(!ir.contains("let mut"), "{ir}");
    assert!(!ir.contains("Some"), "{ir}");
}

/// O2b leaves a state read twice.
#[test]
fn a_let_lazy_read_twice_keeps_its_state() {
    let ir = optimized(&format!(
        "{MARK}run : Bool -> Unit ->{{Async}} Int\nlet run b () =\n  let lazy x = mark 7\n  \
         if b then await x else 0 - await x\n"
    ));
    assert!(ir.contains("let mut x"), "{ir}");
}

/// O2b leaves a state read in a loop, which may read it again.
#[test]
fn a_let_lazy_read_in_a_loop_keeps_its_state() {
    let ir = optimized(&format!(
        "{MARK}run : Unit ->{{Async}} Int\nlet run () =\n  let lazy x = mark 7\n  let mut sum = 0\n  \
         for i <- [1, 2, 3] do sum = sum + i * await x\n  sum\n"
    ));
    assert!(ir.contains("let mut x"), "{ir}");
}

/// O2b leaves a state read after a branch that assigns, as LetMut binds its
/// variables again there.
#[test]
fn a_let_lazy_read_after_an_assigning_branch_keeps_its_state() {
    let ir = optimized(&format!(
        "{MARK}run : Bool -> Unit ->{{Async}} Int\nlet run b () =\n  let lazy x = mark 7\n  let mut n = 0\n  \
         if b then n = 1 else ()\n  n + await x\n"
    ));
    assert!(ir.contains("let mut x"), "{ir}");
}

/// O2b sees through the body of a `try`, which runs once.
#[test]
fn a_let_lazy_read_once_in_a_try_keeps_no_state() {
    let ir = optimized(&format!(
        "{MARK}type error Oops = Oops\nrisky : Int ->{{; Oops}} Int\n\
         let risky n = if n == 0 then Fail.fail Oops else n\n\
         run : Int ->{{Async}} Int\nlet run n =\n  let lazy x = risky n\n  \
         try await x with\n  | Oops -> 0\n"
    ));
    assert!(!ir.contains("let mut"), "{ir}");
    assert!(!ir.contains("Some"), "{ir}");
}
