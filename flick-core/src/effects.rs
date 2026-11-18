//! Runtime routing is shared with the isolated emitter; compiler diagnostics stay here.

pub use flick_backend_support::effects::*;

use crate::diagnostics::{Diagnostic, codes};
use crate::ir::{self, ItemKind};
use crate::types::OpClass;

/// P012 at each handler of a `control` operation on the Effect adapter, which
/// does not run them; Flick's own runtime and Go do.
pub fn control_on_js(module: &ir::Module, diagnostics: &mut Vec<Diagnostic>) {
    fn walk(e: &ir::Expr, diagnostics: &mut Vec<Diagnostic>) {
        if let ir::ExprKind::Handle(h) = &e.kind {
            let control = h.effects.iter().any(|handled| {
                handled
                    .operations
                    .iter()
                    .any(|o| o.class == OpClass::Control)
            });
            if control {
                let message =
                    "the Effect adapter (`effect = \"effect\"`) does not run handlers of \
                     `control` operations; Flick's own runtime (`effect = \"rt\"`) and Go do"
                        .to_owned();
                diagnostics.push(Diagnostic::error(codes::P012, message, e.span));
            }
        }
        e.each_child(&mut |c| walk(c, diagnostics));
    }
    for item in &module.items {
        match &item.kind {
            ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => {
                walk(body, diagnostics);
            }
            ItemKind::Instance { methods, .. } => {
                for (_, m) in methods {
                    walk(m, diagnostics);
                }
            }
            ItemKind::Foreign(_) | ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::tests::check_text;

    #[test]
    fn a_control_handler_names_the_runtimes_that_run_it() {
        let src = "module App.Main exposing (collect)\n\neffect Emit a =\n  control emit : a ->{} Unit\n\ncollect : forall a r e. (Unit ->{Emit a | r; e} Unit) ->{r; e} List a\nlet collect body =\n  try body () with\n  | Emit.emit x -> [x] ++ resume ()\n  | return () -> []\n";
        let (ir, _) = check_text("App.Main", src).expect("the source checks");
        let mut diagnostics = Vec::new();
        control_on_js(&ir, &mut diagnostics);
        let [d] = diagnostics.as_slice() else {
            panic!("one diagnostic: {diagnostics:?}")
        };
        assert_eq!(d.code, codes::P012);
        assert_eq!(
            d.message,
            "the Effect adapter (`effect = \"effect\"`) does not run handlers of `control` operations; Flick's own runtime (`effect = \"rt\"`) and Go do"
        );
        let span = d.span.expect("a span");
        assert!(
            src[span.start as usize..].starts_with("try body () with"),
            "{span:?}"
        );
    }
}
