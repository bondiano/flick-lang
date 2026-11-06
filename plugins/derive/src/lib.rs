//! The derives of std's plugin for the witnesses of the Prelude:
//! `deriving Schema`, `deriving Url` and `deriving Form` write `T$schema`,
//! `T$url`, `T$form` and `T$fields` in Flick, with the combinators of
//! `Schema`, `Route` and `Form`. `deriving HttpApi` writes `T$endpoints`
//! and `T$api` with those of `HttpApi`. What they cannot derive is
//! `std/D001`, `std/D009`, `std/D010` or `std/D011`.

pub mod codec;
mod endpoint;
mod form;
mod http_api;
mod module;
mod schema;
pub mod server_fn;
mod ty;
mod url;

use flick_plugin::{Arena, Block, Builder, DeriveTarget, Diagnostic};

use module::{Abi, Form, Module};
use schema::{Fail, Gen};

/// The classes these derives make: the full names `derive` is called with.
pub const CLASSES: [&str; 4] = [
    "Prelude.Schema",
    "Prelude.Url",
    "Prelude.Form",
    "HttpApi.Derive.HttpApi",
];

/// The code `class` derives for `target`, a type of the module whose types
/// are `module`, with the arguments of the `deriving`.
pub fn derive(
    target: &DeriveTarget,
    class: &str,
    args: Option<&Block>,
    module: &[DeriveTarget],
) -> Result<Arena, Vec<Diagnostic>> {
    let whole = target.decl.whole();
    let refuse = |code: &str, (path, why): Fail| {
        vec![Diagnostic::error(format!("`{path}`: {why}"), whole).with_code(code)]
    };
    // what a value of a GADT is depends on its type, which no derive here reads
    if let Some(case) = target.cases.iter().find(|c| c.refined || c.constrained) {
        let code = match class {
            "Prelude.Url" => "D009",
            "Prelude.Form" => "D010",
            "HttpApi.Derive.HttpApi" => "D011",
            _ => "D001",
        };
        let why = if case.constrained {
            "holds dictionaries"
        } else {
            "names the type it builds"
        };
        let derive = class.rsplit('.').next().unwrap_or(class);
        let fail = (
            target.name.clone(),
            format!(
                "`deriving {derive}` describes ordinary constructors, and `{}` {why}, as a constructor of a GADT",
                case.name
            ),
        );
        return Err(refuse(code, fail));
    }
    let mut b = Builder::new(whole);
    if class == "HttpApi.Derive.HttpApi" {
        let made = http_api::declarations(&mut b, target)
            .map_err(|(at, why)| vec![Diagnostic::error(why, at).with_code("D011")])?;
        return Ok(b.finish(made));
    }
    let module = Module::new(module);
    let local = module::read(target).map_err(|why| refuse("D001", (target.name.clone(), why)))?;
    let made = match class {
        "Prelude.Schema" => {
            let mut schemas = Gen::new(&module, &local);
            schemas.wire_tag = wire_tag(args, &local.form).map_err(|why| {
                vec![Diagnostic::error(why, args.map_or(whole, Block::whole)).with_code("D001")]
            })?;
            (schemas.declarations(&mut b, &local)).map_err(|f| refuse("D001", f))?
        }
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

/// Whether `deriving Schema { wire = "tag" }` puts an enum on the wire as a
/// variant type, `{"_tag": "Get"}`, rather than as its literals.
fn wire_tag(args: Option<&Block>, form: &Form) -> Result<bool, String> {
    let Some(args) = args else {
        return Ok(false);
    };
    let words: Vec<&str> = (args.tokens.iter())
        .map(|t| args.text_of(t.span))
        .filter(|w| !matches!(*w, "{" | "}"))
        .collect();
    let enumeration = matches!(
        form,
        Form::Adt {
            abi: Abi::Literal { .. },
            ..
        }
    );
    match words[..] {
        [] => Ok(false),
        ["wire", "=", "\"tag\""] if enumeration => Ok(true),
        ["wire", "=", "\"tag\""] => {
            Err("`wire` is an argument of an enum's `deriving Schema`".into())
        }
        _ => Err("`deriving Schema` takes one argument, an enum's `{ wire = \"tag\" }`".into()),
    }
}
