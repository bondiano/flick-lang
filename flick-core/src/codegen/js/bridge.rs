//! the bridge to Effect in compiled code. A foreign binding the kernel
//! has no export for is Effect's, and what crosses it is converted by its
//! shape, from the signature. Emitted only for Flick's own runtime.

use flick_ir::Shape;

use super::{Gen, names};
use crate::ir::{Foreign, ForeignMode};

/// which way a value crosses the bridge to Effect.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Toward {
    /// From Flick to Effect's side.
    Host,
    /// From Effect's side to Flick.
    Flick,
}

impl Toward {
    fn back(self) -> Self {
        match self {
            Toward::Host => Toward::Flick,
            Toward::Flick => Toward::Host,
        }
    }
}

impl Gen<'_> {
    /// the import alias of a helper on the Effect adapter package.
    fn bridge(&mut self, name: &str) -> String {
        let alias = format!("$bridge_{name}");
        if !self
            .foreign
            .iter()
            .any(|(source, export, _)| source == "@flick-lang/effect-ts/bridge" && export == name)
        {
            self.foreign.push((
                "@flick-lang/effect-ts/bridge".to_owned(),
                name.to_owned(),
                alias.clone(),
            ));
        }
        alias
    }

    /// the body of a wrapper of `f` that reaches Effect over the
    /// bridge, or `None` where the bridge changes nothing.
    pub(super) fn bridged_body(
        &mut self,
        f: &Foreign,
        runtime: bool,
        call: &str,
        copy_result: bool,
        catch: Option<&str>,
    ) -> Option<String> {
        let shape = &f.adapter.result_shape;
        Some(match (f.mode, runtime) {
            (ForeignMode::Pure, _) | (_, false) => {
                let call = format!("{}(() => {call})", self.bridge("inSyncContext"));
                let value = self.foreign_result(f, copy_result, &call);
                self.crossed(Toward::Flick, shape, value)
            }
            (ForeignMode::Effect, true) => {
                let ran = match catch {
                    Some(c) => format!(
                        "{}(() => {}({call}), {c})",
                        self.rt("foreignEffect"),
                        self.bridge("fromEffect")
                    ),
                    None => format!("{}({call})", self.bridge("fromEffect")),
                };
                let value = self.foreign_result(f, copy_result, "$v");
                let value = self.crossed(Toward::Flick, shape, value);
                self.mapped(ran, "$v", &value)
            }
            (ForeignMode::Sync, true) => {
                let call = format!("{}(() => {call})", self.bridge("inSyncContext"));
                let value = self.foreign_result(f, copy_result, &call);
                let value = self.crossed(Toward::Flick, shape, value);
                match catch {
                    Some(c) => format!("{}(() => {value}, {c})", self.rt("foreignSync")),
                    None => value,
                }
            }
            (ForeignMode::Async, true) => return None,
        })
    }

    /// `value`, of the shape `shape`, as the side `toward` takes it.
    pub(super) fn crossed(&mut self, toward: Toward, shape: &Shape, value: String) -> String {
        match shape {
            Shape::Data => {
                let convert = self.bridge(match toward {
                    Toward::Host => "dataToEffect",
                    Toward::Flick => "dataFromEffect",
                });
                format!("{convert}({value})")
            }
            Shape::Fun {
                args,
                result,
                runtime,
                sync,
            } => self.crossed_fun(toward, args, result, (*runtime, *sync), value),
            Shape::Eff(inner) => self.crossed_eff(toward, inner, &value),
            Shape::List(inner) => {
                let depth = self.bridge_depth();
                let item = self.crossed(toward, inner, format!("$e{depth}"));
                format!("({value}).map(($e{depth}) => {item})")
            }
            Shape::Option(inner) => {
                let depth = self.bridge_depth();
                let item = self.crossed(toward, inner, format!("$e{depth}"));
                let convert = match toward {
                    Toward::Host => self.bridge("optionToEffect"),
                    Toward::Flick => self.bridge("optionFromEffect"),
                };
                format!("{convert}({value}, ($e{depth}) => {item})")
            }
            Shape::Result(error, success) => {
                let depth = self.bridge_depth();
                let error = self.crossed(toward, error, format!("$e{depth}"));
                let success = self.crossed(toward, success, format!("$a{depth}"));
                let convert = match toward {
                    Toward::Host => self.bridge("resultToEffect"),
                    Toward::Flick => self.bridge("resultFromEffect"),
                };
                format!("{convert}({value}, ($a{depth}) => {success}, ($e{depth}) => {error})")
            }
            Shape::Tuple(items) => {
                let depth = self.bridge_depth();
                let parts: Vec<String> = items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| self.crossed(toward, item, format!("$t{depth}[{i}]")))
                    .collect();
                format!("(($t{depth}) => [{}])({value})", parts.join(", "))
            }
            Shape::Record(fields) => {
                let depth = self.bridge_depth();
                let parts: Vec<String> = fields
                    .iter()
                    .filter(|(_, field)| !field.is_data())
                    .map(|(name, field)| {
                        let name = names::string(name);
                        let field = self.crossed(toward, field, format!("$t{depth}[{name}]"));
                        format!("[{name}]: {field}")
                    })
                    .collect();
                let data = self.crossed(toward, &Shape::Data, format!("$t{depth}"));
                format!(
                    "(($t{depth}) => ({{ ...{data}, {} }}))({value})",
                    parts.join(", ")
                )
            }
            Shape::Layer
            | Shape::Schema
            | Shape::Codec
            | Shape::Fiber
            | Shape::Duration
            | Shape::Schedule
            | Shape::Chunk
            | Shape::HashMap
            | Shape::HashSet
            | Shape::Handlers
            | Shape::Exit
            | Shape::Failure => self.crossed_opaque(toward, shape, value),
        }
    }

    fn crossed_opaque(&mut self, toward: Toward, shape: &Shape, value: String) -> String {
        let convert = match (toward, shape) {
            (Toward::Host, Shape::Layer) => self.bridge("layerToEffect"),
            (Toward::Host, Shape::Schema) => self.bridge("schemaToEffect"),
            (Toward::Host, Shape::Codec) => self.bridge("codecToEffect"),
            (Toward::Host, Shape::Fiber) => self.bridge("fiberToEffect"),
            (Toward::Host, Shape::Duration) => self.bridge("durationToEffect"),
            (Toward::Host, Shape::Schedule) => self.bridge("scheduleToEffect"),
            (Toward::Host, Shape::Chunk) => self.bridge("chunkToEffect"),
            (Toward::Host, Shape::HashMap) => self.bridge("hashMapToEffect"),
            (Toward::Host, Shape::HashSet) => self.bridge("hashSetToEffect"),
            (Toward::Host, Shape::Handlers) => self.bridge("handlersToEffect"),
            (Toward::Host, Shape::Exit) => self.bridge("exitToEffect"),
            (Toward::Host, _) => self.bridge("failureToEffect"),
            (Toward::Flick, Shape::Exit) => self.bridge("exitFromEffect"),
            (Toward::Flick, Shape::Failure) => self.bridge("failureFromEffect"),
            (Toward::Flick, Shape::Layer) => self.bridge("layerFromEffect"),
            (Toward::Flick, Shape::Schema) => self.bridge("schemaFromEffect"),
            (Toward::Flick, Shape::Codec) => self.bridge("codecFromEffect"),
            (Toward::Flick, Shape::Fiber) => self.bridge("fiberFromEffect"),
            (Toward::Flick, Shape::Duration) => self.bridge("durationFromEffect"),
            (Toward::Flick, Shape::Schedule) => self.bridge("scheduleFromEffect"),
            (Toward::Flick, Shape::Chunk) => self.bridge("chunkFromEffect"),
            (Toward::Flick, Shape::HashMap) => self.bridge("hashMapFromEffect"),
            (Toward::Flick, Shape::HashSet) => self.bridge("hashSetFromEffect"),
            // a record of handlers is Effect's as it is
            (Toward::Flick, _) => return value,
        };
        format!("{convert}({value})")
    }

    /// a function crossing: its arguments cross back, its result
    /// crosses with it, and the call is run by the side that made it.
    fn crossed_fun(
        &mut self,
        toward: Toward,
        args: &[Shape],
        result: &Shape,
        (runtime, sync): (bool, bool),
        function: String,
    ) -> String {
        let depth = self.bridge_depth();
        let prefix = match toward {
            Toward::Host => "h",
            Toward::Flick => "g",
        };
        let params: Vec<String> = (0..args.len())
            .map(|k| format!("${prefix}{depth}_{k}"))
            .collect();
        let mut call = function;
        for (param, arg) in params.iter().zip(args) {
            let arg = self.crossed(toward.back(), arg, param.clone());
            call = format!("{call}({arg})");
        }
        let at = format!("$r{depth}");
        let body = match (toward, sync, runtime) {
            (Toward::Host, true, _) => {
                let value = self.crossed(toward, result, at.clone());
                let ran = format!("{}(() => {call})", self.bridge("callSync"));
                if value == at {
                    ran
                } else {
                    format!("(({at}) => {value})({ran})")
                }
            }
            (Toward::Host, false, true) => {
                let value = self.crossed(toward, result, at.clone());
                let ran = self.mapped(call, &at, &value);
                format!("{}(() => {ran})", self.bridge("callEffect"))
            }
            (Toward::Flick, true, _) => {
                let ran = format!("{}(() => {call})", self.bridge("inSyncContext"));
                self.crossed(toward, result, ran)
            }
            (Toward::Flick, false, true) => {
                let value = self.crossed(toward, result, at.clone());
                let ran = format!("{}({call})", self.bridge("fromEffect"));
                self.mapped(ran, &at, &value)
            }
            (_, false, false) => self.crossed(toward, result, call),
        };
        // curried, as Flick calls it: a callback the binding's host calls with several
        // arguments at once is adapted by `callback`, as when nothing is bridged
        params
            .iter()
            .rev()
            .fold(body, |body, param| format!("({param}) => {body}"))
    }

    /// an `Eff`, a thunk on the kernel and an Effect on Effect's side.
    fn crossed_eff(&mut self, toward: Toward, inner: &Shape, eff: &str) -> String {
        let depth = self.bridge_depth();
        let at = format!("$r{depth}");
        let value = self.crossed(toward, inner, at.clone());
        match toward {
            Toward::Host => {
                let ran = self.mapped(format!("{eff}()"), &at, &value);
                format!("{}(() => {ran})", self.bridge("callEffect"))
            }
            Toward::Flick => {
                let ran = format!("{}({eff})", self.bridge("fromEffect"));
                format!("() => {}", self.mapped(ran, &at, &value))
            }
        }
    }

    /// `ran`, its value `at` made `value` when they differ.
    fn mapped(&mut self, ran: String, at: &str, value: &str) -> String {
        if value == at {
            ran
        } else {
            format!("{}({ran}, ({at}) => {value})", self.rt("mapped"))
        }
    }

    fn bridge_depth(&mut self) -> String {
        let temp = self.temp();
        temp.trim_start_matches('$').to_owned()
    }
}
