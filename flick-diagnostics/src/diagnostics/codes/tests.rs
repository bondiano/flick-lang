use super::*;
use std::collections::HashSet;

#[test]
fn registry_codes_are_unique_well_formed_and_explained() {
    let mut seen = HashSet::new();
    for info in REGISTRY {
        let code = info.code.to_string();
        assert!(seen.insert(info.code), "{code} registered twice");
        assert!((1..=999).contains(&info.code.number), "{code} out of range");
        assert!(!info.title.is_empty(), "{code} has no title");
        assert_eq!(code.parse::<Code>(), Ok(info.code));
    }
}

#[test]
fn area_letters_are_distinct() {
    let letters: HashSet<_> = Area::ALL.iter().map(|a| a.letter()).collect();
    assert_eq!(letters.len(), Area::ALL.len());
}

#[test]
fn a_plugins_code_shows_under_its_package() {
    let code = plugin("std", "L001").expect("well formed");
    assert_eq!(code.to_string(), "std/L001");
    assert_eq!(plugin("std", "L001"), Some(code));
    assert_ne!(plugin("other", "L001"), Some(code));
    for bad in [
        "",
        "L01",
        "l001",
        "L0012",
        "FLICK-L001",
        "../001",
        "ABCDE001",
        "Lé01",
        "é",
    ] {
        assert_eq!(plugin("std", bad), None, "{bad}");
    }
}

#[test]
fn display_pads_to_three_digits() {
    assert_eq!(S001.to_string(), "FLICK-S001");
    assert_eq!(lookup("FLICK-X001").map(|i| i.code), None);
}
