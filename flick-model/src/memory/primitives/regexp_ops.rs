//! Counted `RegExp` primitive implementations (`packages/std/src/RegExp.fl`).

use flick_core::ir::Lit;
use flick_core::ir::memory::RegExpIntrinsic;
use regex_automata::meta::Regex;
use regex_automata::util::captures::Captures;

use super::{Data, Error, Heap, Value, charge};

pub(super) fn run(
    heap: &mut Heap,
    op: RegExpIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        RegExpIntrinsic::Make => make_regexp(heap, &args[0], &args[1], fuel),
        RegExpIntrinsic::Escape => escape_regexp(heap, &args[0], fuel),
        RegExpIntrinsic::MatchFirst => match_first(heap, &args[0], &args[1], fuel),
        RegExpIntrinsic::MatchAll => match_all(heap, &args[0], &args[1], fuel),
        RegExpIntrinsic::Search => search_regexp(heap, &args[0], &args[1], fuel),
        RegExpIntrinsic::Replace => replace_regexp(heap, &args[0], &args[1], &args[2], false, fuel),
        RegExpIntrinsic::ReplaceAll => {
            replace_regexp(heap, &args[0], &args[1], &args[2], true, fuel)
        }
    }
}

fn regexp_err(heap: &mut Heap, message: String) -> Result<Value, Error> {
    let s = heap.literal(&Lit::String(message))?;
    let err = heap.construct("RegExp.RegExpError.RegExpError", vec![s])?;
    heap.construct("Prelude.Result.Err", vec![err])
}

fn make_regexp(
    heap: &mut Heap,
    pattern_val: &Value,
    flags_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let pattern = heap.string_lossy(pattern_val)?;
    let flags = heap.string_lossy(flags_val)?;
    charge(fuel, pattern.len() + flags.len())?;
    if flags.contains('y') {
        return regexp_err(heap, "the sticky flag `y` is not supported".into());
    }
    let mut inline = String::new();
    for flag in ['i', 'm', 's'] {
        if flags.contains(flag) {
            inline.push(flag);
        }
    }
    let source = if inline.is_empty() {
        pattern.clone()
    } else {
        format!("(?{inline}){pattern}")
    };
    let compiled = match Regex::new(&source) {
        Ok(re) => re,
        Err(err) => {
            return regexp_err(
                heap,
                format!("Invalid regular expression: /{pattern}/{flags}: {err}"),
            );
        }
    };
    let global = flags.contains('g');
    let obj = heap.allocate(Data::RegExp {
        pattern,
        flags,
        global,
        compiled,
    });
    heap.construct("Prelude.Result.Ok", vec![obj])
}

fn escape_regexp(heap: &mut Heap, text_val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let text = heap.string_lossy(text_val)?;
    charge(fuel, text.len())?;
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(
            ch,
            '/' | '\\'
                | '^'
                | '$'
                | '*'
                | '+'
                | '?'
                | '.'
                | '('
                | ')'
                | '|'
                | '['
                | ']'
                | '{'
                | '}'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    heap.literal(&Lit::String(out))
}

fn extract_regexp(heap: &Heap, val: &Value) -> Result<(Regex, bool), Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("RegExp"));
    };
    let Data::RegExp {
        global, compiled, ..
    } = &heap.object(tok.id)?.data
    else {
        return Err(Error::PatternKind("RegExp"));
    };
    Ok((compiled.clone(), *global))
}

fn utf16_index(text: &str, byte_offset: usize) -> i32 {
    i32::try_from(text[..byte_offset].encode_utf16().count()).unwrap_or(i32::MAX)
}

fn build_match(heap: &mut Heap, text: &str, caps: &Captures) -> Result<Value, Error> {
    let whole = caps.get_match().expect("matched captures");
    let matched_val = heap.literal(&Lit::String(text[whole.start()..whole.end()].to_owned()))?;
    let index_val = Value::Int(utf16_index(text, whole.start()));
    let group_count = caps.group_len().saturating_sub(1);
    let mut groups = Vec::with_capacity(group_count);
    for idx in 1..=group_count {
        let item = match caps.get_group(idx) {
            Some(span) => {
                let s = match heap.literal(&Lit::String(text[span.start..span.end].to_owned())) {
                    Ok(v) => v,
                    Err(err) => {
                        heap.drop_values(groups)?;
                        heap.drop_value(matched_val)?;
                        return Err(err);
                    }
                };
                heap.construct("Prelude.Option.Some", vec![s])
            }
            None => heap.construct("Prelude.Option.None", vec![]),
        };
        match item {
            Ok(v) => groups.push(v),
            Err(err) => {
                heap.drop_values(groups)?;
                heap.drop_value(matched_val)?;
                return Err(err);
            }
        }
    }
    let captures_list = match heap.construct("$list", groups) {
        Ok(v) => v,
        Err(err) => {
            heap.drop_value(matched_val)?;
            return Err(err);
        }
    };
    heap.record(vec![
        ("captures".into(), captures_list),
        ("index".into(), index_val),
        ("matched".into(), matched_val),
    ])
}

fn match_first(
    heap: &mut Heap,
    re_val: &Value,
    text_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let (re, _) = extract_regexp(heap, re_val)?;
    let text = heap.string_lossy(text_val)?;
    charge(fuel, text.len())?;
    let mut caps = re.create_captures();
    re.captures(&text, &mut caps);
    if caps.is_match() {
        let m = build_match(heap, &text, &caps)?;
        heap.construct("Prelude.Option.Some", vec![m])
    } else {
        heap.construct("Prelude.Option.None", vec![])
    }
}

fn match_all(
    heap: &mut Heap,
    re_val: &Value,
    text_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let (re, _) = extract_regexp(heap, re_val)?;
    let text = heap.string_lossy(text_val)?;
    charge(fuel, text.len())?;
    let mut out = Vec::new();
    for caps in re.captures_iter(&text) {
        charge(fuel, 1)?;
        match build_match(heap, &text, &caps) {
            Ok(m) => out.push(m),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

fn search_regexp(
    heap: &mut Heap,
    re_val: &Value,
    text_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let (re, _) = extract_regexp(heap, re_val)?;
    let text = heap.string_lossy(text_val)?;
    charge(fuel, text.len())?;
    match re.find(&text) {
        Some(m) => heap.construct(
            "Prelude.Option.Some",
            vec![Value::Int(utf16_index(&text, m.start()))],
        ),
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

fn expand_replacement(repl: &str, text: &str, caps: &Captures) -> String {
    let whole = caps.get_match().expect("matched captures");
    let groups = caps.group_len().saturating_sub(1);
    let bytes = repl.as_bytes();
    let mut out = String::with_capacity(repl.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' || i + 1 >= bytes.len() {
            let ch = repl[i..].chars().next().expect("valid char");
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        match bytes[i + 1] {
            b'$' => {
                out.push('$');
                i += 2;
            }
            b'&' => {
                out.push_str(&text[whole.start()..whole.end()]);
                i += 2;
            }
            b'`' => {
                out.push_str(&text[..whole.start()]);
                i += 2;
            }
            b'\'' => {
                out.push_str(&text[whole.end()..]);
                i += 2;
            }
            b'0'..=b'9' => {
                let mut n = usize::from(bytes[i + 1] - b'0');
                let mut width = 1_usize;
                if i + 2 < bytes.len() && bytes[i + 2].is_ascii_digit() {
                    let two = n * 10 + usize::from(bytes[i + 2] - b'0');
                    if two >= 1 && two <= groups {
                        n = two;
                        width = 2;
                    }
                }
                if n >= 1 && n <= groups {
                    if let Some(span) = caps.get_group(n) {
                        out.push_str(&text[span.start..span.end]);
                    }
                    i += 1 + width;
                } else {
                    out.push('$');
                    i += 1;
                }
            }
            _ => {
                out.push('$');
                i += 1;
            }
        }
    }
    out
}

fn replace_regexp(
    heap: &mut Heap,
    re_val: &Value,
    repl_val: &Value,
    text_val: &Value,
    force_all: bool,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let (re, global) = extract_regexp(heap, re_val)?;
    let repl = heap.string_lossy(repl_val)?;
    let text = heap.string_lossy(text_val)?;
    charge(fuel, text.len() + repl.len())?;
    let all = force_all || global;
    let mut out = String::with_capacity(text.len());
    let mut last = 0_usize;
    for caps in re.captures_iter(&text) {
        charge(fuel, 1)?;
        let whole = caps.get_match().expect("matched captures");
        out.push_str(&text[last..whole.start()]);
        out.push_str(&expand_replacement(&repl, &text, &caps));
        last = whole.end();
        if !all {
            break;
        }
    }
    out.push_str(&text[last..]);
    heap.literal(&Lit::String(out))
}
