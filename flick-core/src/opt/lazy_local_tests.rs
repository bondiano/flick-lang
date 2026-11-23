use super::lazy_tests::{MARK, PURE_WORK, optimized};

/// Counts exact occurrences of synchronized `lazy_make` (not `lazy_make_local`).
fn sync_makes(ir: &str) -> usize {
    ir.match_indices("lazy_make")
        .filter(|&(i, _)| !ir[i..].starts_with("lazy_make_local"))
        .count()
}

/// Counts exact occurrences of unsynchronized `lazy_make_local`.
fn local_makes(ir: &str) -> usize {
    ir.matches("lazy_make_local").count()
}

/// O3: a lazy forced only in its own fiber's block uses `lazy_make_local`.
#[test]
fn a_non_escaping_lazy_uses_a_local_cell() {
    let ir = optimized(&format!(
        "{MARK}run : Unit ->{{Async}} Int\nlet run () =\n  let l = lazy {{ mark 7 }}\n  await l + await l\n"
    ));
    assert_eq!(local_makes(&ir), 1, "{ir}");
    assert_eq!(sync_makes(&ir), 0, "{ir}");
}

/// O3: non-escaping lazies in branches, loops, abort handlers, inlined helpers,
/// and per-call lambda bodies all rewrite to `lazy_make_local`, including an
/// O4-hoisted loop-invariant lazy.
#[test]
fn non_escaping_lazies_in_control_flow_and_inlined_helpers_use_local_cells() {
    let ir_branch_loop = optimized(&format!(
        "{MARK}run : Bool ->{{Async}} Int\nlet run flag =\n  let l = lazy {{ mark 7 }}\n  \
         let mut i = 0\n  let mut acc = 0\n  while i < 3 do {{\n    \
         acc = acc + (if flag then await l else 0)\n    i = i + 1\n  }}\n  acc + await l\n"
    ));
    assert_eq!(local_makes(&ir_branch_loop), 1, "{ir_branch_loop}");
    assert_eq!(sync_makes(&ir_branch_loop), 0, "{ir_branch_loop}");

    let ir_abort = optimized(
        "module App exposing (..)\n\
         effect Stop =\n  abort stop : forall a. String ->{} a\n\
         step : Ref Int ->{Stop} Int\n\
         let step r =\n  Ref.set r (Ref.get r + 1)\n  if Ref.get r < 2 then Stop.stop \"no\" else 7\n\
         run : Unit ->{Async} String\n\
         let run () =\n  let r = Ref.make 0\n  let l = lazy { step r }\n  \
         let a = try show (await l) with | Stop.stop s -> s\n  \
         let b = try show (await l) with | Stop.stop s -> s\n  a ++ b\n",
    );
    assert_eq!(local_makes(&ir_abort), 1, "{ir_abort}");
    assert_eq!(sync_makes(&ir_abort), 0, "{ir_abort}");

    let ir_inlined = optimized(&format!(
        "{MARK}pass_through : Lazy r e Int -> Lazy r e Int\nlet pass_through l = l\n\
         run : Unit ->{{Async}} Int\nlet run () =\n  let l = lazy {{ mark 7 }}\n  \
         await (pass_through l) + await l\n"
    ));
    assert_eq!(local_makes(&ir_inlined), 1, "{ir_inlined}");
    assert_eq!(sync_makes(&ir_inlined), 0, "{ir_inlined}");

    let ir_beta = optimized(&format!(
        "{MARK}run : Unit ->{{Async}} Int\nlet run () =\n  let l = lazy {{ mark 7 }}\n  \
         (fun (x : Lazy {{}} none Int) -> await x + await x) l\n"
    ));
    assert_eq!(local_makes(&ir_beta), 1, "{ir_beta}");
    assert_eq!(sync_makes(&ir_beta), 0, "{ir_beta}");

    let ir_lambda_body = optimized(&format!(
        "{MARK}make_runner : Unit -> (Unit ->{{Async}} Int)\nlet make_runner () =\n  \
         fun () ->\n    let l = lazy {{ mark 7 }}\n    await l + await l\n"
    ));
    assert_eq!(local_makes(&ir_lambda_body), 1, "{ir_lambda_body}");
    assert_eq!(sync_makes(&ir_lambda_body), 0, "{ir_lambda_body}");

    let ir_hoisted = optimized(&format!(
        "{PURE_WORK}run : Int ->{{Async}} Int\nlet run x =\n  let mut i = 0\n  let mut acc = 0\n  \
         while i < 3 do {{\n    let l = lazy {{ work x }}\n    acc = acc + await l\n    i = i + 1\n  }}\n  acc\n"
    ));
    let make = ir_hoisted.find("lazy_make_local").expect(&ir_hoisted);
    let loop_at = ir_hoisted.find("(control-loop ").expect(&ir_hoisted);
    assert!(make < loop_at, "{ir_hoisted}");
    assert_eq!(local_makes(&ir_hoisted), 1, "{ir_hoisted}");
    assert_eq!(sync_makes(&ir_hoisted), 0, "{ir_hoisted}");
}

/// Negative O3: a lazy captured in a `task`, closure, local function, lazy
/// generator, or handler operation arm stays synchronized (`lazy_make`).
#[test]
fn escaping_lazies_into_tasks_closures_and_handlers_stay_synchronized() {
    let ir_task = optimized(&format!(
        "{MARK}run : Unit ->{{Async}} (Int, Int)\nlet run () =\n  let l = lazy {{ mark 7 }}\n  \
         parallel (task {{ await l }}) (task {{ await l }})\n"
    ));
    assert_eq!(sync_makes(&ir_task), 1, "{ir_task}");
    assert_eq!(local_makes(&ir_task), 0, "{ir_task}");

    let ir_closure = optimized(&format!(
        "{MARK}run : Unit ->{{}} (Unit ->{{Async}} Int)\nlet run () =\n  let l = lazy {{ mark 7 }}\n  \
         fun () -> await l\n"
    ));
    assert_eq!(sync_makes(&ir_closure), 1, "{ir_closure}");
    assert_eq!(local_makes(&ir_closure), 0, "{ir_closure}");

    let ir_local_fun = optimized(&format!(
        "{MARK}run : Unit ->{{Async}} Int\nlet run () =\n  let l = lazy {{ mark 7 }}\n  \
         go : Int ->{{Async}} Int\n  let rec go n = if n <= 0 then 0 else await l + go (n - 1)\n  go 2\n"
    ));
    assert_eq!(sync_makes(&ir_local_fun), 1, "{ir_local_fun}");
    assert_eq!(local_makes(&ir_local_fun), 0, "{ir_local_fun}");

    let ir_seq = optimized(&format!(
        "{MARK}run : Unit ->{{}} Seq (Eff {{Async}} none Int)\nlet run () =\n  \
         let l = lazy {{ mark 7 }}\n  for _ <- [1, 2] yield task {{ await l }}\n"
    ));
    assert_eq!(sync_makes(&ir_seq), 1, "{ir_seq}");
    assert_eq!(local_makes(&ir_seq), 0, "{ir_seq}");

    let ir_handler_arm = optimized(&format!(
        "{MARK}effect Ask =\n  ask : Unit ->{{Async}} Int\n\
         run : Unit ->{{Async}} Int\nlet run () =\n  let l = lazy {{ mark 7 }}\n  \
         try Ask.ask () + Ask.ask () with\n  | Ask.ask () -> await l\n"
    ));
    assert_eq!(sync_makes(&ir_handler_arm), 1, "{ir_handler_arm}");
    assert_eq!(local_makes(&ir_handler_arm), 0, "{ir_handler_arm}");
}

/// Negative O3: a lazy placed into a record, constructor, tuple, list, `Ref`,
/// `let mut`, return value, or non-inlined call stays synchronized (`lazy_make`).
#[test]
fn escaping_lazies_into_data_refs_returns_and_calls_stay_synchronized() {
    let cases = [
        (
            "record",
            format!(
                "{MARK}run : Unit ->{{}} {{ cell : Lazy {{}} none Int }}\n\
                 let run () =\n  let l = lazy {{ mark 7 }}\n  {{ cell = l }}\n"
            ),
        ),
        (
            "constructor",
            format!(
                "{MARK}run : Unit ->{{}} Option (Lazy {{}} none Int)\n\
                 let run () =\n  let l = lazy {{ mark 7 }}\n  Some l\n"
            ),
        ),
        (
            "tuple",
            format!(
                "{MARK}run : Unit ->{{}} (Lazy {{}} none Int, Int)\n\
                 let run () =\n  let l = lazy {{ mark 7 }}\n  (l, 1)\n"
            ),
        ),
        (
            "list",
            format!(
                "{MARK}run : Unit ->{{}} List (Lazy {{}} none Int)\n\
                 let run () =\n  let l = lazy {{ mark 7 }}\n  [l]\n"
            ),
        ),
        (
            "ref",
            format!(
                "{MARK}run : Unit ->{{Async}} Int\n\
                 let run () =\n  let r : Ref (Option (Lazy {{}} none Int)) = Ref.make None\n  \
                 let l = lazy {{ mark 7 }}\n  Ref.set r (Some l)\n  \
                 match Ref.get r with\n  | Some c -> await c\n  | None -> 0\n"
            ),
        ),
        (
            "let mut",
            format!(
                "{MARK}run : Bool ->{{Async}} Int\n\
                 let run flag =\n  let l = lazy {{ mark 7 }}\n  let m = lazy {{ mark 8 }}\n  \
                 let mut cur = l\n  if flag then cur = m else ()\n  await cur\n"
            ),
        ),
        (
            "return",
            format!(
                "{MARK}run : Unit ->{{}} Lazy {{}} none Int\n\
                 let run () =\n  let l = lazy {{ mark 7 }}\n  l\n"
            ),
        ),
        (
            "non-inlined call",
            format!(
                "{MARK}consume : Int -> Lazy {{}} none Int ->{{Async}} Int\n\
                 let rec consume n l = if n <= 0 then 0 else await l + consume (n - 1) l\n\
                 run : Unit ->{{Async}} Int\n\
                 let run () =\n  let l = lazy {{ mark 7 }}\n  consume 2 l\n"
            ),
        ),
    ];
    for (label, src) in cases {
        let ir = optimized(&src);
        assert!(
            sync_makes(&ir) >= 1,
            "{label}: expected lazy_make in:\n{ir}"
        );
        assert_eq!(
            local_makes(&ir),
            0,
            "{label}: unexpected lazy_make_local in:\n{ir}"
        );
    }
}
