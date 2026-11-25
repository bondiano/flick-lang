//! One owner for opaque UI values throughout a build. Native UI stays free
//! of Effect until a reachable binding needs the adapter's UI implementation.

use super::builtin_modules_of;
use crate::codegen::EffectBridge;
use crate::ir::Module;
use crate::project::Compilation;
use flick_backend_support::bridge_select;

pub fn select(
    compilation: &Compilation,
    available: EffectBridge,
    runners: &[(String, String)],
) -> EffectBridge {
    select_modules_with_roots(
        compilation.modules.iter().map(|module| &*module.ir),
        available,
        runners
            .iter()
            .map(|(_, module)| (module.clone(), "run".to_owned()))
            .collect(),
    )
}

#[cfg(test)]
fn select_modules<'a>(
    modules: impl Iterator<Item = &'a Module>,
    available: EffectBridge,
) -> EffectBridge {
    select_modules_with_roots(modules, available, Vec::new())
}

fn select_modules_with_roots<'a>(
    modules: impl Iterator<Item = &'a Module>,
    available: EffectBridge,
    pending: Vec<(String, String)>,
) -> EffectBridge {
    bridge_select::select(modules, builtin_modules_of(true).iter(), available, pending)
}

#[cfg(test)]
mod tests {
    use super::{select_modules, select_modules_with_roots};
    use crate::codegen::EffectBridge;
    use crate::ir::tests::lower_text;
    use std::iter::once;

    #[test]
    fn explicit_action_spans_select_the_adapter_ui_context() {
        let module = lower_text("App", "module App exposing (run)\nrun : Unit ->{} Unit\nlet run () = Ui.Tracing.with_action_spans true (task { () })\n").expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::AvailableWithUi
        );
    }

    #[test]
    fn a_native_atom_keeps_the_ui_free_of_effect() {
        let module = lower_text(
            "App",
            "module App exposing (state)\nlet state = Atom.make 0\n",
        )
        .expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::Available
        );
    }

    #[test]
    fn a_serializable_atom_keeps_the_ui_free_of_effect() {
        let module = lower_text(
            "App",
            "module App exposing (count, draft, item)\n\
             let count = Atom.serializable (Codec.json Schema.number) (Atom.make 0)\n\
             draft : String -> Atom.Writable String\n\
             let draft = Atom.serializable_family (Codec.json Schema.string) (fun _ -> Atom.make \"\")\n\
             item : Atom.Atom (Atom.Async String none)\n\
             let item = Atom.serializable_task (Codec.json Schema.string) (Atom.task (task { \"x\" }))\n",
        )
        .expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::Available
        );
    }

    #[test]
    fn a_route_and_its_links_keep_the_ui_free_of_effect() {
        let module = lower_text(
            "App",
            r"module App exposing (page, to)
type Page = Home | User Float
  deriving Eq, Show, Schema, Url
page : Atom.Writable Page
let page = Atom.route Page.url Home
to : Page -> List Ui -> Ui
let to target children = Ui.route_link { route = page; url = Page.url; to = target } children
",
        )
        .expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::Available
        );
    }

    #[test]
    fn a_stream_query_keeps_the_ui_free_of_effect() {
        let module = lower_text(
            "App",
            "module App exposing (latest)\n\
             latest : Atom.Atom (Atom.Async Float none)\n\
             let latest = Atom.query_stream [\"stock\"] (Stream.from_list [7, 8])\n",
        )
        .expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::Available
        );
    }

    const PROCEDURE: &str = "greet : Rpc.Rpc \"greet\" (String ->{r} String)\n\
         let greet = Rpc.make (Record.label : Record.Label \"greet\") |> Rpc.with_payload (Codec.json Schema.string) |> Rpc.with_success (Codec.json Schema.string)\n";

    #[test]
    fn a_client_of_a_procedure_keeps_rpc_on_flicks_runtime() {
        let text = format!(
            "module App exposing (run)\n{PROCEDURE}run : Unit ->{{Async; Rpc.ClientError}} String\nlet run () = Rpc.call \"/rpc\" greet \"Ada\"\n"
        );
        let module = lower_text("App", &text).expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::Available
        );
    }

    #[test]
    fn a_server_takes_every_web_binding_from_the_adapter() {
        let text = format!(
            "module App exposing (port, run)\n{PROCEDURE}run : Unit ->{{Async; Rpc.ClientError}} String\nlet run () = Rpc.call \"/rpc\" greet \"Ada\"\nport : Rpc.Server s -> Float\nlet port = Rpc.port\n"
        );
        let module = lower_text("App", &text).expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::AvailableWithWeb
        );
    }

    #[test]
    fn an_http_binding_of_the_adapter_takes_the_rpc_client_with_it() {
        let text = format!(
            "module App exposing (page, run)\n{PROCEDURE}run : Unit ->{{Async; Rpc.ClientError}} String\nlet run () = Rpc.call \"/rpc\" greet \"Ada\"\npage : Http.ClientResponse.ClientResponse -> Http.ServerResponse.ServerResponse\nlet page = Http.ServerResponse.from_client_response\n"
        );
        let module = lower_text("App", &text).expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::AvailableWithWeb
        );
    }

    #[test]
    fn a_reexported_stream_renderer_keeps_the_ui_free_of_effect() {
        let module = lower_text(
            "App",
            "module App exposing (stream)\nstream : String -> Ui -> Stream.Core.Stream {Async, Reactivity.Atom.Registry} none String\nlet stream = Ui.Server.stream\n",
        )
        .expect("lowers");
        assert_eq!(
            select_modules(once(&module), EffectBridge::Available),
            EffectBridge::Available
        );
        assert_eq!(
            select_modules(once(&module), EffectBridge::Absent),
            EffectBridge::Absent
        );
    }
    #[test]
    fn a_cloudflare_runner_owns_web_values_without_a_source_reference_to_its_run() {
        let module = lower_text("App", "module App exposing (main)\nmain : Unit -> Http.ServerResponse.ServerResponse\nlet main () = Http.ServerResponse.empty Http.ServerResponse.defaults\n").expect("lowers");
        assert_eq!(
            select_modules_with_roots(
                once(&module),
                EffectBridge::Available,
                vec![("Runner.Cloudflare".to_owned(), "run".to_owned())]
            ),
            EffectBridge::AvailableWithWeb
        );
    }
}
