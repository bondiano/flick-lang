//! Generates `Generated.fl` and the coverage report from a [`GoSurface`] and
//! [`GoPolicy`] following the `flick.go-ffi@0.1.0` value ABI.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use flick_ffi::ffi::import::{Entry, Generated, HEADER, Status, flick_name};

use super::convert::convert_export;
use super::surface::{GoExport, GoPolicy, GoSurface, GoTypeShape, go_module_part};

/// A Go named type opaque handle key: `(go_pkg, go_name, is_ptr)`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct HandleKey {
    pub(super) pkg: String,
    pub(super) name: String,
    pub(super) ptr: bool,
}

impl HandleKey {
    pub(super) fn go_form(&self) -> String {
        if self.ptr {
            format!("*{}", self.name)
        } else {
            self.name.clone()
        }
    }
}

#[derive(Debug, Clone)]
struct HandleDecl {
    flick_name: String,
    pkg: String,
    go_form: String,
}

#[derive(Debug, Clone)]
pub(super) struct Blocked {
    status: Status,
    reason: String,
}

impl Blocked {
    pub(super) fn unsupported(reason: impl Into<String>) -> Self {
        Self {
            status: Status::Unsupported,
            reason: reason.into(),
        }
    }

    pub(super) fn policy(reason: impl Into<String>) -> Self {
        Self {
            status: Status::NeedsPolicy,
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct Binding {
    pub(super) signature: String,
    pub(super) mode: &'static str,
    pub(super) metadata: Vec<String>,
    pub(super) handles: BTreeSet<HandleKey>,
    pub(super) uses_default_go_error: bool,
}

impl Binding {
    fn text(&self, name: &str, package: &str, export: &str, provenance: &str) -> String {
        let mode = if self.mode == "pure" { "pure " } else { "" };
        let mut out = format!(
            "/// {provenance}\n{name} : {}\nforeign {mode}{name} from {} {}\n",
            self.signature,
            quote(package),
            quote(export)
        );
        for line in &self.metadata {
            let _ = writeln!(out, "  {line}");
        }
        out
    }
}

/// Converts a Go export name (`ToUpper`, `Replacer.Replace`) into its Flick
/// binding identifier (`to_upper`, `replacer_replace`).
pub(super) fn go_flick_name(export: &str) -> String {
    let raw = export.replace('.', "_");
    let mut name = flick_name(&raw);
    if let Some(rest) = name.strip_prefix("js_") {
        name = format!("go_{rest}");
    }
    name
}

/// Formats the coverage report printed by `flick go:ffi import`.
pub(super) fn report(generated: &Generated, surface: &GoSurface) -> String {
    let mut out = format!("{} ({})\n", surface.provenance(), surface.declarations);
    for e in &generated.entries {
        let _ = writeln!(out, "  {:<13}{} — {}", e.status.label(), e.name, e.detail);
        for note in &e.notes {
            let _ = writeln!(out, "  {:<13}  note: {note}", "");
        }
    }
    for name in &generated.missing {
        let _ = writeln!(
            out,
            "  {:<13}{name} — not exported by the package",
            "missing"
        );
    }
    let count = |s: Status| generated.entries.iter().filter(|e| e.status == s).count();
    let _ = writeln!(
        out,
        "{} supported, {} need a policy, {} unsupported",
        count(Status::Supported),
        count(Status::NeedsPolicy),
        count(Status::Unsupported)
    );
    out
}

/// Generates `Generated.fl` for `surface`, restricted to `select` when present.
pub(super) fn generate(
    surface: &GoSurface,
    policy: &GoPolicy,
    select: Option<&[String]>,
    module: &str,
) -> Generated {
    let chosen: Vec<&GoExport> = surface
        .exports
        .iter()
        .filter(|e| select.is_none_or(|s| s.iter().any(|wanted| wanted == &e.name)))
        .collect();
    let missing: Vec<String> = select
        .unwrap_or_default()
        .iter()
        .filter(|wanted| !surface.exports.iter().any(|e| &e.name == *wanted))
        .cloned()
        .collect();

    let canonical_ptrs = collect_pointer_types(&chosen);
    let mut pending: Vec<(&GoExport, String, Result<Binding, Blocked>)> =
        Vec::with_capacity(chosen.len());
    let mut all_handles = BTreeSet::new();

    for export in &chosen {
        let name = go_flick_name(&export.name);
        let export_policy = policy.of_export(&export.name);
        let outcome = convert_export(export, &export_policy, policy, &canonical_ptrs);
        if let Ok(b) = &outcome {
            all_handles.extend(b.handles.iter().cloned());
        }
        pending.push((export, name, outcome));
    }

    let handle_decls = assign_handle_names(&surface.package, &all_handles, policy);
    let handle_names: BTreeMap<HandleKey, String> = handle_decls
        .iter()
        .map(|(k, d)| (k.clone(), d.flick_name.clone()))
        .collect();

    let mut taken = BTreeSet::new();
    let mut entries = Vec::new();
    let mut items = Vec::new();
    let mut used_handles = BTreeSet::new();
    let mut needs_go_error = false;

    for (export, name, outcome) in pending {
        let outcome = if taken.insert(name.clone()) {
            outcome
        } else {
            Err(Blocked::unsupported(format!(
                "its Flick name `{name}` is taken by another export"
            )))
        };
        let provenance = format!("{} {}", surface.provenance(), export.fingerprint);
        match outcome {
            Ok(mut binding) => {
                for key in &binding.handles {
                    let placeholder = handle_placeholder(key);
                    if let Some(resolved) = handle_names.get(key) {
                        binding.signature = binding.signature.replace(&placeholder, resolved);
                    }
                    used_handles.insert(key.clone());
                }
                if binding.uses_default_go_error {
                    needs_go_error = true;
                }
                items.push(binding.text(&name, &surface.package, &export.name, &provenance));
                entries.push(Entry {
                    export: export.name.clone(),
                    name,
                    status: Status::Supported,
                    detail: binding.signature,
                    notes: Vec::new(),
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

    let text = render_module(
        surface,
        select,
        module,
        needs_go_error,
        &used_handles,
        &handle_decls,
        &items,
    );
    Generated {
        text,
        entries,
        missing,
    }
}

fn render_module(
    surface: &GoSurface,
    select: Option<&[String]>,
    module: &str,
    needs_go_error: bool,
    used_handles: &BTreeSet<HandleKey>,
    handle_decls: &BTreeMap<HandleKey, HandleDecl>,
    items: &[String],
) -> String {
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

    if items.iter().any(|item| mentions_bytes(item)) {
        text.push_str("\nimport Bytes exposing (Bytes)\n");
    }

    if needs_go_error {
        text.push_str(
            "\n/// A Go `error` message caught across a foreign boundary.\n\
             type error GoError = GoError { message : String }\n  \
             deriving Eq, Show\n\n\
             from_go_error : String -> GoError\n\
             let from_go_error message = GoError { message = message }\n",
        );
    }

    let mut emitted_handles: Vec<&HandleDecl> = used_handles
        .iter()
        .filter_map(|k| handle_decls.get(k))
        .collect();
    emitted_handles.sort_by(|a, b| a.flick_name.cmp(&b.flick_name));
    for decl in emitted_handles {
        let _ = write!(
            text,
            "\n/// A Go `{}` from `{}`, opaque to Flick.\n@go from {} {}\ntype {}\n",
            decl.go_form,
            decl.pkg,
            quote(&decl.pkg),
            quote(&decl.go_form),
            decl.flick_name
        );
    }

    for item in items {
        text.push('\n');
        text.push_str(item);
    }
    text
}

fn mentions_bytes(item: &str) -> bool {
    item.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(|tok| tok == "Bytes")
}

/// Collects `(pkg, name)` pairs that appear as pointer types `*T` anywhere in
/// `exports` (params, results, or pointer-receiver methods) so value-receiver
/// methods on a pointer-handled struct can share the same `*T` handle cleanly.
fn collect_pointer_types(exports: &[&GoExport]) -> BTreeSet<(String, String)> {
    fn walk(ty: &GoTypeShape, out: &mut BTreeSet<(String, String)>) {
        match ty.kind.as_str() {
            "pointer" => {
                if let Some(elem) = ty.elem.as_deref()
                    && elem.kind == "named"
                    && elem.exported
                    && !elem.is_interface()
                    && let (Some(pkg), Some(name)) = (&elem.package, &elem.name)
                {
                    out.insert((pkg.clone(), name.clone()));
                }
            }
            "slice" => {
                if let Some(elem) = ty.elem.as_deref() {
                    walk(elem, out);
                }
            }
            _ => {}
        }
    }
    let mut ptrs = BTreeSet::new();
    for export in exports {
        if let Some(recv) = &export.receiver
            && recv.ptr
            && !recv.is_interface
        {
            ptrs.insert((recv.package.clone(), recv.name.clone()));
        }
        if let Some(sig) = &export.signature {
            for p in &sig.params {
                walk(&p.ty, &mut ptrs);
            }
            for r in &sig.results {
                walk(r, &mut ptrs);
            }
        }
    }
    ptrs
}

pub(super) fn handle_placeholder(key: &HandleKey) -> String {
    let ptr = if key.ptr { "ptr" } else { "val" };
    format!("__GO_HANDLE__{}__{}__{}__", key.pkg, key.name, ptr)
}

/// Assigns deterministic, collision-free Flick type names for every opaque Go
/// handle referenced by the generated module.
fn assign_handle_names(
    surface_pkg: &str,
    keys: &BTreeSet<HandleKey>,
    policy: &GoPolicy,
) -> BTreeMap<HandleKey, HandleDecl> {
    // Count how many distinct packages use each simple Go type name.
    let mut pkgs_by_name: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut ptr_and_val: BTreeMap<(&str, &str), (bool, bool)> = BTreeMap::new();
    for k in keys {
        pkgs_by_name
            .entry(&k.name)
            .or_default()
            .insert(k.pkg.as_str());
        let entry = ptr_and_val
            .entry((k.pkg.as_str(), k.name.as_str()))
            .or_default();
        if k.ptr {
            entry.0 = true;
        } else {
            entry.1 = true;
        }
    }

    let mut used_names: BTreeSet<String> = [
        "Unit", "Bool", "Int", "Float", "String", "Bytes", "List", "Option", "Result", "GoError",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let mut out = BTreeMap::new();

    // Prioritize types from the target package and pointer forms so they claim
    // the unadorned name (e.g. `Replacer` for `*strings.Replacer`).
    let mut ordered: Vec<&HandleKey> = keys.iter().collect();
    ordered.sort_by_key(|k| {
        (
            k.pkg != surface_pkg,
            !k.ptr,
            k.name.as_str(),
            k.pkg.as_str(),
        )
    });

    for key in ordered {
        let multi_pkg = pkgs_by_name
            .get(key.name.as_str())
            .is_some_and(|set| set.len() > 1);
        let both_ptr_and_val = ptr_and_val
            .get(&(key.pkg.as_str(), key.name.as_str()))
            .is_some_and(|&(has_ptr, has_val)| has_ptr && has_val);

        let mut base = if multi_pkg && key.pkg != surface_pkg {
            format!("{}{}", go_module_part(&key.pkg), key.name)
        } else {
            key.name.clone()
        };
        if both_ptr_and_val && !key.ptr {
            base.push_str("Value");
        }
        let mut candidate = base.clone();
        let mut n = 2usize;
        while !used_names.insert(candidate.clone()) {
            candidate = format!("{base}{n}");
            n += 1;
        }
        let go_form = policy
            .of_type(&key.go_form())
            .or_else(|| policy.of_type(&key.name))
            .and_then(|p| p.go_form.clone())
            .unwrap_or_else(|| key.go_form());
        out.insert(
            key.clone(),
            HandleDecl {
                flick_name: candidate,
                pkg: key.pkg.clone(),
                go_form,
            },
        );
    }
    out
}

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
