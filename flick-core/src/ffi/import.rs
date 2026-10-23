//! From a [`Surface`] to `Generated.fl` and the coverage report. Each
//! selected export gets one of three statuses: supported (a `foreign`
//! binding), needs policy (a contract the `.d.ts` does not state) or
//! unsupported (outside the subset); the last two become `foreign blocked`
//! with the reason, so using them is an error while the rest stay usable.
//! Nothing is inferred that a declaration cannot show: a call is never pure,
//! a promise's rejection, a nullable result and a callback's retention all
//! need a policy, and `any` is `JsValue`, or refused in strict mode.

use std::collections::BTreeSet;
use std::fmt::Write;

use super::convert::{Blocked, Convert, arrow_operand, mentions, var_name};
use super::{ExportKind, ExportPolicy, ExportShape, Policy, Signature, Surface};
use crate::syntax::kind::keyword;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Supported,
    NeedsPolicy,
    Unsupported,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Supported => "supported",
            Status::NeedsPolicy => "needs policy",
            Status::Unsupported => "unsupported",
        }
    }
}

/// One line of the coverage report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The JS export.
    pub export: String,
    /// The Flick name of its binding.
    pub name: String,
    pub status: Status,
    /// The Flick signature of a supported binding; the reason otherwise.
    pub detail: String,
    /// What a supported binding leaves out.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    /// `Generated.fl`.
    pub text: String,
    pub entries: Vec<Entry>,
    /// Selected names the package does not export.
    pub missing: Vec<String>,
}

impl Generated {
    /// The report `flick ffi import` prints.
    pub fn report(&self, surface: &Surface) -> String {
        let mut out = format!("{} ({})\n", surface.provenance(), surface.declarations);
        for e in &self.entries {
            let _ = writeln!(out, "  {:<13}{} — {}", e.status.label(), e.name, e.detail);
            for note in &e.notes {
                let _ = writeln!(out, "  {:<13}  note: {note}", "");
            }
        }
        for name in &self.missing {
            let _ = writeln!(
                out,
                "  {:<13}{name} — not exported by the package",
                "missing"
            );
        }
        let count = |s: Status| self.entries.iter().filter(|e| e.status == s).count();
        let _ = writeln!(
            out,
            "{} supported, {} need a policy, {} unsupported",
            count(Status::Supported),
            count(Status::NeedsPolicy),
            count(Status::Unsupported)
        );
        out
    }
}

/// The first line of every generated module; `ffi check` reads the import
/// back from it.
pub const HEADER: &str = "// flick ffi import ";

/// Generates module `module` for the exports `select` picks (all without it).
pub fn generate(
    surface: &Surface,
    policy: &Policy,
    select: Option<&[String]>,
    module: &str,
) -> Generated {
    let chosen: Vec<&ExportShape> = surface
        .exports
        .iter()
        .filter(|e| select.is_none_or(|s| s.contains(&e.name) || s.contains(&e.local)))
        .collect();
    let missing = select
        .unwrap_or_default()
        .iter()
        .filter(|s| {
            !surface
                .exports
                .iter()
                .any(|e| &&e.name == s || &&e.local == s)
        })
        .cloned()
        .collect();
    let mut taken = BTreeSet::new();
    let mut entries = Vec::new();
    let mut items = Vec::new();
    let mut handles = BTreeSet::new();
    for export in chosen {
        for (name, outcome) in bindings(export, &policy.of(&export.name), policy.strict) {
            let outcome = if taken.insert(name.clone()) {
                outcome
            } else {
                Err(Blocked::unsupported(format!(
                    "its Flick name `{name}` is taken by another export"
                )))
            };
            let provenance = format!("{} {}", surface.provenance(), export.fingerprint);
            match outcome {
                Ok(b) => {
                    handles.extend(b.handles.iter().cloned());
                    items.push(b.text(&name, &surface.package, &export.name, &provenance));
                    entries.push(Entry {
                        export: export.name.clone(),
                        name,
                        status: Status::Supported,
                        detail: b.signature,
                        notes: b.notes,
                    });
                }
                Err(blocked) => {
                    let reason = format!("{}: {}", blocked.status.label(), blocked.reason);
                    items.push(format!(
                        "/// {provenance}\nforeign blocked {name} {}\n",
                        quote(&reason)
                    ));
                    entries.push(Entry {
                        export: export.name.clone(),
                        name,
                        status: blocked.status,
                        detail: blocked.reason,
                        notes: Vec::new(),
                    });
                }
            }
        }
    }
    let mut text = format!("{HEADER}{}", surface.package);
    if let Some(select) = select {
        let _ = write!(text, " --select {}", select.join(","));
    }
    let _ = write!(
        text,
        "\n// Generated from {} of {}: do not edit.\n\
         // Reviewed bindings go in a module of your own, which a new import\n\
         // never touches.\n\
         module {module} exposing (..)\n",
        surface.declarations,
        surface.provenance()
    );
    if items.iter().any(|i| i.contains("JsError.from_js")) {
        text.push_str("\nimport JsError exposing (JsError)\n");
    }
    for handle in &handles {
        let _ = write!(
            text,
            "\n/// A JS `{handle}`, opaque to Flick.\ntype {handle}\n"
        );
    }
    for item in items {
        text.push('\n');
        text.push_str(&item);
    }
    Generated {
        text,
        entries,
        missing,
    }
}

/// A supported binding.
struct Binding {
    signature: String,
    mode: &'static str,
    metadata: Vec<String>,
    handles: BTreeSet<String>,
    notes: Vec<String>,
}

impl Binding {
    fn text(&self, name: &str, package: &str, export: &str, provenance: &str) -> String {
        let mut out = format!(
            "/// {provenance}\n{name} : {}\nforeign {} {name} from {} {}\n",
            self.signature,
            self.mode,
            quote(package),
            quote(export)
        );
        for line in &self.metadata {
            let _ = writeln!(out, "  {line}");
        }
        out
    }
}

/// The bindings of one export: one per signature, an overload's name ending
/// in its number of parameters.
fn bindings(
    export: &ExportShape,
    policy: &ExportPolicy,
    strict: bool,
) -> Vec<(String, Result<Binding, Blocked>)> {
    let name = flick_name(&export.local);
    let refuse = |reason: &str| vec![(name.clone(), Err(Blocked::unsupported(reason.to_owned())))];
    match export.kind {
        ExportKind::Class => {
            return refuse(
                "a class: its constructor and methods are bound by hand; functions see its instances as an opaque type",
            );
        }
        ExportKind::Value => return refuse("a value, not a function: bind it by hand"),
        ExportKind::Function => {}
    }
    match &export.signatures[..] {
        [one] => vec![(name, binding(one, policy, strict))],
        many => {
            let arities: BTreeSet<usize> = many.iter().map(|s| s.params.len()).collect();
            if arities.len() < many.len() {
                return vec![(
                    name,
                    Err(Blocked::unsupported(format!(
                        "{} overloads, some with the same number of parameters: no safe way to tell them apart",
                        many.len()
                    ))),
                )];
            }
            many.iter()
                .map(|s| {
                    let name = format!("{name}_{}", s.params.len());
                    (name, binding(s, policy, strict))
                })
                .collect()
        }
    }
}

fn binding(sig: &Signature, policy: &ExportPolicy, strict: bool) -> Result<Binding, Blocked> {
    if sig.this {
        return Err(Blocked::unsupported(
            "it declares a `this` parameter".into(),
        ));
    }
    let mut c = Convert {
        strict,
        handles: BTreeSet::new(),
        vars: BTreeSet::new(),
        callback: false,
    };
    let (types, mut metadata, notes) = c.params(&sig.params)?;
    let (result, promise, empty) = c.result(&sig.result, policy)?;
    if let Some(var) = sig
        .type_params
        .iter()
        .find(|v| !types.iter().any(|t| mentions(t, &var_name(v))))
    {
        return Err(Blocked::unsupported(format!(
            "type parameter `{var}` appears in no parameter, so nothing ties it to a value"
        )));
    }
    if c.callback && policy.retention.is_none() {
        return Err(Blocked::policy(
            "it takes a callback: say how long JS keeps it (`retention = \"call\"` or `\"owner\"`)"
                .into(),
        ));
    }
    metadata.insert(0, format!("arity {}", types.len()));
    let (mode, errors) = call_mode(promise, policy, &mut metadata)?;
    if empty {
        metadata.push("nullable".to_owned());
    }
    if let Some(r) = &policy.retention {
        metadata.push(format!("retention {r}"));
    }
    Ok(Binding {
        signature: signature(types, mode, errors, &result, &c.vars),
        mode,
        metadata,
        handles: c.handles,
        notes,
    })
}

/// The mode of the call and the error its arrow carries, with `catch` and
/// `cancel signal` added to `metadata`.
fn call_mode(
    promise: bool,
    policy: &ExportPolicy,
    metadata: &mut Vec<String>,
) -> Result<(&'static str, &'static str), Blocked> {
    let pure = policy.mode.as_deref() == Some("pure");
    let catch = policy.catch.is_some();
    let refuse = |what: &str| Err(Blocked::unsupported(what.to_owned()));
    if promise && !catch {
        return Err(Blocked::policy(
            "it returns a Promise: say what a rejection becomes (`catch = \"JsError\"`)".into(),
        ));
    }
    if pure && (promise || catch) {
        return refuse("`mode = \"pure\"` for a call that may throw or returns a Promise");
    }
    if !promise && policy.cancel.is_some() {
        return refuse("`cancel = \"signal\"` for a function that returns no Promise");
    }
    if catch {
        metadata.push("catch JsError.from_js".to_owned());
    }
    if policy.cancel.is_some() {
        metadata.push("cancel signal".to_owned());
    }
    let errors = if catch { "JsError" } else { "" };
    Ok(match (promise, pure) {
        (true, _) => ("async", errors),
        (false, true) => ("pure", errors),
        (false, false) => ("sync", errors),
    })
}

/// `A -> B ->{; E} R`: the last arrow runs the call, unless it is pure.
fn signature(
    mut types: Vec<String>,
    mode: &str,
    errors: &str,
    result: &str,
    vars: &BTreeSet<String>,
) -> String {
    if types.is_empty() {
        types.push("Unit".to_owned());
    }
    let last = types.pop().unwrap_or_default();
    let mut out = String::new();
    for t in &types {
        let _ = write!(out, "{} -> ", arrow_operand(t));
    }
    let arrow = match (mode, errors) {
        ("pure", _) => " -> ".to_owned(),
        (_, "") => " ->{} ".to_owned(),
        (_, e) => format!(" ->{{; {e}}} "),
    };
    let _ = write!(out, "{}{arrow}{result}", arrow_operand(&last));
    if vars.is_empty() {
        out
    } else {
        let vars: Vec<&str> = vars.iter().map(String::as_str).collect();
        format!("forall {}. {out}", vars.join(" "))
    }
}

/// `nextId` → `next_id`, `parseISO` → `parse_iso`; a keyword gets a `_`.
pub fn flick_name(js: &str) -> String {
    let chars: Vec<char> = js
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).map(|j| chars[j]);
        let next = chars.get(i + 1);
        let boundary = c.is_ascii_uppercase()
            && prev.is_some_and(|p| {
                p.is_ascii_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_ascii_uppercase() && next.is_some_and(char::is_ascii_lowercase))
            });
        if boundary && !out.ends_with('_') {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    if !out.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') {
        out = format!("js_{out}");
    }
    if keyword(&out).is_some() {
        out.push('_');
    }
    out
}

/// A Flick string literal.
fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
