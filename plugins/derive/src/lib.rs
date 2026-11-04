//! The derives of std's plugin for the witnesses of the Prelude:
//! `deriving Schema`, `deriving Url` and `deriving Form` write `T$schema`,
//! `T$url`, `T$form` and `T$fields` in Flick, with the combinators of
//! `Schema`, `Route` and `Form`. `deriving HttpApi` writes `T$endpoints`
//! and `T$api` with those of `HttpApi`. What they cannot derive is
//! `std/D001`, `std/D009`, `std/D010` or `std/D011`.

mod endpoint;
mod form;
mod http_api;
mod module;
mod schema;
mod ty;
mod url;

use flick_plugin::{Arena, Builder, DeriveTarget, Diagnostic};

use module::Module;
use schema::{Fail, Gen};

/// The classes these derives make: the full names `derive` is called with.
pub const CLASSES: [&str; 4] = [
    "Prelude.Schema",
    "Prelude.Url",
    "Prelude.Form",
    "HttpApi.Derive.HttpApi",
];

/// The code `class` derives for `target`, a type of the module whose types
/// are `module`.
pub fn derive(
    target: &DeriveTarget,
    class: &str,
    module: &[DeriveTarget],
) -> Result<Arena, Vec<Diagnostic>> {
    let whole = target.decl.whole();
    let refuse = |code: &str, (path, why): Fail| {
        vec![Diagnostic::error(format!("`{path}`: {why}"), whole).with_code(code)]
    };
    let mut b = Builder::new(whole);
    if class == "HttpApi.Derive.HttpApi" {
        let made = http_api::declarations(&mut b, target)
            .map_err(|(at, why)| vec![Diagnostic::error(why, at).with_code("D011")])?;
        return Ok(b.finish(made));
    }
    let module = Module::new(module);
    let local = module::read(target).map_err(|why| refuse("D001", (target.name.clone(), why)))?;
    let made = match class {
        "Prelude.Schema" => Gen::new(&module, &local)
            .declarations(&mut b, &local)
            .map_err(|f| refuse("D001", f))?,
        "Prelude.Url" => {
            url::declarations(&mut b, &module, &local).map_err(|f| refuse("D009", f))?
        }
        "Prelude.Form" => {
            form::declarations(&mut b, &module, &local).map_err(|f| refuse("D010", f))?
        }
        other => {
            let message = format!("`{other}` has no derive in std");
            return Err(vec![Diagnostic::error(message, whole)]);
        }
    };
    Ok(b.finish(made))
}
