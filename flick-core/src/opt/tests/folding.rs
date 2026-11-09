use super::release;

#[test]
fn show_of_a_literal_folds_into_the_string() {
    let js = release(
        "module App exposing (main)\n\n\
         main : Unit ->{; AppError} Unit\n\
         let main () =\n  let test = 2 + 3\n  Platform.stdout_line (\"Hello from Flick\" ++ show test)\n",
    );
    assert!(js.contains("stdout_line(\"Hello from Flick5\")"), "{js}");
    assert!(!js.contains("$show"), "{js}");
}

#[test]
fn an_interpolation_of_constants_folds_into_one_string() {
    let js = release(
        "module App exposing (shown, label)\n\n\
         shown : String\nlet shown = \"a \\{\"b\"} \\{2 + 3} \\{1.5} \\{true} \\{-0.0}!\"\n\n\
         label : Int -> String\nlet label n = \"[\\{\"x\"}\\{1}] \\{n}\"\n",
    );
    assert!(
        js.contains(r#"export const shown = "a b 5 1.5 true -0!";"#),
        "{js}"
    );
    // the constants before a hole join into one part
    assert!(js.contains(r#""[x1] " + "#), "{js}");
    assert!(!js.contains("$display(\"x\")"), "{js}");
}

#[test]
fn show_folds_as_the_runtime_spells_it() {
    let js = release(
        "module App exposing (shown, fraction)\n\n\
         shown : String\nlet shown = show true ++ show \"a\\n\\\"\" ++ show 12n ++ show (-0.0)\n\n\
         fraction : String\nlet fraction = show (10.0 + 10.01) ++ \" \" ++ show (1000000000.0 * 1000000000000.0) \
         ++ \" \" ++ show (1.0 / 10000000.0) ++ \" \" ++ show 1658206780088562.25\n",
    );
    assert!(
        js.contains(r#"export const shown = "true\"a\\n\\\"\"12n-0";"#),
        "{js}"
    );
    // an exact tie spells the even digit, as JS does
    assert!(
        js.contains(
            r#"export const fraction = "20.009999999999998 1e+21 1e-7 1658206780088562.2";"#
        ),
        "{js}"
    );
}

#[test]
fn non_finite_floats_fold_to_the_js_globals() {
    let js = release(
        "module App exposing (shown, named, sum, neg)\n\n\
         shown : String\nlet shown = show (1.0 / 0.0) ++ show (-1.0 / 0.0) ++ show (0.0 / 0.0)\n\n\
         named : String\nlet named = show Float.infinity ++ show Float.nan\n\n\
         sum : Float\nlet sum = Float.infinity + 1.0\n\n\
         neg : Float\nlet neg = 0.0 - 1.0 / 0.0\n",
    );
    assert!(
        js.contains(r#"export const shown = "Infinity-InfinityNaN";"#),
        "{js}"
    );
    assert!(
        js.contains(r#"export const named = "InfinityNaN";"#),
        "{js}"
    );
    assert!(js.contains("export const sum = Infinity;"), "{js}");
    assert!(js.contains("export const neg = -Infinity;"), "{js}");
}

#[test]
fn a_private_literal_constant_inlines_at_its_uses() {
    let js = release(
        "module App exposing (label)\n\n\
         size : Float\nlet size = 128 * 2\n\n\
         label : Unit -> String\nlet label () = \"w\" ++ show size\n",
    );
    assert!(js.contains("\"w256\""), "{js}");
    assert!(!js.contains("const size"), "{js}");
}

#[test]
fn combinators_of_a_for_fuse_into_its_loop() {
    let js = release(
        "module App exposing (chain, listed, firsts, nested)\n\n\
         chain : List Float -> List Float\n\
         let chain xs = Seq.to_list (Seq.map (fun x -> x * 10) (Seq.filter (fun x -> x > 1) (for { x <- xs } yield x)))\n\n\
         listed : List Float -> List Float\n\
         let listed xs = List.map (fun y -> y + 1) (List.filter (fun y -> y > 2) [for x <- xs yield x * 2])\n\n\
         firsts : Seq Int\nlet firsts = Seq.take 3 (Seq.flat_map (fun x -> Seq.of_list [x, x]) (for { x <- Seq.from 1 } yield x))\n\n\
         nested : List Float -> List (Float, Float)\n\
         let nested xs = [for { x <- for { y <- xs; if y > 0 } yield y * 2; z <- [1, 2] } yield (x, z)]\n",
    );
    insta::assert_snapshot!(js);
}

#[test]
fn a_known_function_fuses_and_a_sync_for_is_left() {
    let js = release(
        "module App exposing (doubled, read)\n\n\
         double : Float -> Float\nlet double x = x * 2\n\n\
         doubled : List Float -> List Float\nlet doubled xs = List.map double [for x <- xs yield x + 1]\n\n\
         count : Atom.Writable Float\nlet count = Atom.make 1\n\n\
         read : Unit ->{Atom.Read;} List Float\n\
         let read () = Seq.to_list (Seq.map double (for { x <- [1, 2] } yield x + Atom.value count))\n",
    );
    assert!(js.contains("$t1.push((x + 1) * 2);"), "{js}");
    // its steps read the services it captured, so they stay in its generator
    assert!(js.contains("$m_Seq.map(double)($rt.seqIn("), "{js}");
}

#[test]
fn a_strict_for_that_may_throw_is_built_before_its_consumer() {
    let js = release(
        "module App exposing (first, powers, sums)\n\n\
         first : Option BigInt\n\
         let first = Seq.first (for x <- [for y <- [1n, 0n] yield 2n ** (0n - y)] yield x)\n\n\
         powers : List BigInt\n\
         let powers = List.map (fun x -> 2n ** (0n - x)) [for y <- [1n, 0n] yield 2n ** (0n - y)]\n\n\
         sums : List BigInt -> List BigInt\n\
         let sums xs = List.map (fun x -> 2n ** x) [for y <- xs yield y + 1n]\n",
    );
    // the negative powers run, in order, before anything reads the list
    assert_eq!(js.matches(".push(2n ** (0n - y))").count(), 2, "{js}");
    // a producer that cannot throw still fuses with a callback that may
    assert!(js.contains(".push(2n ** (y + 1n))"), "{js}");
}

#[test]
fn a_numeric_function_is_copied_for_the_instances_it_is_called_with() {
    let js = release(
        "module App exposing (both)\n\n\
         import Numeric exposing (Numeric)\n\n\
         scale : Numeric a => a -> a -> a\nlet scale k x = k * x + 1\n\n\
         both : (Int, Float) -> (Int, Float)\nlet both (i, f) = (scale 3 i, scale 2 f)\n",
    );
    assert!(
        js.contains("scale$Numeric_Int = (k) => (x) => Math.imul(k, x) + 1 | 0"),
        "{js}"
    );
    assert!(
        js.contains("scale$Numeric_Float = (k) => (x) => k * x + 1"),
        "{js}"
    );
    // inlined, the copies leave plain arithmetic at the calls
    assert!(
        js.contains("return [Math.imul(3, i) + 1 | 0, 2 * f + 1];"),
        "{js}"
    );
}

#[test]
fn a_numeric_function_of_another_module_is_copied_too() {
    let js = release(
        "module App exposing (total)\n\n\
         total : List Int -> Int\nlet total xs = List.sum xs\n",
    );
    assert!(js.contains("List$Core$sum$Numeric_Int"), "{js}");
    assert!(js.contains("total + x | 0"), "{js}");
    assert!(!js.contains("$i_Numeric_Int"), "{js}");
}

/// Folding writes a float as JS does: the corpus of the Go runtime's test,
/// so both targets spell a number the same whether folded or shown at run
/// time.
#[test]
fn folded_floats_are_written_as_js_writes_them() {
    let corpus = include_str!("../../../runtime-go/flickrt/testdata/numbers.txt");
    let mut cases = 0;
    for line in corpus.lines().filter(|l| !l.starts_with('#')) {
        let (bits, want) = line.split_once(' ').expect("bits and text");
        let x = f64::from_bits(u64::from_str_radix(bits, 16).expect("hex bits"));
        assert_eq!(super::fold::js_number(x), want, "{bits}");
        cases += 1;
    }
    assert!(cases > 300, "{cases} cases");
}
