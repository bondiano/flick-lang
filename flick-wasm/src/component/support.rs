//! IR program construction, shared `.d.ts` projection, declared input hashing,
//! and base64 wire helpers for browser component builds.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use flick_backend::{HostExport, InlineForeign, Input, InputKind, Span as BackendSpan, Symbol};
use flick_core::build::{self, Artifact as FrontendArtifact, Settings};
use flick_core::codegen;
use flick_core::codegen::ts::Typing;
use flick_core::diagnostics::Diagnostic;
use flick_core::iface::Interface;
use flick_core::ir;
use flick_core::project::Compilation;
use flick_core::source::{FileId, SourceMap};
use flick_core::types;
use flick_ir::Program;

pub(super) fn build_program(
    sources: &mut SourceMap,
    compilation: &Compilation,
) -> (Program, Vec<FileId>) {
    let files = RefCell::new(Vec::new());
    let builtins = build::builtin_bundle_of(true);
    let builtin_files: Vec<_> = builtins
        .sources
        .iter()
        .map(|s| {
            let existing = sources
                .files()
                .position(|f| f.path == s.name && f.text == s.text);
            existing.map_or_else(
                || sources.add(&s.name, &s.text),
                |i| FileId::new(u32::try_from(i).expect("source count fits u32")),
            )
        })
        .collect();
    let mut modules: Vec<_> = compilation
        .modules
        .iter()
        .map(|m| (*m.ir).clone())
        .collect();
    for builtin in &builtins.modules {
        if compilation.modules.iter().any(|m| m.name == builtin.name) {
            continue;
        }
        let mut module = builtin.clone();
        module.each_span_mut(&mut |s| s.file = builtin_files[s.file.index() as usize]);
        modules.push(module);
    }
    let program = Program::new(
        &format!("flick {}", flick_core::VERSION),
        modules,
        ir::lang_items(),
        &|file| {
            files.borrow_mut().push(file);
            sources.get(file).path.clone()
        },
    );
    (program, files.into_inner())
}

pub(super) fn collect_inline_foreign(program: &Program) -> Vec<InlineForeign> {
    let mut out = Vec::new();
    for module in &program.modules {
        for preamble in &module.preambles {
            out.push(InlineForeign {
                module: module.name.clone(),
                name: None,
                code: preamble.code.clone(),
                span: BackendSpan {
                    file: preamble.span.file.index(),
                    start: preamble.span.start,
                    end: preamble.span.end,
                },
            });
        }
        for item in &module.items {
            if let flick_ir::ItemKind::Foreign(foreign) = &item.kind
                && let flick_ir::Binding::Inline(inline) = &foreign.binding
            {
                out.push(InlineForeign {
                    module: module.name.clone(),
                    name: Some(foreign.name.clone()),
                    code: inline.code.clone(),
                    span: BackendSpan {
                        file: inline.span.file.index(),
                        start: inline.span.start,
                        end: inline.span.end,
                    },
                });
            }
        }
    }
    out
}

pub(super) fn collect_own_host_exports(
    program: &Program,
    compilation: &Compilation,
) -> Vec<HostExport> {
    let own: BTreeSet<&str> = compilation
        .modules
        .iter()
        .filter(|m| m.package.is_none())
        .map(|m| m.name.as_str())
        .collect();
    let mut out = Vec::new();
    for module in &program.modules {
        if !own.contains(module.name.as_str()) {
            continue;
        }
        for export in &module.exports.host {
            out.push(HostExport {
                name: export.export_name.clone(),
                symbol: Symbol {
                    module: module.name.clone(),
                    name: export.name.clone(),
                },
                span: BackendSpan {
                    file: export.span.file.index(),
                    start: export.span.start,
                    end: export.span.end,
                },
            });
        }
    }
    out
}

pub(super) fn build_shared_dts(
    compilation: &Compilation,
    settings: &Settings<'_>,
    interfaces: &BTreeMap<&str, Interface>,
    builtin_names: &[&str],
    resolved: &RefCell<Vec<codegen::Resolved>>,
) -> (Vec<FrontendArtifact>, Vec<Diagnostic>) {
    let mut shared = Vec::new();
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    let module_path = |m: &str| codegen::module_path(builtin_names, false, m);
    let package = |_: &str| None;
    let interface = |m: &str| {
        interfaces.get(m).cloned().or_else(|| {
            types::builtins_of(true)
                .iter()
                .find(|i| i.module == m)
                .cloned()
        })
    };
    let foreign = |module: &str, spec: &str| {
        let specifier = build::javascript_foreign_specifier(settings, &module_path, module, spec);
        let mut all = resolved.borrow_mut();
        if !all.iter().any(|r| r.module == module && r.spec == spec) {
            all.push(codegen::Resolved {
                module: module.into(),
                spec: spec.into(),
                specifier: specifier.clone(),
            });
        }
        specifier
    };
    let typing = Typing {
        interface: &interface,
        package: &package,
        foreign: &foreign,
        path: &module_path,
        rt: true,
    };
    let own = compilation.modules.iter().map(|m| &*m.ir);
    for module in own.chain(build::builtin_modules_of(true)) {
        if !seen.insert(module.name.clone()) {
            continue;
        }
        let is_own = interfaces.contains_key(module.name.as_str());
        if let Some(iface) = (typing.interface)(&module.name) {
            let (files, issues) =
                build::javascript_shared_files(&module.name, &iface, &typing, true, false);
            shared.extend(files);
            if is_own {
                diagnostics.extend(issues);
            }
        }
    }
    (shared, diagnostics)
}

pub(super) fn declare_input(
    manifest: &mut Vec<Input>,
    bytes_map: &mut BTreeMap<String, Vec<u8>>,
    id: String,
    kind: InputKind,
    bytes: Vec<u8>,
) -> String {
    manifest.push(Input {
        id: id.clone(),
        kind,
        sha256: flick_backend::sha256(&bytes),
        size: u64::try_from(bytes.len()).expect("input size fits u64"),
    });
    bytes_map.insert(id.clone(), bytes);
    id
}

pub(crate) fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = if chunk.len() > 1 {
            u32::from(chunk[1])
        } else {
            0
        };
        let b2 = if chunk.len() > 2 {
            u32::from(chunk[2])
        } else {
            0
        };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

pub(crate) fn base64_decode(input: &str) -> Result<Vec<u8>, &'static str> {
    let bytes = input.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err("base64 length must be a multiple of 4");
    }
    let val = |b: u8| -> Result<u8, &'static str> {
        match b {
            b'A'..=b'Z' => Ok(b - b'A'),
            b'a'..=b'z' => Ok(b - b'a' + 26),
            b'0'..=b'9' => Ok(b - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err("invalid base64 character"),
        }
    };
    let mut out = Vec::with_capacity((bytes.len() / 4) * 3);
    for chunk in bytes.chunks_exact(4) {
        let v0 = u32::from(val(chunk[0])?);
        let v1 = u32::from(val(chunk[1])?);
        let pad2 = chunk[2] == b'=';
        let pad3 = chunk[3] == b'=';
        if pad2 && !pad3 {
            return Err("invalid base64 padding");
        }
        let v2 = if pad2 { 0 } else { u32::from(val(chunk[2])?) };
        let v3 = if pad3 { 0 } else { u32::from(val(chunk[3])?) };
        let word = (v0 << 18) | (v1 << 12) | (v2 << 6) | v3;
        out.push(((word >> 16) & 0xff) as u8);
        if !pad2 {
            out.push(((word >> 8) & 0xff) as u8);
        }
        if !pad3 {
            out.push((word & 0xff) as u8);
        }
    }
    Ok(out)
}
