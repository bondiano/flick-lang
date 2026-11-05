//! The code `server fn` and `server name` stand for, and what is wrong
//! with the ones that stand for none.

use flick_core::plugin::testing::expand;
use flick_plugin::{Arena, Block, Compiler, Diagnostic, Plugin};
use flick_plugin_derive::server_fn;

struct Server;

impl Plugin for Server {
    fn collect(&self, _: &[Block], _: &mut dyn Compiler) -> Vec<u8> {
        Vec::new()
    }

    fn expand(
        &self,
        block: &Block,
        _: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        server_fn::expand(block, compiler)
    }
}

/// What the one block of a module with `body` stands for.
fn one(body: &str) -> Result<String, Vec<String>> {
    let source = format!(
        "module App.Shop exposing (..)\n\nimport Server exposing (plugin server)\n\n{body}"
    );
    let mut found = expand(&Server, &source);
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

fn errors(body: &str) -> String {
    one(body).expect_err("errors").join("\n")
}

#[test]
fn a_server_fn_is_its_body_its_procedure_its_call_and_what_serves_it() {
    let code = one(
        "/// Up to three.\nplace : Order ->{Server.Server; OutOfStock} String\nserver fn place order =\n  let item = order.item\n  item\n",
    );
    assert_eq!(
        code.as_deref(),
        Ok("place$impl : (Order ->{Server.Server; OutOfStock} String)
let place$impl order = (let item = order.item
  item)
place$procedure : (Server.Derive.Procedure Order {OutOfStock} String)
let place$procedure = (Server.Derive.procedure \"App.Shop.place\" Order.schema Schema.string (Server.Derive.with_error OutOfStock.schema Server.Derive.no_errors))
/// Up to three.
let place = (Server.Derive.call place$procedure)
let place$server = (Server.Derive.function place$procedure place$impl)")
    );
}

#[test]
fn several_parameters_travel_as_a_tuple_and_unauthorized_guards() {
    let code = one(
        "label : String -> Float ->{Server.Server; Server.Unauthorized} String\nserver fn label item count = item\n",
    );
    assert_eq!(
        code.as_deref(),
        Ok("label$impl : (String -> Float ->{Server.Server; Server.Unauthorized} String)
let label$impl item count = item
label$procedure : (Server.Derive.Procedure (String, Float) {Server.Unauthorized} String)
let label$procedure = (Server.Derive.procedure \"App.Shop.label\" (Schema.unsafe_tuple [(Schema.item Schema.string), (Schema.item Schema.number)]) Schema.string (Server.Derive.with_error Server.Unauthorized$schema Server.Derive.no_errors))
let label = (fun p0 p1 -> (Server.Derive.call label$procedure (p0, p1)))
let label$server = (Server.Derive.guarded label$procedure (fun (p0, p1) -> (label$impl p0 p1)))")
    );
}

#[test]
fn server_names_the_implementation() {
    assert_eq!(
        one("let served = server App.Api.place\n").as_deref(),
        Ok("App.Api.place$server")
    );
    assert_eq!(
        errors("let s = server (f x)\n"),
        "E010 on the block: `server` names a server fn: `server name` or `server M.name`"
    );
}

#[test]
fn a_signature_that_is_no_server_fns_is_refused() {
    for (body, why) in [
        (
            "server fn lone x = x\n",
            "server fn `lone` needs a signature right above it: what the server runs",
        ),
        (
            "pick : forall a. a ->{Server.Server;} a\nserver fn pick x = x\n",
            "server fn `pick`: a server fn is not generic: its types cross the network",
        ),
        (
            "pure : String -> String\nserver fn pure x = x\n",
            "server fn `pure`: a server fn runs on the server: its arrow is `->{r; e}`",
        ),
        (
            "open : String ->{Server.Server; | e} String\nserver fn open x = x\n",
            "server fn `open`: a server fn lists its errors: each crosses the network with its codec",
        ),
        (
            "two : String -> String ->{Server.Server;} String\nserver fn two x = x\n",
            "server fn `two` names each of its 2 parameters",
        ),
    ] {
        assert_eq!(errors(body), format!("E010 on the block: {why}"), "{body}");
    }
}
