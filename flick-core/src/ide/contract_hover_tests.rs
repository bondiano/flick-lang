use super::tests::fixture_of;
use crate::diagnostics::Diagnostic;

#[test]
fn contract_keyword_hover_explains_unchecked_status_and_flick_prove() {
    let source = "module App.Inventory exposing (consume)

consume : Int -> Int -> Int
let consume available used
where
  require (0 <= used && used <= available)
  ensure (fun remaining -> 0 <= remaining)
=
  available
";
    let f = fixture_of(&[("src/App/Inventory.fl", source)]);
    let ide = f.ide();
    // Mode::Analyze type-checks contracts without proving them, so even a false
    // postcondition produces no proof error in editor analysis.
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Inventory");
    let req_at = f.at("App.Inventory", "require", 0);
    let (req_md, req_range) = ide.hover(file, req_at + 2).expect("require hover");
    assert_eq!(req_range, (req_at, req_at + 7));
    assert!(req_md.contains("require …"), "{req_md}");
    assert!(req_md.contains("flick check"), "{req_md}");
    assert!(req_md.contains("flick prove"), "{req_md}");

    let ens_at = f.at("App.Inventory", "ensure", 0);
    let (ens_md, ens_range) = ide.hover(file, ens_at + 2).expect("ensure hover");
    assert_eq!(ens_range, (ens_at, ens_at + 6));
    assert!(ens_md.contains("ensure …"), "{ens_md}");
    assert!(ens_md.contains("flick check"), "{ens_md}");
    assert!(ens_md.contains("flick prove"), "{ens_md}");
}
