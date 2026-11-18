use std::fmt::Write;

use crate::ir::tests::check_text;
use crate::types::ty::Type;

/// The schemes of the foreign values `p0`, `p1`, … of types `signatures`.
fn schemes(signatures: &[&str]) -> Vec<Type> {
    let mut text = String::from("module Probe exposing (..)\n");
    for (i, signature) in signatures.iter().enumerate() {
        let _ = write!(
            text,
            "p{i} : {signature}\nforeign effect p{i} from \"./x.mjs\" \"x\"\n  retention call\n"
        );
    }
    let (_, interface) = check_text("Probe", &text).expect("the signatures check");
    (0..signatures.len())
        .map(|i| {
            let name = format!("p{i}");
            let export = interface.exports.iter().find(|e| e.entity.name == name);
            export.and_then(|e| e.scheme.clone()).expect("a scheme")
        })
        .collect()
}

/// The variables the outer `forall` of `scheme` binds.
fn outer(scheme: &Type) -> Vec<&str> {
    match scheme {
        Type::Forall { vars, .. } => vars.iter().map(|v| v.name.as_str()).collect(),
        _ => Vec::new(),
    }
}

/// Each pair, the shorthand and the binder written on its callback, is one type.
#[test]
fn a_callback_binds_the_rows_only_it_mentions() {
    let pairs = [
        // map
        (
            "(a^v -> b)^c -> (List a)^d -> (List b)^c",
            "(forall v. a^v -> b)^c -> (List a)^d -> (List b)^c",
        ),
        // a class method's form, with its outer binders written
        (
            "forall c d a b. (a^v -> b)^c -> (List a)^d -> (List b)^(c + d)",
            "forall c d a b. (forall v. a^v -> b)^c -> (List a)^d -> (List b)^(c + d)",
        ),
        // flat_map
        (
            "(a^v -> (List b)^(c + v))^c -> (List a)^d -> (List b)^(c + d)",
            "(forall v. a^v -> (List b)^(c + v))^c -> (List a)^d -> (List b)^(c + d)",
        ),
        // map2
        (
            "(a^u -> b^v -> x)^c -> (Option a)^d -> (Option b)^d -> (Option x)^(c + d)",
            "(forall u v. a^u -> b^v -> x)^c -> (Option a)^d -> (Option b)^d -> (Option x)^(c + d)",
        ),
        // Validation's map2: `v` bounds a parameter of the callback's result
        (
            "(e -> e -> e) -> (a^u -> (b^v -> x)^(c + u))^c -> (Option a)^d -> (Option x)^(c + d)",
            "(e -> e -> e) -> (forall u v. a^u -> (b^v -> x)^(c + u))^c -> (Option a)^d -> (Option x)^(c + d)",
        ),
        // local
        (
            "(r^v -> r^(c + v))^c -> (m a)^d -> (m a)^(c + d)",
            "(forall v. r^v -> r^(c + v))^c -> (m a)^d -> (m a)^(c + d)",
        ),
        // state
        (
            "(s^v -> (a, s)^(c + v))^c -> (m a)^c",
            "(forall v. s^v -> (a, s)^(c + v))^c -> (m a)^c",
        ),
        // a callback with a region binder of its own keeps it, after the row
        (
            "(forall t. a^{t} -> b^v -> x)^c -> x",
            "(forall v t. a^{t} -> b^v -> x)^c -> x",
        ),
    ];
    for (short, long) in pairs {
        let got = schemes(&[short, long]);
        assert_eq!(got[0], got[1], "{short}\n  is not\n{long}");
    }
}

/// A row mentioned outside the callback, only on its qualifier, or by no
/// parameter of it keeps its meaning: quantified over the whole signature.
#[test]
fn a_row_the_callback_does_not_own_stays_at_the_top() {
    let cases = [
        // `Ref.modify`: `c` qualifies the reference too, `x` the result
        (
            "(List a)^c -> (a^c -> (b^x, a^c))^d -> b^x",
            &["c", "x"][..],
        ),
        // `Rpc.serve`: `c` is the qualifier of the callback itself
        ("Int -> (hs^c -> Int)^c", &["c"]),
        ("(hs^c -> Int)^c -> Int", &["c"]),
        // a region is not a capture row
        ("(a^{t} -> b)^c ->{} Int", &["t"]),
        // what the callback returns is the caller's choice
        ("(a -> b^v)^c -> Int", &["v"]),
        // two callbacks with the same row
        ("(a^v -> b)^c -> (a^v -> b)^d -> Int", &["v"]),
    ];
    for (signature, names) in cases {
        let got = schemes(&[signature]);
        for name in names {
            assert!(
                outer(&got[0]).contains(name),
                "`{name}` of {signature} left the top: {:?}",
                got[0]
            );
        }
    }
}
