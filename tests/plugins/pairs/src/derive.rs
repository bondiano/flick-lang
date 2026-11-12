//! `deriving Pairs.Arbitrary`: `instance Pairs.Arbitrary T`, whose
//! `arbitrary` is the first constructor of `T`, each field `Pairs.arbitrary`;
//! of a type over another, what its `make` gives of `Pairs.arbitrary`.

use flick_plugin::{Arena, Builder, DeriveTarget, Diagnostic, Expr, Name, NodeId, Ty, flick};

pub fn derive(target: &DeriveTarget, class: &str) -> Result<Arena, Vec<Diagnostic>> {
    let whole = target.decl.whole();
    let refuse = |message: String| vec![Diagnostic::error(message, whole)];
    if class != "Pairs.Arbitrary" {
        return Err(refuse(format!("`{class}` has no derive in pairs")));
    }
    let case = (target.cases.first())
        .ok_or_else(|| refuse(format!("`{}` has no constructor", target.name)))?;
    let mut b = Builder::new(whole);
    if target.wraps.is_some() {
        let head = b.ty(Ty::Name(target.name.clone()));
        let make = format!("{}.make", target.name);
        let instance = flick!(b, instance Pairs.Arbitrary #head {
            let arbitrary = #(Name(&make)) Pairs.arbitrary
        });
        return Ok(b.finish(instance));
    }
    let fields: Vec<NodeId> = (case.fields.iter())
        .map(|_| b.expr(Expr::Name("Pairs.arbitrary".into())))
        .collect();
    let con = b.expr(Expr::Name(case.name.clone()));
    let value = if fields.is_empty() {
        con
    } else {
        b.expr(Expr::Apply {
            function: con,
            arguments: fields,
        })
    };
    let head = b.ty(Ty::Name(target.name.clone()));
    let instance = flick!(b, instance Pairs.Arbitrary #head { let arbitrary = #value });
    Ok(b.finish(instance))
}
