//! Standalone `.fln` (Flick Literal Notation) checking and formatting helpers
//! shared by CLI commands (`flick check`, `flick fmt`) and `flick lsp`.

use std::ffi::OsStr;
use std::path::Path;

use flick_core::diagnostics::{Diagnostic, codes};
use flick_core::source::{FileId, Span};
use fln::{LayoutOptions, parse_spanned, parse_spanned_recover, pretty_spanned_text_with_options};

/// Whether `path` has a `.fln` file extension.
#[must_use]
pub fn is_fln_path(path: &Path) -> bool {
    path.extension().and_then(OsStr::to_str) == Some("fln")
}

/// Whether `path` (as a CLI path string) has a `.fln` file extension.
#[must_use]
pub fn is_fln_str(path: &str) -> bool {
    is_fln_path(Path::new(path))
}

/// Parses `.fln` text with recovery and converts syntax errors to Flick diagnostics.
#[must_use]
pub fn parse_diagnostics(file: FileId, source_name: &str, text: &str) -> Vec<Diagnostic> {
    let output = parse_spanned_recover(source_name, text);
    let len = u32::try_from(text.len()).unwrap_or(u32::MAX);
    output
        .errors
        .into_iter()
        .map(|err| {
            let sp = err.span();
            let start = u32::try_from(sp.start).unwrap_or(len).min(len);
            let end = u32::try_from(sp.end).unwrap_or(len).min(len).max(start);
            let msg = if err.expected().is_empty() {
                err.message().to_owned()
            } else {
                format!("{}; expected {}", err.message(), err.expected().join(", "))
            };
            Diagnostic::error(codes::S007, msg, Span::new(file, start, end))
        })
        .collect()
}

/// Formats `.fln` text into Flick's canonical layout.
///
/// Leading and trailing comments outside the root value are preserved. Because
/// `fln_ast::SpannedValue` does not retain interior comments, a valid document
/// containing comments inside the root value is returned unchanged so formatting
/// never silently deletes comments.
///
/// # Errors
/// Returns syntax diagnostics with source spans when `text` is not valid FLN.
pub fn format_fln(file: FileId, source_name: &str, text: &str) -> Result<String, Vec<Diagnostic>> {
    let Ok(spanned) = parse_spanned(source_name, text) else {
        return Err(parse_diagnostics(file, source_name, text));
    };
    let root = spanned.span;
    let body = &text[root.start..root.end];
    if contains_comment(body) {
        return Ok(text.to_owned());
    }
    let formatted = pretty_spanned_text_with_options(&spanned, LayoutOptions::flick());
    let prefix = text[..root.start].trim_matches(['\r', '\n']);
    let suffix = text[root.end..].trim_matches(['\r', '\n']);
    let mut out = String::new();
    if !prefix.trim().is_empty() {
        out.push_str(prefix);
        out.push('\n');
    }
    out.push_str(&formatted);
    out.push('\n');
    if !suffix.trim().is_empty() {
        out.push_str(suffix);
        out.push('\n');
    }
    Ok(out)
}

/// Scans `src` outside string/raw-string/multiline-string literals for `//` or `/*`.
fn contains_comment(src: &str) -> bool {
    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'/' if i + 1 < len && (bytes[i + 1] == b'/' || bytes[i + 1] == b'*') => {
                return true;
            }
            b'"' => {
                if i + 2 < len && bytes[i + 1] == b'"' && bytes[i + 2] == b'"' {
                    i += 3;
                    while i < len {
                        if bytes[i] == b'\\' {
                            i += 2;
                        } else if i + 2 < len
                            && bytes[i] == b'"'
                            && bytes[i + 1] == b'"'
                            && bytes[i + 2] == b'"'
                        {
                            i += 3;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                } else {
                    i += 1;
                    while i < len {
                        if bytes[i] == b'\\' {
                            i += 2;
                        } else if bytes[i] == b'"' {
                            i += 1;
                            break;
                        } else {
                            i += 1;
                        }
                    }
                }
            }
            b'r' if i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_' => {
                let mut hashes = 0;
                while i + 1 + hashes < len && bytes[i + 1 + hashes] == b'#' {
                    hashes += 1;
                }
                if i + 1 + hashes < len && bytes[i + 1 + hashes] == b'"' {
                    i += 2 + hashes;
                    while i < len {
                        if bytes[i] == b'"'
                            && i + hashes < len
                            && bytes[i + 1..=i + hashes].iter().all(|b| *b == b'#')
                        {
                            i += 1 + hashes;
                            break;
                        }
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    false
}
