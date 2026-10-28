//! The editor's TypeScript projection of a module: a virtual `.flick.ts`
//! declaring each export, with span maps between it and the `.fl` source.
//! It is made from the same analysis as the executable code and says so in
//! its first line; it is never a source of types for Flick. A name maps
//! exactly (the same text on both sides), a type does not: only exact
//! mappings may carry a rename across languages.

use std::fmt::Write;

use serde::Serialize;

use crate::codegen::ident;
use crate::codegen::ts::Projection;
use crate::iface::{Decl, Interface, Namespace};
use crate::project::Analyzed;
use crate::syntax::kind::SyntaxKind as K;

/// Version of the projection format and of the Content Mapper protocol.
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mapping {
    /// UTF-8 byte range in the projection.
    pub ts: [u32; 2],
    /// UTF-8 byte range in the `.fl` source.
    pub fl: [u32; 2],
    /// Both ranges hold the same name.
    pub exact: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Projected {
    pub version: u32,
    pub module: String,
    /// The projection's path, relative to the source root: `App/Main.flick.ts`.
    pub file: String,
    /// The fingerprint of the analysis it was made from.
    pub analysis: String,
    pub text: String,
    pub mappings: Vec<Mapping>,
}

/// `App/Main.flick.ts` for `App.Main`.
pub fn file_of(module: &str) -> String {
    format!("{}.flick.ts", module.replace('.', "/"))
}

/// The projection of `module`, whose source is `text`.
pub fn project(
    module: &Analyzed,
    text: &str,
    analysis: &str,
    lookup: &dyn Fn(&str) -> Option<Interface>,
    package: &dyn Fn(&str) -> Option<String>,
    specifier: &dyn Fn(&str) -> String,
) -> Projected {
    let signature = |name: &str| signature(module, text, name);
    render(
        &module.interface,
        &signature,
        analysis,
        lookup,
        package,
        specifier,
    )
}

/// The projection of a module known by its interface alone, a built-in or
/// a package's: its declarations map to no source.
pub fn project_interface(
    interface: &Interface,
    analysis: &str,
    lookup: &dyn Fn(&str) -> Option<Interface>,
    package: &dyn Fn(&str) -> Option<String>,
    specifier: &dyn Fn(&str) -> String,
) -> Projected {
    let mut projected = render(interface, &|_| None, analysis, lookup, package, specifier);
    projected.mappings.clear();
    projected
}

fn render(
    interface: &Interface,
    signature: &dyn Fn(&str) -> Option<[u32; 2]>,
    analysis: &str,
    lookup: &dyn Fn(&str) -> Option<Interface>,
    package: &dyn Fn(&str) -> Option<String>,
    specifier: &dyn Fn(&str) -> String,
) -> Projected {
    let name = &interface.module;
    let projection = Projection::new(name, lookup, package);
    let (types, _) = projection.declarations(interface);
    let mut body = String::new();
    let mut pending: Vec<(usize, [u32; 2], bool)> = Vec::new();
    for line in &types {
        // `export interface Name…` or `export type Name…`
        let name = line.split_whitespace().nth(2).unwrap_or("");
        let name = name.split(['<', ' ', '=', '{']).next().unwrap_or("");
        if let Some(at) = at(interface, Namespace::Type, name) {
            let offset = body.len() + line.find(name).unwrap_or(0);
            pending.push((offset, at, true));
            pending.push((offset + name.len(), at, true));
        }
        let _ = writeln!(body, "{line}");
    }
    for export in &interface.exports {
        let entity = &export.entity;
        let value = matches!(entity.namespace, Namespace::Value | Namespace::Constructor);
        if !value
            || entity.module != *name
            || export.decl == Decl::Method
            || entity.name.contains('$')
        {
            continue;
        }
        let Some(scheme) = &export.scheme else {
            continue;
        };
        let ident = ident(&entity.name);
        let ty = projection
            .value(scheme)
            .unwrap_or_else(|reason| format!("unknown /* {reason} */"));
        let start = body.len() + "export declare const ".len();
        let _ = write!(body, "export declare const {ident}: ");
        let ty_start = body.len();
        let _ = writeln!(body, "{ty};");
        if let Some(at) = export.at {
            pending.push((start, at, ident == entity.name));
            pending.push((start + ident.len(), at, ident == entity.name));
        }
        if let Some(sig) = signature(&entity.name) {
            pending.push((ty_start, sig, false));
            pending.push((ty_start + ty.len(), sig, false));
        }
    }
    let header = format!(
        "// flick-analysis {analysis}\n{}\n",
        projection.imports(specifier)
    );
    let shift = u32::try_from(header.len()).unwrap_or(0);
    let mappings = pending
        .chunks(2)
        .filter_map(|pair| {
            let [(s, fl, exact), (e, _, _)] = pair else {
                return None;
            };
            let s = u32::try_from(*s).ok()? + shift;
            let e = u32::try_from(*e).ok()? + shift;
            Some(Mapping {
                ts: [s, e],
                fl: *fl,
                exact: *exact,
            })
        })
        .collect();
    Projected {
        version: VERSION,
        module: name.clone(),
        file: file_of(name),
        analysis: analysis.to_owned(),
        text: header + &body,
        mappings,
    }
}

fn at(interface: &Interface, namespace: Namespace, name: &str) -> Option<[u32; 2]> {
    interface.find(namespace, name)?.at
}

/// The range of the type in the signature of the top-level `name`.
fn signature(module: &Analyzed, text: &str, name: &str) -> Option<[u32; 2]> {
    let sig = module.tree.nodes().find(|d| {
        d.kind == K::Signature
            && d.child_tokens()
                .next()
                .is_some_and(|t| t.text(text) == name)
    })?;
    let (s, e) = sig.nodes().next()?.range();
    Some([s, e])
}

/// The specifier of `to`'s projection from `from`'s, both under one root.
pub fn relative(from: &str, to: &str) -> String {
    let depth = from.matches('.').count();
    let up = if depth == 0 {
        "./".to_owned()
    } else {
        "../".repeat(depth)
    };
    format!("{up}{}", file_of(to))
}
