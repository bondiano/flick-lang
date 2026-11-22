use super::fmt;

/// `orelse` keeps right-associative chains unparenthesized, makes mixed
/// grouping with `|>` and `<|` explicit, preserves record labels named
/// `orelse`, and is idempotent on a second formatting pass.
#[test]
fn formats_orelse_chains_mixed_pipes_and_record_labels_idempotently() {
    let src = "type alias Cfg = { host : String; orelse : Option Int }\nlet a = x orelse y orelse z\nlet b = (x orelse y) orelse z\nlet c = x |> find orelse default\nlet d = show <| opt orelse 0\nlet e = opt /* note */ orelse if ok then 1 else 2\nlet r = { host = \"a\"; orelse = Some 1 }\nlet v = r.orelse orelse 0\n";
    let first = fmt(src);
    assert_eq!(
        first,
        "type alias Cfg = { host : String; orelse : Option Int }\n\
         let a = x orelse y orelse z\n\
         let b = (x orelse y) orelse z\n\
         let c = (x |> find) orelse default\n\
         let d = show <| (opt orelse 0)\n\
         let e = opt /* note */ orelse if ok then 1 else 2\n\
         let r = { host = \"a\"; orelse = Some 1 }\n\
         let v = r.orelse orelse 0\n"
    );
    assert_eq!(fmt(&first), first, "formatting must be idempotent");
}
