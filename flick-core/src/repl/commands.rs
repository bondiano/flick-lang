//! Session command checking (`:handle`, `:provide`, `:bg`) for [`Session`].

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::analysis::{HandlerStyle, format_type, inspect_handler, inspect_layer, inspect_task};
use super::render::{Layout, Name, Space};
use super::{Phrase, ROOT, Session};

use crate::diagnostics::{Diagnostic, Severity};
use crate::project::Compilation;
use crate::types::Type;

const HANDLER_BINDING: &str = "__repl_handler";
const LAYER_BINDING: &str = "__repl_layer";
const BG_BINDING: &str = "__repl_bg";

/// What entering a `:handle <expr>` command gave.
pub struct AcceptedHandle<'s> {
    pub phrase: &'s Phrase,
    pub compilation: Compilation,
    pub diagnostics: Vec<Diagnostic>,
    /// Local binding name inside `phrase.module` (`__repl_handler`).
    pub global_name: String,
    /// Effect names handled by this handler.
    pub handled_effects: Vec<String>,
    pub style: HandlerStyle,
}

/// What entering a `:provide <expr>` command gave.
pub struct AcceptedProvide<'s> {
    pub phrase: &'s Phrase,
    pub compilation: Compilation,
    pub diagnostics: Vec<Diagnostic>,
    /// Local binding name inside `phrase.module` (`__repl_layer`).
    pub global_name: String,
    /// Service names provided by this layer.
    pub provided_services: Vec<String>,
}

/// What entering a `:bg <expr>` command gave.
pub struct AcceptedBg<'s> {
    pub phrase: &'s Phrase,
    pub compilation: Compilation,
    pub diagnostics: Vec<Diagnostic>,
    /// Local binding name inside `phrase.module` (`__repl_bg`).
    pub global_name: String,
    /// Unresolved user effects on the task, if any.
    pub effects: Vec<String>,
    pub ok_type: Option<Type>,
    pub err_type: Option<Type>,
}

impl Session {
    /// Checks `:handle <expr>` and records its handled effects for subsequent phrases.
    pub fn enter_handle(&mut self, expr: &str) -> Result<AcceptedHandle<'_>, Vec<Diagnostic>> {
        let trimmed = expr.trim();
        let n = self.phrases.len() + 1;
        let module = format!("Repl.P{n}");
        let path = PathBuf::from(format!("{ROOT}/Repl/P{n}.fl"));
        let file = self.sources.add(format!("repl:{n}"), trimmed);
        let mut own = BTreeSet::new();
        own.insert((Space::Value, HANDLER_BINDING.to_owned()));
        let head = self.command_head(&module, &own);
        let layout = if trimmed.starts_with('|') {
            Layout::handle_arms(&head, HANDLER_BINDING, trimmed)
        } else {
            Layout::named_expression(&head, HANDLER_BINDING, trimmed, None)
        };
        let (layout, diagnostics, compilation) = self.check_and_sign(&path, layout);
        let placed = self.place(diagnostics, &path, &layout, file);
        if placed.iter().any(|d| d.severity == Severity::Error) {
            self.files.remove(&path);
            return Err(placed);
        }
        let scheme = Self::binding_scheme(&compilation, &module, HANDLER_BINDING);
        let module_ir = compilation
            .modules
            .iter()
            .find(|m| m.name == module)
            .map(|m| m.ir.as_ref());
        let Some((style, handled_effects)) = scheme
            .as_ref()
            .and_then(|ty| inspect_handler(ty, module_ir, HANDLER_BINDING))
        else {
            self.files.remove(&path);
            let actual = scheme
                .as_ref()
                .map_or_else(|| "unknown".to_owned(), format_type);
            return Err(vec![self.input_error(
                file,
                format!(
                    "`:handle` expects a handler function `(Unit ->{{E | r; e}} a) ->{{r; e}} a` \
                     or `Eff {{E | r}} e a -> ...`, got `{actual}`"
                ),
            )]);
        };
        self.save_undo(&path);
        for effect in &handled_effects {
            if !self.handled_effects.contains(effect) {
                self.handled_effects.push(effect.clone());
            }
        }
        self.phrases.push(Phrase {
            module,
            input: file,
            names: Vec::new(),
        });
        Ok(AcceptedHandle {
            phrase: self.phrases.last().expect("the handle phrase just entered"),
            compilation,
            diagnostics: placed,
            global_name: HANDLER_BINDING.to_owned(),
            handled_effects,
            style,
        })
    }

    /// Checks `:provide <expr>` as a `Layer` and records its provided services.
    pub fn enter_provide(&mut self, expr: &str) -> Result<AcceptedProvide<'_>, Vec<Diagnostic>> {
        let trimmed = expr.trim();
        let n = self.phrases.len() + 1;
        let module = format!("Repl.P{n}");
        let path = PathBuf::from(format!("{ROOT}/Repl/P{n}.fl"));
        let file = self.sources.add(format!("repl:{n}"), trimmed);
        let mut own = BTreeSet::new();
        own.insert((Space::Value, LAYER_BINDING.to_owned()));
        let head = self.command_head(&module, &own);
        let layout = Layout::named_expression(&head, LAYER_BINDING, trimmed, None);
        let (layout, diagnostics, compilation) = self.check_and_sign(&path, layout);
        let placed = self.place(diagnostics, &path, &layout, file);
        if placed.iter().any(|d| d.severity == Severity::Error) {
            self.files.remove(&path);
            return Err(placed);
        }
        let scheme = Self::binding_scheme(&compilation, &module, LAYER_BINDING);
        let Some(provided_services) = scheme.as_ref().and_then(inspect_layer) else {
            self.files.remove(&path);
            let actual = scheme
                .as_ref()
                .map_or_else(|| "unknown".to_owned(), format_type);
            return Err(vec![self.input_error(
                file,
                format!("`:provide` expects a `Layer`, got `{actual}`"),
            )]);
        };
        self.save_undo(&path);
        for service in &provided_services {
            if !self.provided_services.contains(service) {
                self.provided_services.push(service.clone());
            }
        }
        self.phrases.push(Phrase {
            module,
            input: file,
            names: Vec::new(),
        });
        Ok(AcceptedProvide {
            phrase: self
                .phrases
                .last()
                .expect("the provide phrase just entered"),
            compilation,
            diagnostics: placed,
            global_name: LAYER_BINDING.to_owned(),
            provided_services,
        })
    }

    /// Checks `:bg <expr>` as a task (`Eff r e a`) without binding `it`.
    pub fn enter_bg(&mut self, expr: &str) -> Result<AcceptedBg<'_>, Vec<Diagnostic>> {
        let trimmed = expr.trim();
        let n = self.phrases.len() + 1;
        let module = format!("Repl.P{n}");
        let path = PathBuf::from(format!("{ROOT}/Repl/P{n}.fl"));
        let file = self.sources.add(format!("repl:{n}"), trimmed);
        let mut own = BTreeSet::new();
        own.insert((Space::Value, BG_BINDING.to_owned()));
        let head = self.command_head(&module, &own);
        let previous_it = self
            .scope
            .names
            .get(&(Space::Value, "it".to_owned()))
            .map(|(_, module)| module.clone());
        let layout = Layout::named_expression(&head, BG_BINDING, trimmed, previous_it.as_deref());
        let (layout, diagnostics, compilation) = self.check_and_sign(&path, layout);
        let placed = self.place(diagnostics, &path, &layout, file);
        if placed.iter().any(|d| d.severity == Severity::Error) {
            self.files.remove(&path);
            return Err(placed);
        }
        let scheme = Self::binding_scheme(&compilation, &module, BG_BINDING);
        let Some(task_info) = scheme
            .as_ref()
            .and_then(|ty| inspect_task(ty, &self.handled_effects, &self.provided_services))
        else {
            self.files.remove(&path);
            let actual = scheme
                .as_ref()
                .map_or_else(|| "unknown".to_owned(), format_type);
            return Err(vec![self.input_error(
                file,
                format!("`:bg` expects a task (`Eff`), got `{actual}`"),
            )]);
        };
        self.save_undo(&path);
        self.phrases.push(Phrase {
            module,
            input: file,
            names: Vec::new(),
        });
        Ok(AcceptedBg {
            phrase: self.phrases.last().expect("the bg phrase just entered"),
            compilation,
            diagnostics: placed,
            global_name: BG_BINDING.to_owned(),
            effects: task_info.effects,
            ok_type: Some(task_info.ok_type),
            err_type: task_info.err_type,
        })
    }

    fn command_head(&self, module: &str, own: &BTreeSet<Name>) -> String {
        let earlier: Vec<String> = self.phrases.iter().map(|p| p.module.clone()).collect();
        format!(
            "module {module} exposing (..)\n{}",
            self.scope.imports(&earlier, own, &[])
        )
    }
}
