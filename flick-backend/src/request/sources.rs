//! Validate source contents once per input, then check all IR byte spans.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::iter;
use std::sync::Arc;

use flick_ir::{Expr, Program, Span};

use crate::{BuildRequest, InputKind, Inputs};

pub(super) fn validate(
    request: &BuildRequest,
    program: &Program,
    manifest: &BTreeMap<&str, InputKind>,
    inputs: &mut dyn Inputs,
) -> Result<Vec<Arc<String>>, String> {
    if request.sources.len() != program.files.len() {
        return Err("request sources differ from Program.files".into());
    }
    let mut contents = BTreeMap::new();
    let mut texts = Vec::with_capacity(request.sources.len());
    for (source, name) in request.sources.iter().zip(&program.files) {
        if source.name != *name || manifest.get(source.input.as_str()) != Some(&InputKind::Source) {
            return Err("request source has no matching declared source input".into());
        }
        let (text, lines) = match contents.entry(&source.input) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let bytes = inputs
                    .read(&source.input)
                    .map_err(|_| "declared source is unavailable")?;
                let text = String::from_utf8(bytes).map_err(|_| "request source is not UTF-8")?;
                let lines: Result<Vec<u32>, _> = iter::once(0)
                    .chain(text.match_indices('\n').map(|(i, _)| i + 1))
                    .map(u32::try_from)
                    .collect();
                entry.insert((
                    Arc::new(text),
                    lines.map_err(|_| "source exceeds the span address space")?,
                ))
            }
        };
        if *lines != source.line_starts {
            return Err("request source has an invalid line index".into());
        }
        texts.push(text.clone());
    }
    Ok(texts)
}

fn valid(span: Span, texts: &[Arc<String>]) -> bool {
    span.start <= span.end
        && texts.get(span.file.index() as usize).is_some_and(|text| {
            text.is_char_boundary(span.start as usize) && text.is_char_boundary(span.end as usize)
        })
}

fn expression(expr: &Expr, texts: &[Arc<String>]) -> bool {
    if !valid(expr.span, texts) {
        return false;
    }
    let mut checked = true;
    expr.each_child(&mut |child| checked = checked && expression(child, texts));
    checked
}

pub(super) fn spans(program: &Program, texts: &[Arc<String>]) -> Result<(), String> {
    for module in &program.modules {
        for item in &module.items {
            let mut checked = valid(item.span, texts);
            item.each_body(&mut |body| checked = checked && expression(body, texts));
            if !checked {
                return Err("backend IR span is outside its source or splits UTF-8".into());
            }
        }
    }
    Ok(())
}
